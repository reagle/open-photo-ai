import { toast } from "sonner";
import { awaitAnalysis } from "@/hooks/useAutopilot";
import i18n from "@/i18n";
import type { CropInfo } from "@/ipc/crop";
import type { Operation, Processor } from "@/ipc/enhance";
import type { ImageRecord } from "@/ipc/images";
import type { Mask } from "@/ipc/mask";
import { chime } from "@/lib/chime";
import { runnable } from "@/lib/enhancements";
import { withChoice } from "@/lib/faces";
import { regionOf, toWire, withRemoval } from "@/lib/mask";
import { useBatchStore } from "@/stores/batch";
import { useCropStore } from "@/stores/crop";
import { useEnhancementStore } from "@/stores/enhancements";
import { useFacesStore } from "@/stores/faces";
import { guardNavigation, useFileStore } from "@/stores/files";
import { useMaskStore } from "@/stores/mask";
import { useSettingsStore } from "@/stores/settings";

// The batch is the window paging through the photographs by itself. It keeps the application's one slot - one run, one
// result, for the photograph on screen, and paging stops it - and adds nothing beside it: each photograph the batch
// reaches is made current, the view follows, the window's own run does the work, and the batch moves on when that run
// has ended. One run at a time falls out of that rather than being enforced, and what the user sees is always what is
// being worked on. What it replaced - a queue of runs in the background beside the window's - held a second working
// set in memory, and made a click wait for a background graph to finish before its own run could start.

/** The parts of a window run's request a result is a function of: what makes two requests the same work. */
export type RunRequest = {
    source: string;
    operations: Operation[];
    processor: Processor;
    crop?: CropInfo;
    mask?: Mask;
};

/**
 * A request spelt as one string, so the batch can tell whether the run that ended for a photograph is the run its
 * photograph would be asked for now. The one spelling both sides use: `useEnhancementRun` keys what it records with it
 * and {@link requestOf} what it compares, so the two cannot drift apart field by field.
 */
export const keyOf = ({ source, operations, processor, crop, mask }: RunRequest) =>
    JSON.stringify({ source, operations, processor, crop: crop ?? null, mask: mask ?? null });

/**
 * What the window's run would send for `file` were it shown now, keyed, or `undefined` for a photograph with nothing
 * to run.
 *
 * **The same composition as `useEnhancementRun`'s**, read off the stores rather than subscribed to: the stack less
 * what an enlarging restore suspends, the choice among faces, the removal's region, the framing and the limiting mask.
 */
export const requestOf = (file: ImageRecord): (RunRequest & { key: string }) | undefined => {
    const source = file.identity;
    if (source === undefined) return;

    const stack = runnable(useEnhancementStore.getState().enhancements.get(file.path) ?? []);
    if (stack.length === 0) return;

    const masks = useMaskStore.getState();
    const choice = useFacesStore.getState().choices.get(source);
    const operations = withRemoval(withChoice(stack, choice), regionOf(masks.removals.get(file.path)));
    const crop = useCropStore.getState().crops.get(source);
    const mask = toWire(masks.masks.get(file.path));
    const processor = useSettingsStore.getState().processor;
    const request = { source, operations, processor, ...(crop && { crop }), ...(mask && { mask }) };

    return { ...request, key: keyOf(request) };
};

/**
 * Whether `file` has nothing on it yet that Autopilot, being on, would suggest something for - a photograph the batch
 * has to show before it can know whether there is work on it. Not one the batch has already waited on.
 */
const awaitingAutopilot = (file: ImageRecord) =>
    useEnhancementStore.getState().autopilot &&
    file.identity !== undefined &&
    !useEnhancementStore.getState().enhancements.has(file.path) &&
    !useBatchStore.getState().analysed.has(file.path);

/** Where `file` stands in a batch: work to do, done (or failed: the batch does not try again), or nothing to do. */
const standingOf = (file: ImageRecord): "unfinished" | "done" | "failed" | undefined => {
    const request = requestOf(file);
    if (!request) return awaitingAutopilot(file) ? "unfinished" : undefined;

    const outcome = useBatchStore.getState().outcomes.get(file.path);
    if (outcome?.key !== request.key) return "unfinished";

    return outcome.failed ? "failed" : "done";
};

/** The batch's photographs in the strip's order, each with where it stands. */
const walk = () => {
    const { members } = useBatchStore.getState();

    return useFileStore
        .getState()
        .files.filter((file) => members.has(file.path))
        .map((file) => ({ file, standing: standingOf(file) }));
};

/** How far the batch has got: how many it has finished (failures included), and how many are left. */
export const progressOf = () => {
    const walked = walk();
    const done = walked.filter(({ standing }) => standing === "done").length;
    const failed = walked.filter(({ standing }) => standing === "failed").length;
    const left = walked.filter(({ standing }) => standing === "unfinished").length;

    return { done, failed, left, total: done + failed + left };
};

/**
 * How many open photographs a "Process all" would go through: those with work to do that the window has not finished
 * for the request they would send now.
 */
export const unprocessedCount = () =>
    useFileStore.getState().files.filter((file) => standingOf(file) === "unfinished").length;

/** Makes `path` the photograph on screen on the batch's own account: past the guard, which is for the user's choices. */
const show = (path: string) => {
    const index = useFileStore.getState().files.findIndex((file) => file.path === path);
    if (index < 0) return;

    useBatchStore.getState().at(path);
    if (useFileStore.getState().currentIndex !== index) useFileStore.setState({ currentIndex: index });
};

/** The stores whose change can change a photograph's request, and with it whether the batch is waiting on it still. */
const WATCHED = [useBatchStore, useFileStore, useEnhancementStore, useCropStore, useFacesStore, useMaskStore];

/**
 * Resolves once the batch need not wait on `file` any longer: the window's run has ended for the request the
 * photograph would send now (`done`), the photograph has nothing to run or was closed (`done` too: move on), the
 * batch was paused (`paused`), or something other than the batch moved the view off the photograph (`moved`).
 */
const settled = (file: ImageRecord) =>
    new Promise<"done" | "paused" | "moved">((resolve) => {
        const unsubscribes: (() => void)[] = [];
        const look = () => {
            const { phase } = useBatchStore.getState();
            const { files, currentIndex } = useFileStore.getState();
            const open = files.find((candidate) => candidate.path === file.path);
            const answer =
                phase !== "running"
                    ? "paused"
                    : !open || standingOf(open) !== "unfinished"
                      ? "done"
                      : files[currentIndex]?.path !== file.path
                        ? "moved"
                        : undefined;
            if (answer === undefined) return;

            for (const unsubscribe of unsubscribes) unsubscribe();
            resolve(answer);
        };

        for (const store of WATCHED) {
            unsubscribes.push((store.subscribe as (listener: () => void) => () => void)(look));
        }
        look();
    });

/** Tells the user the batch is through: a notice in the window, a sound, and the Dock's attention. */
const announce = () => {
    const { done, failed } = progressOf();
    const message =
        failed > 0
            ? i18n.t("batch.finishedWithFailures", { count: done + failed, failed })
            : i18n.t("batch.finished", { count: done });

    toast.success(message, { duration: 10_000 });
    chime();
};

let driving = false;

/**
 * Pages through the batch until it is paused or through, and does nothing if it is already doing that.
 *
 * Each photograph is chosen afresh from the strip, so one closed, edited or added while the batch runs is taken into
 * account at the next step. A photograph Autopilot has not yet looked at is shown and its analysis waited for, as an
 * export waits for it, and passed over if it suggested nothing. One whose run failed is left marked failed, and the
 * batch moves on: the window has already said why.
 */
export const drive = async () => {
    if (driving) return;
    driving = true;

    try {
        while (useBatchStore.getState().phase === "running") {
            const next = walk().find(({ standing }) => standing === "unfinished")?.file;
            if (!next) {
                useBatchStore.getState().finish();
                announce();
                break;
            }

            show(next.path);

            if (awaitingAutopilot(next)) {
                await awaitAnalysis(next, useCropStore.getState().crops.get(next.identity ?? ""));
                useBatchStore.getState().markAnalysed(next.path);
                continue;
            }

            // Moved off by something other than the user's choice - a click is asked about first - so the batch stops
            // where it is rather than dragging the view back. A pause needs nothing more: the loop looks at the phase
            // again, so a resume that came before this loop noticed the pause carries straight on.
            if ((await settled(next)) === "moved") useBatchStore.getState().pause();
        }
    } finally {
        driving = false;
        useBatchStore.getState().at(undefined);
    }
};

/** Starts the batch over `paths` - added to it, if one is under way - and sets it going. */
export const startBatch = (paths: readonly string[]) => {
    if (paths.length === 0) return;

    useBatchStore.getState().start(paths);
    void drive();
};

/** Starts a batch over every open photograph: the "Process all" button. */
export const processAll = () => startBatch(useFileStore.getState().files.map((file) => file.path));

/** Pauses the batch where it is. The photograph on screen stays there, and its run carries on as any shown one's. */
export const pauseBatch = () => useBatchStore.getState().pause();

/** Resumes the batch from its next unfinished photograph, which it makes current. */
export const resumeBatch = () => {
    useBatchStore.getState().resume();
    void drive();
};

/** Answers the question a click during the batch asked: pause and go to the photograph, or keep processing. */
export const answerPause = (pause: boolean, dontAskAgain: boolean) => {
    const { asking } = useBatchStore.getState();
    useBatchStore.getState().ask(undefined);
    if (!pause || asking === undefined) return;

    if (dontAskAgain) useSettingsStore.getState().stopAskingBeforePause();
    pauseBatch();
    useFileStore.getState().setCurrentIndex(asking);
};

/*
 * The user's own choice of photograph while the batch runs is asked about first, unless they said not to ask: the
 * batch is what is on screen, and going elsewhere stops the run it is waiting on.
 */
guardNavigation((index) => {
    const { phase } = useBatchStore.getState();
    if (phase !== "running" || index === useFileStore.getState().currentIndex) return true;

    if (useSettingsStore.getState().pauseBatchWithoutAsking) {
        pauseBatch();
        return true;
    }

    useBatchStore.getState().ask(index);
    return false;
});

/** What the strip says about a photograph: see {@link badgeOf}. */
export type Badge = "queued" | "processing" | "done" | "failed";

/**
 * What the strip's badge on `file` says, or `undefined` for a photograph with nothing to say.
 *
 * **Processing** only while the window's run over it has reported working - the one photograph actually computing;
 * **done** or **failed** when its last run ended so for the request it would send now; **queued** for a photograph the
 * batch has still to reach, and for the one on screen while its run has not started. A photograph outside any batch
 * and not shown says nothing until it is.
 */
export const badgeOf = (file: ImageRecord): Badge | undefined => {
    const standing = standingOf(file);
    if (standing === undefined) return;

    const { onScreen, onScreenStarted, members, phase } = useBatchStore.getState();
    if (onScreen === file.path) return onScreenStarted ? "processing" : "queued";
    if (standing !== "unfinished") return standing;

    return (phase === "running" || phase === "paused") && members.has(file.path) ? "queued" : undefined;
};
