import { create } from "zustand";
import { registerFileOwner, useFileStore } from "@/stores/files";

/**
 * Where the batch is: not started (or forgotten), paging through the photographs, paused by the user, or through them.
 */
export type BatchPhase = "idle" | "running" | "paused" | "finished";

/** How the window's last run over a photograph ended, for the request it was asked as. */
export type Outcome = { key: string; failed: boolean };

type BatchStore = {
    phase: BatchPhase;
    /** The photographs the batch is to go through, by path. Walked in the strip's order, not this set's. */
    members: ReadonlySet<string>;
    /** The photograph the batch has made current and is waiting on, while it runs. */
    current: string | undefined;
    /**
     * The photographs whose Autopilot analysis the batch has already waited for, so one that suggested nothing - or
     * whose analysis failed - is passed over rather than visited again.
     */
    analysed: ReadonlySet<string>;
    /**
     * How the window's last run over each open photograph ended - the batch's or the user's own - by path, with the
     * request it ended for. A photograph whose request has changed since (an edit) is unfinished again, whatever this
     * says.
     */
    outcomes: ReadonlyMap<string, Outcome>;
    /** The photograph the window's run is over now, if one is in flight. */
    onScreen: string | undefined;
    /** Whether that run has reported working - the one thing a spinning badge may be drawn for. */
    onScreenStarted: boolean;
    /** The strip position a click asked for while the batch ran, while the question whether to pause is open. */
    asking: number | undefined;

    /** Adds `paths` to the batch and sets it running. */
    start: (paths: readonly string[]) => void;
    pause: () => void;
    resume: () => void;
    finish: () => void;
    /** Says which photograph the batch is waiting on, or none. */
    at: (path: string | undefined) => void;
    /** Records that the batch has waited out `path`'s Autopilot analysis. */
    markAnalysed: (path: string) => void;
    /** Records how a window run over `path`, asked for as `key`, ended. Ignored for a photograph that is not open. */
    record: (path: string, key: string, failed: boolean) => void;
    /** Says which photograph the window's run is over, or none. */
    showing: (path: string | undefined) => void;
    /** Says that the window's run has reported working. */
    started: () => void;
    ask: (index: number | undefined) => void;
    forget: (path: string) => void;
    forgetAll: () => void;
};

const without = <T>(set: ReadonlySet<T>, value: T) => {
    const copy = new Set(set);
    copy.delete(value);

    return copy;
};

/**
 * The batch: the window paging through the open photographs by itself, one run at a time in the window's own slot.
 * See `lib/batch.ts` for the walk, and `hooks/useEnhancementRun.ts` for where the outcomes come from.
 *
 * Not persisted: a batch is over the photographs that are open.
 */
export const useBatchStore = create<BatchStore>()((set) => ({
    phase: "idle",
    members: new Set<string>(),
    current: undefined,
    analysed: new Set<string>(),
    outcomes: new Map<string, Outcome>(),
    onScreen: undefined,
    onScreenStarted: false,
    asking: undefined,

    start: (paths) =>
        set((state) => ({
            phase: "running",
            // A batch that had finished starts over with what it is given; one paused or running takes them on.
            members: new Set([
                ...(state.phase === "finished" || state.phase === "idle" ? [] : state.members),
                ...paths,
            ]),
            analysed: state.phase === "finished" || state.phase === "idle" ? new Set<string>() : state.analysed,
        })),
    pause: () => set((state) => (state.phase === "running" ? { phase: "paused", current: undefined } : state)),
    resume: () => set((state) => (state.phase === "paused" ? { phase: "running" } : state)),
    finish: () => set({ phase: "finished", current: undefined }),
    at: (path) => set({ current: path }),
    markAnalysed: (path) => set((state) => ({ analysed: new Set(state.analysed).add(path) })),
    record: (path, key, failed) =>
        set((state) =>
            useFileStore.getState().files.some((file) => file.path === path)
                ? { outcomes: new Map(state.outcomes).set(path, { key, failed }) }
                : state,
        ),
    showing: (path) => set({ onScreen: path, onScreenStarted: false }),
    started: () => set((state) => (state.onScreenStarted ? state : { onScreenStarted: true })),
    ask: (index) => set({ asking: index }),
    forget: (path) =>
        set((state) => {
            if (!state.members.has(path) && !state.outcomes.has(path)) return state;

            const outcomes = new Map(state.outcomes);
            outcomes.delete(path);

            return {
                members: without(state.members, path),
                analysed: without(state.analysed, path),
                outcomes,
                ...(state.current === path && { current: undefined }),
            };
        }),
    forgetAll: () =>
        set({
            phase: "idle",
            members: new Set<string>(),
            current: undefined,
            analysed: new Set<string>(),
            outcomes: new Map<string, Outcome>(),
            asking: undefined,
        }),
}));

/* A per-file owner keyed by path, as the enhancement stack's is: a closed photograph leaves the batch. */
registerFileOwner({
    forget: (path) => useBatchStore.getState().forget(path),
    forgetAll: () => useBatchStore.getState().forgetAll(),
});
