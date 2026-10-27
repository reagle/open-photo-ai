import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Progress } from "@/components/ui/progress";
import { useCatalogue } from "@/hooks/useCatalogue";
import type { Operation, PreparingProvider, RunProgress } from "@/ipc/enhance";
import { preparingName, progressLabelKey } from "@/lib/enhancements";
import { toPercent } from "@/lib/utils";

/**
 * How long a session has to have been opening before the chip says so.
 *
 * A session the provider has already compiled opens in about a second - Kazan's took 0.7 s on CoreML once compiled -
 * and saying "this can take a few minutes" for that would be a scary message flashed for nothing. A first compile is
 * minutes. Anything between is a wait long enough to deserve an explanation and short enough that the explanation
 * costs nothing when it turns out to be generous.
 */
export const PREPARING_NOTICE_DELAY_MS = 3000;

/**
 * The providers that compile a model the first time a session is opened on it, which is the wait the notice explains:
 * CoreML into a model specialised for this machine's GPU and Neural Engine, TensorRT into an engine for this GPU. Both
 * cache the result, so it happens once. CUDA, WebGPU and the CPU open a session in seconds at most, every time.
 */
const COMPILES: ReadonlySet<PreparingProvider> = new Set(["CoreML", "TensorRT"]);

/**
 * Whether `key` has stayed the same, and defined, for `delay` milliseconds.
 *
 * Keyed rather than a boolean, so that a second preparation following the first - the next operation of the chain, or
 * the CPU after a provider would not open the model - waits out its own delay rather than inheriting the first one's.
 */
const useLingering = (key: string | undefined, delay: number) => {
    const [lingered, setLingered] = useState<string>();

    useEffect(() => {
        if (key === undefined) return;

        const timer = setTimeout(() => setLingered(key), delay);

        return () => clearTimeout(timer);
    }, [key, delay]);

    return key !== undefined && lingered === key;
};

/** How often a bar filling against an estimate moves: a compile of minutes has no use for a finer step. */
export const ESTIMATE_TICK_MS = 1000;

/**
 * How long `key` has stayed the same, and defined, in milliseconds - counted from when it became that, and brought up
 * to date every {@link ESTIMATE_TICK_MS}. `0` while it is undefined, and again from the start for a new key.
 */
const useElapsed = (key: string | undefined) => {
    const [since, setSince] = useState<{ key: string; at: number }>();
    const [now, setNow] = useState(() => Date.now());

    useEffect(() => {
        if (key === undefined) return;

        setSince({ key, at: Date.now() });
        setNow(Date.now());
        const timer = setInterval(() => setNow(Date.now()), ESTIMATE_TICK_MS);

        return () => clearInterval(timer);
    }, [key]);

    return key !== undefined && since?.key === key ? Math.max(0, now - since.at) : 0;
};

/** Where a bar filling against an estimate stands once the estimate has run out. */
const AT_ESTIMATE = 0.9;
/** The most a bar filling against an estimate ever shows: the rest is the session actually opening. */
const CEILING = 0.95;

/**
 * How far to fill a bar for a wait expected to take `expectedMs`, `elapsedMs` into it.
 *
 * Linear up to {@link AT_ESTIMATE} at the estimate, because the estimate is the last compile of the same model on the
 * same machine and is usually close. Past it, approaching {@link CEILING} and never reaching it - halfway there for
 * every further half of the estimate - so a compile slower than the last one is a bar still creeping rather than one
 * standing full, and the end of the bar is kept for the session actually opening. An estimate of nothing - a request
 * that joined a compile already past its estimate - starts at the ceiling's edge rather than dividing by zero.
 */
export const estimatedFraction = (elapsedMs: number, expectedMs: number) => {
    const through = expectedMs > 0 ? elapsedMs / expectedMs : Number.POSITIVE_INFINITY;

    if (through <= 1) return AT_ESTIMATE * through;

    return CEILING - (CEILING - AT_ESTIMATE) * 2 ** (-(through - 1) / 0.5);
};

/** The whole minutes a wait of `ms` is said to take: rounded, and never fewer than one. */
export const estimatedMinutes = (ms: number) => Math.max(1, Math.round(ms / 60_000));

/**
 * What the chip says about a session that has been opening for a while: which model, and whether it is being compiled
 * (the first time) or loaded. The machine is not named: what matters is the wait, not what it is for.
 *
 * A component of its own so the catalogue the model's name is read from is asked for only here, the one place that
 * needs it, rather than on every render of every run.
 */
const PreparingLabel = ({
    report,
    compiles,
    operations,
}: {
    report: RunProgress;
    compiles: boolean;
    operations: readonly Operation[];
}) => {
    const { t } = useTranslation();
    const families = useCatalogue();

    const model = preparingName(t, families, operations, report.family) ?? t("preview.progress.enhancing");

    return t(compiles ? "preview.progress.compiling" : "preview.progress.loading", { model });
};

/**
 * What the run is doing, over the preview: how far the whole chain has got, and what the enhancement it
 * is on is doing - *Upscaling*, *Restoring faces* - or the fetch's own percentage while a model is
 * being installed.
 *
 * **Drawn from the moment a run is asked for, before it has reported anything.** A run spends its
 * first seconds loading a model and arranging the work, and nothing reports during that - so a bar
 * that waited for the first report appeared long after the enhancement was added, and the interval
 * in between read as the application having ignored the click. An unreported run is drawn empty and
 * labelled generically, which is the one honest thing to say about it: something is running and it
 * has not said what yet.
 *
 * The cost is that a chain made entirely of results the application already had now flashes the bar
 * for as long as that chain takes, rather than never drawing it. That is the trade the reverse
 * asks for: the case it makes legible is every real run, and the case it spoils finishes too fast to
 * read either way.
 *
 * **The activity is named from the report's own family**, in the language the rest of the window
 * is speaking. The report's `operation` is the library's composed English sentence, `Kyoto 4x
 * (FP16)`: right for a log, and not something a front end can translate. An activity rather than the
 * enhancement's name because a chain runs several under one bar, and the label is what says which
 * one the time is going into.
 *
 * **As wide as its label, and never narrower than the design's 148px.** An activity is a phrase
 * rather than a word in several languages - *Restauration des visages* - and a fixed width would clip
 * it or wrap it. So the label is laid out in the flow and the bar is drawn behind it, rather than the
 * other way round.
 *
 * **A fetch is spelled out with its own percentage.** The bar tracks the whole chain, and a fetch
 * occupies only the head of one operation's share of it - so during a multi-gigabyte download the
 * bar barely moves, and without the number it reads as a stall. That is the reference's own
 * correction, and the reason the report carries two figures.
 *
 * **A family the catalogue publishes and `ENHANCEMENTS` does not name falls back to the generic
 * label**, which is the same label an unreported run carries. It is the same class of drift
 * `lib/enhancements.ts` answers for a model the catalogue stops publishing, and the same answer:
 * draw something honest rather than throw.
 *
 * **The position is given, not read off the report.** `report.chainFraction` describes the run the
 * report came from, and a face recovery is two runs - a detection and then the chain - each
 * reporting its own `0..1`. `hooks/useEnhancementRun.ts` composes the one figure that spans both;
 * drawing the report's own filled the bar, emptied it and filled it again.
 *
 * **A detection reads as Face Recovery**, which is why the name comes through `progressLabelKey`
 * rather than off `ENHANCEMENTS` directly: the detector is not an enhancement a user adds and is
 * offered by no menu here, so what the chip should say is the enhancement that needed it. The
 * transfer of that detector is the first thing a user sees of a face recovery, and it would
 * otherwise be a window sitting still through a download.
 *
 * **A session that is slow to open says so, after {@link PREPARING_NOTICE_DELAY_MS}.** Opening a model on CoreML or
 * TensorRT compiles it the first time, which is minutes with the bar standing still and the machine looking idle - and
 * nothing to tell a compile from a hang. Once the report has said `preparing` for that long the chip names the model
 * and what is happening to it (*Compiling Kazan model*, or *Loading Kazan model* where nothing compiles), adds that
 * this happens once and can take minutes, and sweeps an indeterminate bar rather than holding a still one. A fast load
 * never reaches the delay, and reads as the activity it is about to become. The model is named from `operations`, the
 * chain being run, since a report names its family and not its model.
 *
 * **Where the library remembers the last slow open of the model, the chip says how long it took** - *First use only,
 * may take about 3 minutes* - and fills its bar against it rather than sweeping, by {@link estimatedFraction}: the
 * compile is repeated exactly when what the last one wrote was discarded, on the same machine, so the last time is a
 * good guess and a bar that moves says more than one that sweeps. Counted from the first `preparing` report rather
 * than from the notice, since the estimate is for the whole open.
 */
export const ProgressBar = ({
    report,
    fraction,
    operations = [],
}: {
    report?: RunProgress;
    fraction: number;
    operations?: readonly Operation[];
}) => {
    const { t } = useTranslation();

    const named = progressLabelKey(report?.family);
    const preparing = report?.stage === "preparing" ? report : undefined;
    const slow = useLingering(
        preparing && `${preparing.run}:${preparing.family}:${preparing.provider ?? ""}`,
        PREPARING_NOTICE_DELAY_MS,
    );
    const compiles = slow && preparing?.provider !== undefined && COMPILES.has(preparing.provider);
    const expectedMs = preparing?.expectedMs;
    const elapsed = useElapsed(
        preparing && expectedMs !== undefined
            ? `${preparing.run}:${preparing.family}:${preparing.provider ?? ""}`
            : undefined,
    );
    const value = !slow
        ? toPercent(fraction)
        : expectedMs === undefined
          ? null
          : toPercent(estimatedFraction(elapsed, expectedMs));

    const label =
        slow && preparing ? (
            <PreparingLabel report={preparing} compiles={compiles} operations={operations} />
        ) : report?.stage === "installing" && report.installFraction !== undefined ? (
            t("preview.progress.downloading", { percent: toPercent(report.installFraction) })
        ) : named ? (
            t(named)
        ) : (
            t("preview.progress.enhancing")
        );

    return (
        <div
            /*
             * Three stacked shadows rather than one: a hairline contact shadow that keeps the bar's
             * own edge readable, and two progressively wider and softer casts. A single shadow over
             * a photograph reads as a grey smudge - the bar has to sit above an arbitrary image, and
             * it is the falloff between the tight cast and the wide one that the eye reads as
             * height rather than as a border.
             */
            className="absolute top-4 right-4 z-5 min-h-8 w-max min-w-37 overflow-hidden rounded-lg shadow-[0_1px_2px_rgba(0,0,0,0.35),0_4px_10px_-2px_rgba(0,0,0,0.45),0_12px_28px_-8px_rgba(0,0,0,0.5)]"
            data-slot="preview-progress"
        >
            {/*
             * The shared bar rather than a filled `<div>` of this component's own: its indicator
             * carries the transition that turns each report into a slide rather than a jump, and
             * reports arrive about a hundred times a run - far enough apart for the difference to
             * be the whole of how the bar reads. It also makes the thing a `progressbar` to a
             * screen reader, which a pair of nested boxes was not.
             */}
            <Progress value={value} className="absolute inset-0 h-full rounded-lg bg-secondary" />

            {/*
             * The bevel, as an overlay of its own rather than as an inset shadow on the box above:
             * an inset shadow paints over its element's background but under its children's, so on
             * the container it would disappear behind the track, and on the track it would disappear
             * behind the indicator for as much of the bar as the run has filled. Drawn last, over
             * both, it survives the fill sweeping across it.
             *
             * A light top edge and a dark bottom one are what make the surface look lit from above -
             * without them the layered cast below reads as the bar hovering flat rather than as a
             * solid object with a thickness.
             */}
            <span
                aria-hidden="true"
                className="pointer-events-none absolute inset-0 rounded-lg shadow-[inset_0_1px_0_rgba(255,255,255,0.3),inset_0_-1px_0_rgba(0,0,0,0.35),inset_0_0_0_1px_rgba(255,255,255,0.08)]"
            />

            {/*
             * In the flow, and the one child that is: it is what gives the box its size. Relative,
             * so it paints over the bar and the bevel drawn before it. `whitespace-nowrap` keeps
             * each line one line; the second is the notice a slow compile adds.
             */}
            <span className="relative flex min-h-8 flex-col items-center justify-center px-3 py-1 font-medium text-[13px] text-white whitespace-nowrap">
                {label}
                {compiles && (
                    <span className="font-normal text-[11px] text-white/85">
                        {expectedMs === undefined
                            ? t("preview.progress.firstUse")
                            : t("preview.progress.firstUseAbout", { count: estimatedMinutes(expectedMs) })}
                    </span>
                )}
            </span>
        </div>
    );
};
