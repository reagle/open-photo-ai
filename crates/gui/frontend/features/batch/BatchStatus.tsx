import { type ReactNode, useEffect, useState } from "react";
import { Check, CircleAlert, Clock, LoaderCircle, Pause, Play } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useBatchCounts, useBatchRows } from "@/hooks/useBatch";
import { renditionFor } from "@/ipc/images";
import { pauseBatch, processAll, progressOf, resumeBatch } from "@/lib/batch";
import { THUMBNAIL_BOUND } from "@/lib/constants";
import { fileName } from "@/lib/paths";
import { cn } from "@/lib/utils";
import { useBatchStore } from "@/stores/batch";
import { useFileStore } from "@/stores/files";

/** The least a "Process all" is offered for: one photograph is processed by looking at it. */
const PROCESS_ALL_FROM = 2;

/** How long the pill says the batch is done before it fades, and how long the fade takes. */
export const DONE_SHOWN_MS = 5000;
export const DONE_FADE_MS = 500;

// Filled, with the window's near-black for the text, so the pill is the one coloured thing in the title bar and still
// reads at 12px: 6.7:1 on the accent, 13:1 on the amber, both clear of WCAG AA's 4.5:1. The application has one
// palette, dark (see `style.css`), so there is no light theme to pass separately; the fills are also 6:1 and 11.6:1
// against the navbar's card, so the pill's edge is plain without a border. The words say the state - *Processing*,
// *Paused*, *Done* - and the colour only repeats them.
/** The pill's fill and text for each state of the batch it shows. */
const TONES = {
    running: "bg-primary text-background",
    paused: "bg-warning text-background",
    done: "bg-success text-background",
} as const;

/** The pill's inner controls: the body that opens the list, and the button at its end. */
const INNER =
    "flex h-full items-center rounded-full outline-none transition-colors hover:bg-background/15 focus-visible:ring-2 focus-visible:ring-background/60";

/**
 * How much of the batch is through, as a ring that fills clockwise from the top. Drawn in the text's colour, its
 * track at a third of it; hidden from assistive technology, since the words beside it say the same.
 */
const ProgressRing = ({ fraction }: { fraction: number }) => {
    const radius = 5;
    const circumference = 2 * Math.PI * radius;
    const filled = Math.min(Math.max(fraction, 0), 1);

    return (
        <svg
            aria-hidden
            viewBox="0 0 14 14"
            className="-rotate-90 size-3.5"
            data-slot="batch-ring"
            data-fraction={filled.toFixed(2)}
        >
            <circle cx="7" cy="7" r={radius} fill="none" stroke="currentColor" strokeOpacity={0.3} strokeWidth="2" />
            <circle
                cx="7"
                cy="7"
                r={radius}
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeDasharray={circumference}
                strokeDashoffset={circumference * (1 - filled)}
            />
        </svg>
    );
};

/** The glyph each row of the list draws beside its state's name. */
const ROW_GLYPHS = { done: Check, processing: LoaderCircle, pending: Clock, failed: CircleAlert } as const;

/**
 * The batch's photographs in the strip's order, each with its thumbnail, its name and where it stands. A row goes to
 * its photograph as a click on the strip does - so while the batch runs, through the question whether to pause it.
 */
const BatchList = ({ close }: { close: () => void }) => {
    const { t } = useTranslation();
    const rows = useBatchRows();

    const go = (index: number) => {
        close();
        useFileStore.getState().setCurrentIndex(index);
    };

    return (
        <ul aria-label={t("batch.list.title")} className="flex max-h-80 flex-col gap-0.5 overflow-y-auto">
            {rows.map(({ file, index, state }) => {
                const Glyph = ROW_GLYPHS[state];

                return (
                    <li key={file.path}>
                        <button
                            type="button"
                            onClick={() => go(index)}
                            data-state={state}
                            className="flex w-full items-center gap-2.5 rounded-md p-1 text-left text-xs outline-none hover:bg-accent focus-visible:bg-accent"
                        >
                            <img
                                src={renditionFor(file, THUMBNAIL_BOUND)}
                                alt=""
                                loading="lazy"
                                className="size-8 flex-none rounded-sm bg-surface-thumbnail object-cover"
                            />
                            <span className="min-w-0 flex-1 truncate">{fileName(file.path)}</span>
                            <span
                                className={cn(
                                    "flex flex-none items-center gap-1",
                                    state === "failed" ? "text-destructive" : "text-muted-foreground",
                                )}
                            >
                                <Glyph aria-hidden className={cn("size-3", state === "processing" && "animate-spin")} />
                                {t(`batch.state.${state}`)}
                            </span>
                        </button>
                    </li>
                );
            })}
        </ul>
    );
};

type PillProps = {
    tone: keyof typeof TONES;
    /** What the pill says: also its body's accessible name, and what the live region announces. */
    label: string;
    /** Before the label: the ring while running or paused, a tick when done. */
    lead: ReactNode;
    /** The button at the pill's end: Pause or Resume. None when done. */
    action?: { label: string; icon: ReactNode; onPress: () => void };
    /** What a press on the body does instead of opening the list: the done pill's dismissal. */
    onBodyPress?: () => void;
    fading?: boolean;
};

/**
 * The pill itself, one element tree for all three states, so the list open over a running batch stays open when it
 * pauses. The words are announced politely from a live region of their own rather than from the pill, so the buttons'
 * names are not read out with every step.
 */
const Pill = ({ tone, label, lead, action, onBodyPress, fading = false }: PillProps) => {
    const { t } = useTranslation();
    const [open, setOpen] = useState(false);
    const body = (
        <button
            type="button"
            className={cn(INNER, "gap-1.5 pr-2 pl-1.5")}
            {...(onBodyPress && { onClick: onBodyPress })}
        >
            {lead}
            <span className="whitespace-nowrap">{label}</span>
        </button>
    );

    return (
        <div
            data-slot="batch-status"
            data-tone={tone}
            className={cn(
                "flex h-6 items-center gap-0.5 rounded-full p-0.5 font-medium text-xs transition-opacity",
                TONES[tone],
                fading && "opacity-0",
            )}
            style={{ transitionDuration: `${DONE_FADE_MS}ms` }}
        >
            <span role="status" className="sr-only">
                {label}
            </span>

            {onBodyPress ? (
                body
            ) : (
                <Popover open={open} onOpenChange={setOpen}>
                    <PopoverTrigger asChild>{body}</PopoverTrigger>
                    <PopoverContent align="end" className="w-80 p-1.5" aria-label={t("batch.list.title")}>
                        <BatchList close={() => setOpen(false)} />
                    </PopoverContent>
                </Popover>
            )}

            {action && (
                <button type="button" onClick={action.onPress} className={cn(INNER, "gap-1 bg-background/15 px-2")}>
                    {action.icon}
                    {action.label}
                </button>
            )}
        </div>
    );
};

/** The counts a finished batch ended with, held for the moment the pill says so. */
type Ended = { done: number; failed: number; fading: boolean };

/**
 * The pill says *Done* for {@link DONE_SHOWN_MS} once the batch is through, then fades and goes, or goes at once when
 * pressed. The counts are taken as it ends: an edit afterwards re-opens a photograph, and the batch it ended is still
 * the one that is reported.
 */
const useEnded = () => {
    const phase = useBatchStore((state) => state.phase);
    const [ended, setEnded] = useState<Ended>();

    useEffect(() => {
        if (phase !== "finished") {
            setEnded(undefined);
            return;
        }

        const { done, failed } = progressOf();
        setEnded({ done, failed, fading: false });
        const fade = setTimeout(() => setEnded((now) => now && { ...now, fading: true }), DONE_SHOWN_MS);
        const gone = setTimeout(() => setEnded(undefined), DONE_SHOWN_MS + DONE_FADE_MS);

        return () => {
            clearTimeout(fade);
            clearTimeout(gone);
        };
    }, [phase]);

    return [ended, () => setEnded(undefined)] as const;
};

/**
 * The batch, in the title bar: a pill in the accent colour with a ring, *Processing 7 of 21* and Pause while it runs;
 * amber, *Paused — 14 left* and Resume while it is paused; green, *Done — 21 processed*, for a few seconds once it is
 * through. Its body opens the list of the batch's photographs. Without a batch, *Process all (5)* where two or more
 * open photographs have enhancements the window has not run yet, in the same place. Nothing otherwise.
 *
 * In the navbar rather than over the canvas: the canvas's corners are the panes' chips and the run's own bar, which
 * say what is happening to the photograph on screen; this says what is happening to all of them.
 */
export const BatchStatus = () => {
    const { t } = useTranslation();
    const phase = useBatchStore((state) => state.phase);
    const { done, failed, left, total, unprocessed } = useBatchCounts();
    const [ended, dismiss] = useEnded();
    const through = done + failed;

    if (phase === "running" && total > 0) {
        return (
            <Pill
                tone="running"
                label={t("batch.processing", { current: Math.min(through + 1, total), total })}
                lead={<ProgressRing fraction={through / total} />}
                action={{
                    label: t("batch.pause"),
                    icon: <Pause aria-hidden className="size-3" />,
                    onPress: pauseBatch,
                }}
            />
        );
    }

    if (phase === "paused" && left > 0) {
        return (
            <Pill
                tone="paused"
                label={t("batch.paused", { left })}
                lead={<ProgressRing fraction={through / total} />}
                action={{
                    label: t("batch.resume"),
                    icon: <Play aria-hidden className="size-3" />,
                    onPress: resumeBatch,
                }}
            />
        );
    }

    if (phase === "finished" && ended) {
        return (
            <Pill
                tone="done"
                label={
                    ended.failed > 0
                        ? t("batch.doneWithFailures", { processed: ended.done + ended.failed, failed: ended.failed })
                        : t("batch.done", { processed: ended.done })
                }
                lead={<Check aria-hidden className="size-3.5" />}
                onBodyPress={dismiss}
                fading={ended.fading}
            />
        );
    }

    if (unprocessed >= PROCESS_ALL_FROM) {
        return (
            <Button variant="secondary" size="xs" onClick={processAll} data-slot="batch-status">
                <Play aria-hidden />
                {t("batch.processAll", { count: unprocessed })}
            </Button>
        );
    }

    return null;
};
