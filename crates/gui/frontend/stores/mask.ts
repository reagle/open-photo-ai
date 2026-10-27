import { create } from "zustand";
import type { Stroke } from "@/ipc/mask";
import {
    HISTORY_LIMIT,
    isEmpty,
    keepsPoint,
    radiusOf,
    SIZE_DEFAULT,
    type Snapshot,
    SOFTNESS_DEFAULT,
    stepSize,
    stepSoftness,
} from "@/lib/mask";
import { clampTo } from "@/lib/utils";
import { registerFileOwner } from "@/stores/files";

/**
 * One photograph's mask: what is painted, and the steps back and forward through it.
 *
 * Replaced whole on every change, never mutated, so the entry itself says whether anything changed - which is what
 * lets a run compare it by reference rather than by what is in it.
 */
export type MaskEntry = Snapshot & {
    /** The states before the present one, the latest last. At most {@link HISTORY_LIMIT}. */
    undo: Snapshot[];
    /** The states Undo stepped back from, the next last. Emptied by any new change. */
    redo: Snapshot[];
    /**
     * Which change this entry is the result of: a number the whole store counts up, so no two entries ever share one,
     * and a photograph whose mask is taken away and given back is not mistaken for the one it had before.
     */
    version: number;
};

/**
 * What the brush does, and whether the tint is shown. One set for the window: the tool does not change with the
 * photograph.
 */
export type Tool = {
    mode: "paint" | "erase";
    /** The brush's diameter as a fraction of the longer side. */
    size: number;
    /** `0` is a hard edge and `1` feathers from the middle out. */
    softness: number;
    /** Whether the painted area is tinted over the preview while editing. */
    overlay: boolean;
};

/** A stroke being drawn, which belongs to no mask until the pointer comes up. */
export type LiveStroke = { path: string; stroke: Stroke };

type MaskStore = {
    // **By path**, like the enhancement stack it limits: the mask belongs to what is done to this file. Strokes are
    // fractions of the *framed* photograph, so a change of framing takes it away - see `takeMask`.
    /** Each open photograph's mask, keyed by the file's path. */
    masks: Map<string, MaskEntry>;
    tool: Tool;
    /**
     * The photograph the preview is painting on, or absent. The preview is in paint mode exactly while this is set, for
     * that photograph and no other. Left when the photograph changes, the stack empties, Escape is pressed or Done is.
     */
    editing: { path: string } | undefined;
    /** Whether Space is held, which turns a drag from a stroke into a pan. Written by the panel's key handling. */
    panning: boolean;
    /** The stroke under the pointer, if any. Not part of any mask, so drawing it re-runs nothing. */
    live: LiveStroke | undefined;

    setMode: (mode: Tool["mode"]) => void;
    /** Swaps paint and erase. */
    swapMode: () => void;
    setSize: (size: number) => void;
    stepSize: (direction: 1 | -1) => void;
    setSoftness: (softness: number) => void;
    stepSoftness: (direction: 1 | -1) => void;
    setOverlay: (overlay: boolean) => void;
    setPanning: (panning: boolean) => void;

    startEditing: (path: string) => void;
    stopEditing: () => void;

    /** Starts a stroke at (`x`, `y`) with the tool's size and softness, and erasing if the tool is, or `erase` is. */
    beginStroke: (path: string, x: number, y: number, erase: boolean) => void;
    /**
     * Adds a point to the stroke, unless it is too close to the last one - see {@link keepsPoint}. `aspect` is the
     * photograph's width over its height.
     */
    extendStroke: (x: number, y: number, aspect: number) => void;
    /** Ends the stroke at (`x`, `y`) and commits it to the mask, which is the one step Undo takes back. */
    endStroke: (x: number, y: number) => void;
    /** Drops the stroke without committing it: a press the system took away. */
    cancelStroke: () => void;

    /** Turns the weight over, as one undoable step. Nothing for a mask that paints nothing. */
    invert: (path: string) => void;
    /** Takes every stroke away, as one undoable step. */
    clear: (path: string) => void;
    undo: (path: string) => void;
    redo: (path: string) => void;

    /**
     * Takes the mask, and its history, away, and hands it back.
     *
     * For a change of framing, which the strokes do not survive. Not an undoable step - the history it would belong to
     * is the old framing's - so what comes back is what {@link MaskStore.restoreMask} gives back, to undo the crop
     * with. The entry is always taken, but `undefined` comes back for a mask that paints nothing: there is nothing to
     * offer back, though its history was the old framing's all the same.
     */
    takeMask: (path: string) => MaskEntry | undefined;
    /** Gives a mask taken by {@link MaskStore.takeMask} back, with its history, as a change of its own. */
    restoreMask: (path: string, entry: MaskEntry) => void;

    forget: (path: string) => void;
    forgetAll: () => void;
};

/** The count {@link MaskEntry.version} is taken from. */
let changes = 0;

/** What an entry is a copy of, which is all a step of history holds. */
const snapshot = ({ strokes, inverted }: Snapshot): Snapshot => ({ strokes, inverted });

/** `history` with `step` on the end, the oldest dropped past the limit. */
const pushed = (history: Snapshot[], step: Snapshot) => [...history, step].slice(-HISTORY_LIMIT);

/** `entry`'s state, changed to `next`, with the step it came from on the undo stack and nothing to redo. */
const changed = (entry: MaskEntry | undefined, next: Snapshot): MaskEntry => ({
    ...next,
    undo: pushed(entry?.undo ?? [], snapshot(entry ?? { strokes: [], inverted: false })),
    redo: [],
    version: ++changes,
});

const withEntry = (masks: Map<string, MaskEntry>, path: string, entry: MaskEntry) => new Map(masks).set(path, entry);

// Not persisted, as nothing else per photograph in the window is: strokes are editing state, and the framing they are
// fractions of is itself forgotten at the next launch.
/**
 * Each open photograph's mask and its undo, the brush's settings, and whether the preview is painting.
 *
 * **A stroke is committed when the pointer comes up**, and only then does the mask change: what a run is sent, and what
 * a re-run compares, is the committed mask, never the stroke in progress.
 */
export const useMaskStore = create<MaskStore>()((set, get) => ({
    masks: new Map<string, MaskEntry>(),
    tool: { mode: "paint", size: SIZE_DEFAULT, softness: SOFTNESS_DEFAULT, overlay: true },
    editing: undefined,
    panning: false,
    live: undefined,

    setMode: (mode) => set((state) => ({ tool: { ...state.tool, mode } })),
    swapMode: () =>
        set((state) => ({ tool: { ...state.tool, mode: state.tool.mode === "paint" ? "erase" : "paint" } })),
    setSize: (size) => set((state) => ({ tool: { ...state.tool, size } })),
    stepSize: (direction) => set((state) => ({ tool: { ...state.tool, size: stepSize(state.tool.size, direction) } })),
    setSoftness: (softness) => set((state) => ({ tool: { ...state.tool, softness: clampTo(softness, 0, 1) } })),
    stepSoftness: (direction) =>
        set((state) => ({ tool: { ...state.tool, softness: stepSoftness(state.tool.softness, direction) } })),
    setOverlay: (overlay) => set((state) => ({ tool: { ...state.tool, overlay } })),
    setPanning: (panning) => set((state) => (state.panning === panning ? state : { panning })),

    startEditing: (path) => set({ editing: { path } }),
    stopEditing: () => set({ editing: undefined, panning: false, live: undefined }),

    beginStroke: (path, x, y, erase) => {
        const { tool } = get();

        set({
            live: {
                path,
                stroke: {
                    erase: erase || tool.mode === "erase",
                    radius: radiusOf(tool.size),
                    softness: tool.softness,
                    points: [x, y],
                },
            },
        });
    },

    extendStroke: (x, y, aspect) =>
        set((state) => {
            const { live } = state;
            if (!live || !keepsPoint(live.stroke.points, x, y, live.stroke.radius, aspect)) return state;

            return { live: { ...live, stroke: { ...live.stroke, points: [...live.stroke.points, x, y] } } };
        }),

    endStroke: (x, y) =>
        set((state) => {
            const { live } = state;
            if (!live) return state;

            // The last point is always kept, so a stroke ends where the pointer did - which for a click, with no move
            // between, is the one point it began at.
            const { points } = live.stroke;
            const [lastX, lastY] = [points[points.length - 2], points[points.length - 1]];
            const ends = lastX !== x || lastY !== y;
            const stroke = { ...live.stroke, points: ends ? [...points, x, y] : points };

            const entry = state.masks.get(live.path);

            return {
                live: undefined,
                masks: withEntry(
                    state.masks,
                    live.path,
                    changed(entry, {
                        strokes: [...(entry?.strokes ?? []), stroke],
                        inverted: entry?.inverted ?? false,
                    }),
                ),
            };
        }),

    cancelStroke: () => set({ live: undefined }),

    invert: (path) =>
        set((state) => {
            const entry = state.masks.get(path);
            // The weight of nothing, turned over, is still nothing: Rust reads it as no mask, so there is nothing to
            // undo.
            if (!entry || isEmpty(entry)) return state;

            return {
                masks: withEntry(
                    state.masks,
                    path,
                    changed(entry, { strokes: entry.strokes, inverted: !entry.inverted }),
                ),
            };
        }),

    clear: (path) =>
        set((state) => {
            const entry = state.masks.get(path);
            if (!entry || isEmpty(entry)) return state;

            return { masks: withEntry(state.masks, path, changed(entry, { strokes: [], inverted: false })) };
        }),

    undo: (path) =>
        set((state) => {
            const entry = state.masks.get(path);
            const previous = entry?.undo.at(-1);
            if (!entry || !previous) return state;

            return {
                masks: withEntry(state.masks, path, {
                    ...previous,
                    undo: entry.undo.slice(0, -1),
                    redo: [...entry.redo, snapshot(entry)],
                    version: ++changes,
                }),
            };
        }),

    redo: (path) =>
        set((state) => {
            const entry = state.masks.get(path);
            const next = entry?.redo.at(-1);
            if (!entry || !next) return state;

            return {
                masks: withEntry(state.masks, path, {
                    ...next,
                    undo: pushed(entry.undo, snapshot(entry)),
                    redo: entry.redo.slice(0, -1),
                    version: ++changes,
                }),
            };
        }),

    takeMask: (path) => {
        const entry = get().masks.get(path);
        if (!entry) return;

        // Taken whether or not it paints anything: an emptied mask still holds the old framing's history, and an Undo
        // after the crop must not step back into it.
        set((state) => {
            const masks = new Map(state.masks);
            masks.delete(path);

            return { masks };
        });

        return isEmpty(entry) ? undefined : entry;
    },

    restoreMask: (path, entry) =>
        set((state) => ({ masks: withEntry(state.masks, path, { ...entry, version: ++changes }) })),

    forget: (path) =>
        set((state) => {
            const left = state.editing?.path === path ? { editing: undefined, panning: false } : {};
            const live = state.live?.path === path ? { live: undefined } : {};
            if (!state.masks.has(path) && Object.keys(left).length === 0 && Object.keys(live).length === 0)
                return state;

            const masks = new Map(state.masks);
            masks.delete(path);

            return { masks, ...left, ...live };
        }),

    forgetAll: () => set({ masks: new Map<string, MaskEntry>(), editing: undefined, panning: false, live: undefined }),
}));

/** One photograph's mask, or `undefined` for one with none. */
export const useFileMask = (path: string | undefined): MaskEntry | undefined =>
    useMaskStore((state) => (path === undefined ? undefined : state.masks.get(path)));

/** Whether one photograph has a mask that paints something. */
export const useHasMask = (path: string | undefined): boolean =>
    useMaskStore((state) => path !== undefined && !isEmpty(state.masks.get(path)));

/** Whether the preview is painting on the photograph at `path`. */
export const useIsEditing = (path: string | undefined): boolean =>
    useMaskStore((state) => path !== undefined && state.editing?.path === path);

/*
 * A per-file owner keyed by path, so a closed photograph's mask goes with it, and a preview painting on it stops. The
 * identity is ignored: the mask belongs to what is done to the file, like its stack.
 */
registerFileOwner({
    forget: (path) => useMaskStore.getState().forget(path),
    forgetAll: () => useMaskStore.getState().forgetAll(),
});
