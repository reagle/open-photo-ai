import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { type CropInfo, sameCrop } from "@/ipc/crop";
import { cancelEnhance, enhance, onEnhanceProgress, type RunProgress } from "@/ipc/enhance";
import type { Face } from "@/ipc/faces";
import type { ImageRecord } from "@/ipc/images";
import { runnable } from "@/lib/enhancements";
import { withChoice } from "@/lib/faces";
import { refineOf, regionOf, toWire, withRefine, withRemoval } from "@/lib/mask";
import { report } from "@/lib/report";
import { useFileEnhancements } from "@/stores/enhancements";
import { useFaceChoice, useFacesStore } from "@/stores/faces";
import { useFileMask } from "@/stores/mask";
import { useSettingsStore } from "@/stores/settings";

/**
 * How long a change that is only to the mask waits for another before it re-runs, in milliseconds: one run per pause in
 * a flurry of strokes.
 */
const MASK_DEBOUNCE_MS = 300;

/**
 * How long a painted removal waits for the next stroke before it is run, in milliseconds. Unlike the limiting mask's
 * wait, this one leads to models: a removal is the chain's first step, so a new region recomputes it and everything
 * after it, and a run already computing is stopped for it - one run per pause, not one per stroke.
 */
const REMOVAL_DEBOUNCE_MS = 300;

/**
 * `entry`, once it has stayed the same for {@link REMOVAL_DEBOUNCE_MS} - or at once, for another photograph than the
 * one last settled, and for one whose region was taken away or emptied.
 *
 * **Keyed by the photograph**: a value left from the photograph before must never be sent with this one's pixels, so a
 * change of photograph answers the new one's region at once. A region taken away (a crop change, which re-runs for the
 * new framing in the same render) or cleared has nothing to compute and is not waited for either.
 */
const useSettledRegion = <T extends { strokes: readonly unknown[] }>(
    path: string | undefined,
    entry: T | undefined,
) => {
    const [settled, setSettled] = useState<{ path: string | undefined; entry: T | undefined }>({ path, entry });
    const now = entry === undefined || entry.strokes.length === 0;

    useEffect(() => {
        if (settled.path === path && settled.entry === entry) return;
        if (now || settled.path !== path) {
            setSettled({ path, entry });
            return;
        }

        const timer = setTimeout(() => setSettled({ path, entry }), REMOVAL_DEBOUNCE_MS);

        return () => clearTimeout(timer);
    }, [path, entry, now, settled]);

    return settled.path !== path || now ? entry : settled.entry;
};

/** Where an enhanced result's pixels are, and how large they are. */
export type EnhancedImage = { identity: string; width: number; height: number };

/** What the canvas needs while and after a run: the result to draw, and what to say about the run. */
export type EnhancementRun = {
    /**
     * The result the enhanced pane draws, or `undefined` while it should draw the source.
     *
     * Absent for an image with no enhancements, for one whose first run has not landed yet, and for
     * one whose run was stopped before it produced anything.
     */
    enhanced?: EnhancedImage;
    /**
     * Whether a run is in flight, which is true from the moment one is asked for.
     *
     * What the indicator is drawn on, rather than {@link EnhancementRun.report}: a run spends its
     * first seconds loading a model and reports nothing while it does, and an indicator that waited
     * for the first report appeared long after the enhancement was added.
     */
    running: boolean;
    /** The latest report about the run in flight, or `undefined` while none has arrived yet. */
    report?: RunProgress;
    /**
     * How far along the indicator is, in `0..1`: the report's own {@link RunProgress.chainFraction}.
     *
     * One number for a face recovery too. Rust finds the faces inside the run and maps that detection
     * into the first enhancement's equal slot of the bar (`opai::ProcessOptions::faces`, in
     * `crates/opai/src/inference/handover.rs`), so the bar moves forward once. Zero for a run that has
     * reported nothing yet.
     */
    fraction: number;
};

/**
 * Runs the current image's enhancements and answers what the canvas should draw and report.
 *
 * ```
 *   useEnhancementRun(file, crop)
 *       |                                        cleanup: cancelEnhance(run)
 *       enhance(identity, ops + face choice, proc, crop) ---------------+
 *       |                                                                |
 *       +-- done -> { outcome: "enhanced", identity, w, h, faces? } -----+--> enhanced pane, faces store
 *       |           { outcome: "stopped" }  -> nothing
 *       +-- rejects -> notice (errors.enhanceFailed)
 *       |
 *       +-- onEnhanceProgress -> report.run === mine ? keep : discard --> bar
 * ```
 *
 * **A face recovery finds its own faces.** The chain is asked for with the choice among faces - the
 * user's exceptions to each face's default, by key - and Rust detects inside that one run, hands the
 * recovery the faces the choice keeps, and answers every face found beside the result. So there is no
 * detect-then-enhance sequence here: one request, one progress stream, one stop. The faces answered
 * are recorded for the row to count and the picker to offer, and recording them re-runs nothing - the
 * choice is the dependency, and a detection never writes one. A detection that failed is reported
 * with its own notice; the chain still ran, over no faces. See design.md D4 and D10.
 *
 * **A photograph that has been closed is not written.** An answer settling late for a photograph the
 * window has moved off is still recorded, a warm entry for when it comes back; a photograph that was
 * *closed* has already had every owner told to forget it, and putting an entry back for it would outlive
 * the only thing that empties the store.
 *
 * **A run starts when the stack, the image, the framing, the processor, the choice of faces or the mask changes**,
 * and stops the one it replaces - except for the mask, which is gentler: see "A mask is waited for" below. A choice is
 * one write however many boxes were clicked, so a chooser opened and applied costs one run - and applying a choice that
 * changes nothing writes nothing, so it costs none. The backend enforces the displacement rather than trusting this
 * side to ask, so the explicit stop in the cleanup is for the case a new run does not cover: removing the last
 * enhancement, or moving to a photograph with none.
 *
 * **A mask is waited for.** A stroke re-runs only the blend, the models behind it being served from the cache, but only
 * once a run has computed them. A change that is only to the mask therefore waits {@link MASK_DEBOUNCE_MS} for another,
 * so a flurry of strokes costs one run; and it never stops a run whose models are still computing - the first, uncached
 * run of a chain, or any run a change of anything else started - because stopping it would throw that work away. That
 * run finishes, and the latest mask is run once it has: cheap, and the one a stop is then free to displace.
 *
 * **An empty stack asks for nothing.** The seam accepts an empty chain and answers with the source,
 * but that is a round trip for an answer the window already holds - so the pane is pointed at the
 * source directly, and whatever was in flight is still stopped.
 *
 * **No store.** The authoritative single-run slot is in Rust, and a store here would be a second copy
 * of it that can disagree. Nothing outside the canvas reads the run: the sidebar's rows describe what
 * *will* be run, not what is running.
 *
 * **Called once**, from the component that draws both panes and the bar.
 *
 * # What a stop leaves on screen
 *
 * A result is held until a newer one lands rather than cleared when a run starts, which is what the
 * spec asks for: a stopped run leaves the view showing what it was showing, and never a
 * partly-enhanced image. So typing in the scale field re-runs on every keystroke while the last
 * result stays on screen, instead of flashing the source between them.
 *
 * It is dropped in the three cases where holding it would be a lie: the stack emptying, the image
 * changing, and the **framing** changing. A result belongs to the photograph it was made from, which
 * is why it is held beside the identity *and the crop* it was made from rather than on its own.
 *
 * A crop takes the image-change behaviour rather than the enhancement-change behaviour, deliberately,
 * and this is where this application diverges from the reference - which keeps the previous result on
 * screen. Changing an enhancement keeps its result because the shape of what is drawn does not
 * change; changing a crop changes the pane's box in the same render, so holding the previous result
 * would draw it to the wrong proportions until the re-run landed. See design.md D10.
 *
 * **A stopped run writes nothing at all.** Slice 1's D2 made a displaced run answer `stopped`
 * whatever it returned, precisely so this can branch on the outcome and never ask whether the run it
 * is holding is still the current one. A rejection raises the notice; a stop does not, because a
 * user who changed their mind has not been told their enhancement broke.
 */
export const useEnhancementRun = (file: ImageRecord | undefined, crop?: CropInfo): EnhancementRun => {
    const { t } = useTranslation();

    const source = file?.identity;
    const stack = useFileEnhancements(file?.path);
    /*
     * What runs: the stack, less an upscale an enlarging restore has taken out of the chain. Memoized on the
     * stack, because the run effect keys on this array - a fresh one per render would restart the run.
     */
    const operations = useMemo(() => runnable(stack), [stack]);
    /*
     * The processor in force, which is all this store ever holds.
     *
     * It is a dependency of the run effect below, so a change to it cancels the run and starts a new
     * one - correct for a saved preference, and wrong for every keystroke on the way to one. While
     * the settings store held drafts this subscribed to the draft: clicking through the Performance
     * radio with the dialog open restarted the inference run on every click, and Cancel restarted it
     * once more. Nothing here had to change to fix that; the store stopped publishing drafts.
     */
    const processor = useSettingsStore((state) => state.processor);

    /*
     * The choice made among this photograph's faces, and the reason this is a dependency rather than
     * something the effect reads off the store: it is replaced whole on a write and is the same
     * reference until one happens, so the chain re-runs exactly when a choice is applied and at no other
     * time. That is the whole of "a committed change re-runs the preview, once" - it falls out of this
     * effect instead of needing a second one. See design.md D7.
     */
    const choice = useFaceChoice(source);

    /*
     * What the user has painted over this photograph, as the run is sent it - or nothing, for a mask that paints
     * nothing.
     *
     * Compared by the entry the store holds: it is replaced on every committed change and is the same object until the
     * next one, which is the store's version counter by another name. A stroke is committed when the pointer comes up,
     * so one being drawn is in no entry and re-runs nothing. Memoised on the entry so the run effect sees one mask
     * between changes, not a new object per render.
     */
    const painted = useFileMask(file?.path);
    const mask = useMemo(() => toWire(painted), [painted]);

    /*
     * What Remove is to take out: the strokes painted for it, settled for a moment, as the wire carries them - an empty
     * region where nothing is painted, since a removal is sent whatever it holds. A dependency of the run like the
     * stack, not like the mask: a new region is new work for the models. Memoised on the entry, as the mask is.
     */
    const removing = useSettledRegion(file?.path, useFileMask(file?.path, "removal"));
    const region = useMemo(() => regionOf(removing), [removing]);

    /*
     * The Lens Blur's Refine brush, settled as the removal's region is, and a dependency of the run likewise: a stroke
     * changes the trimap the matting model is asked over, and the render. The depth and Bilbao's matte are served
     * from the run cache, and so is the matte wherever a stroke left the trimap as it was. Nothing for a photograph
     * with nothing painted, whose run is the one it always was.
     */
    const refining = useSettledRegion(file?.path, useFileMask(file?.path, "refine"));
    const refine = useMemo(() => refineOf(refining), [refining]);

    /*
     * What the mask-only re-run is built from, all read in the run effect rather than listed as its dependencies - a
     * mask change must not restart it on its own, which is the whole of the wait below.
     */
    /* The latest mask, which any run starts with whatever started it. */
    const latest = useRef(mask);
    latest.current = mask;
    /* The mask the run in flight, or the last one, was started with: what `latest` is compared with. */
    const ran = useRef(mask);
    /* Whether the run in flight is still computing models, which is what a mask change leaves be. */
    const computing = useRef(false);
    /* Set by whoever asks for a re-run for the mask, and read by the run it starts: that run computes no models. */
    const forMask = useRef(false);
    /* Counted up to ask the run effect for a re-run of the same chain over the latest mask. */
    const [rerun, setRerun] = useState(0);

    /*
     * The last result, beside the photograph it was made from.
     *
     * The source travels with it rather than being cleared by an effect keyed on the identity,
     * which is what makes "a result belongs to the photograph it was made from" a fact about the
     * value rather than a race between two effects: a render that has moved to another image simply
     * does not match, so there is no frame in which one image's enhancement is drawn over another's.
     */
    const [result, setResult] = useState<{ source: string; crop?: CropInfo; image: EnhancedImage }>();
    /* What the bar is drawing: the last report this window kept. */
    const [indicator, setIndicator] = useState<RunProgress>();

    /*
     * Set in the same turn the run is asked for, so the indicator is on screen before the backend
     * has said anything at all. `mine` cannot answer this: it is a ref precisely so the progress
     * listener can read it without re-registering, and a ref does not re-render.
     */
    const [running, setRunning] = useState(false);

    // The framing is compared by value, through `sameCrop`: a result made at the framing in force is
    // this framing's result, whichever object the crop store happens to hand back for it.
    const enhanced = result && result.source === source && sameCrop(result.crop, crop) ? result.image : undefined;

    /*
     * The run this window is drawing, read by the progress listener - which is registered once and
     * must not close over a value that changes with every keystroke in the scale field.
     */
    const mine = useRef<string | undefined>(undefined);

    /*
     * The notice's own sentence, through a ref rather than as a dependency of the run effect below.
     * `t` changes identity when the language does, and a language change must not cancel an
     * inference run and start it again - which is exactly what listing it there would do. The
     * listener in `useDroppedImages` lists it because re-registering a listener costs nothing.
     */
    const notice = useRef(t);
    notice.current = t;

    /*
     * One subscription for the application, as `ipc/enhance.ts` describes: every report names its
     * own run, so a report about a run this window has abandoned is discarded rather than drawn.
     * A displaced run goes on reporting until it notices it has been stopped.
     */
    useEffect(() => {
        const listening = onEnhanceProgress((incoming) => {
            if (incoming.run !== mine.current) return;

            setIndicator(incoming);
        });

        /*
         * A subscription that could not be registered leaves the bar unreported rather than
         * unhandled. `listen` reads a global Tauri's init script installs, so it rejects wherever
         * that script has not run - a webview torn down mid-registration, and every test rendering
         * a preview without the event bridge stubbed. Caught here rather than left to the cleanup
         * below, which only runs if the component is still around to unmount: an unmount before the
         * registration settles would otherwise leave the rejection with nobody to answer it.
         *
         * The same handling the stop in the run effect below gets, and for the same reason: there
         * is nobody to propagate a dead bridge to from an effect, and the next run discovers it.
         */
        listening.catch((error: unknown) => report("subscribing to enhancement progress failed", error));

        return () => {
            // Registered asynchronously, so the unregistration has to wait for it - otherwise a
            // reloaded webview in development accumulates a listener per reload. A registration
            // that failed has nothing to unregister, and its reason is already reported above.
            listening.then((unlisten) => unlisten()).catch(() => {});
        };
    }, []);

    // biome-ignore lint/correctness/useExhaustiveDependencies: `rerun` is a request nothing in here reads
    useEffect(() => {
        mine.current = undefined;

        const cheap = forMask.current;
        forMask.current = false;

        if (source === undefined || operations.length === 0) {
            computing.current = false;
            setRunning(false);
            setResult(undefined);
            setIndicator(undefined);
            return;
        }

        setIndicator(undefined);

        // The mask as it is now, whatever started the run: a stack that changed while strokes were waiting takes them
        // along.
        const sent = latest.current;
        ran.current = sent;
        computing.current = !cheap;

        const chain = withRefine(withRemoval(withChoice(operations, choice), region), refine);
        const { run, done } = enhance(source, chain, processor, crop, sent);
        mine.current = run;
        setRunning(true);

        // Only for the run that is still current: an older run resolving after a newer one started
        // would otherwise take the bar down over a run that is still working, and wipe the newer
        // run's report out from under it.
        const ended = () => {
            if (mine.current !== run) return;

            computing.current = false;
            setRunning(false);
            setIndicator(undefined);
        };

        /*
         * Asks for the run a mask change was held back for, once this one - which was computing models - is over. The
         * mask that is now in force is not the one this run was given only if a change arrived while it worked.
         */
        const catchUp = () => {
            if (mine.current !== run || latest.current === ran.current) return;

            forMask.current = true;
            setRerun((count) => count + 1);
        };

        /*
         * What a chain carrying a face recovery found, recorded for the row and the picker - only while
         * there is still a photograph to hold it for. Read off the store rather than subscribed to: what
         * this needs is the writer.
         */
        const record = (found: Face[]) => useFacesStore.getState().recordFaces(source, crop, found);

        done.then(
            (outcome) => {
                const current = mine.current === run;
                ended();

                if (outcome.outcome !== "enhanced") return;

                setResult({
                    source,
                    ...(crop && { crop }),
                    image: { identity: outcome.identity, width: outcome.width, height: outcome.height },
                });
                if (current) catchUp();

                if (outcome.faces) record(outcome.faces);

                /*
                 * Recorded as no faces rather than left unanswered, and the chain ran anyway: refusing it
                 * would have meant a failed detection also cancelled an upscale the user asked for in the
                 * same list. The notice says what failed, as it always has. See design.md D10.
                 */
                if (outcome.facesError !== undefined) {
                    report("detecting the faces in the image failed", outcome.facesError);
                    toast.error(notice.current("errors.faceDetectFailed"));
                    record([]);
                }

                /*
                 * The lens blur rendered from its depth alone, and the result is kept: the subject's
                 * edges are only softer. A matting model that is not installed sends nothing here.
                 */
                if (outcome.matteError !== undefined) {
                    report("matting the lens blur's subject failed", outcome.matteError);
                    toast.error(notice.current("errors.matteFailed"));
                }
            },
            (error: unknown) => {
                ended();

                // The reason in full is reported and the catalogue's own sentence goes to
                // the user, which is what every failed IPC call in this application does: a toast
                // fades before anyone copies a diagnostic out of it.
                report("enhancing the image failed", error);
                toast.error(notice.current("errors.enhanceFailed"));
            },
        );

        return () => {
            cancelEnhance(run).catch((error: unknown) =>
                // A stop that cannot be delivered means the bridge is gone, which the next run would
                // discover anyway. There is nobody to propagate it to from a cleanup.
                report("stopping the enhancement failed", error),
            );
        };
        // The crop is one of these for the same reason the image and the stack are: changing it stops
        // the run in flight and starts one over the new framing, because the run in flight is
        // producing pixels for a framing nobody is looking at any more.
        //
        // The choice among faces is one of these because it is what the run is asked to keep. It changes
        // only when a person applies one - a detection landing records faces and never writes a choice -
        // so nothing else re-runs on its account.
        //
        // The mask is not one of these, but `rerun` is: a committed stroke, an undo, an invert or a clear changes the
        // mask the chain is limited to, and the effect below asks for a run for it once it has waited out the next
        // change. The models behind that run are served from the cache, so what it costs is the blend.
        //
        // The region a removal takes out is one of these: it is part of what a model computes, the removal's. So is the
        // Lens Blur's refinement, which the matting model is asked over.
    }, [source, operations, processor, crop, choice, region, refine, rerun]);

    /*
     * A change that is only to the mask asks for a re-run after a pause - and not at all while the run in flight is
     * still computing models, which asks for it itself when it ends. A change that came with another one has already
     * been taken by the effect above, which left `ran` equal to `latest`.
     */
    useEffect(() => {
        if (mask === ran.current || computing.current) return;

        const timer = setTimeout(() => {
            if (latest.current === ran.current) return;

            forMask.current = true;
            setRerun((count) => count + 1);
        }, MASK_DEBOUNCE_MS);

        return () => clearTimeout(timer);
    }, [mask]);

    return {
        running,
        // Zero for a run that has reported nothing yet, which is what the bar draws while a model
        // loads - the one honest thing to say about a run that has not spoken.
        fraction: indicator?.chainFraction ?? 0,
        ...(enhanced && { enhanced }),
        ...(indicator && { report: indicator }),
    };
};
