import { create } from "zustand";
import type { Stroke } from "@/ipc/mask";
import {
    HISTORY_LIMIT,
    isEmpty,
    isUsedFor,
    keepsPoint,
    radiusOf,
    SIZE_DEFAULT,
    type Snapshot,
    SOFTNESS_DEFAULT,
    stepSize,
    stepSoftness,
    type Use,
    usesOf,
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
    /**
     * What painting paints on the lens blur's refinement: focus, kept sharp, or blur. The limiting mask and the removal
     * have one thing to paint, and ignore it.
     */
    paints: "focus" | "blur";
    /** The brush's diameter as a fraction of the longer side. */
    size: number;
    /** `0` is a hard edge and `1` feathers from the middle out. */
    softness: number;
    /** Whether the painted area is tinted over the preview while editing. */
    overlay: boolean;
};

/**
 * Which of a photograph's painted regions: the **limiting** mask, which keeps the chain's result where it is white
 * and the photograph where it is black, the **removal**, what Remove takes out, or the **refinement**, the lens blur's
 * Refine brush - focus strokes kept sharp and blur strokes blurred, each stroke saying which (`Stroke.blur`).
 *
 * Painted with one brush and one panel, and independent in everything else: each has its own strokes and its own
 * history, and none is ever sent in the place of another - the limiting mask is the run's, the removal is the Remove
 * operation's, the refinement the Lens Blur's. One is edited at a time.
 */
export type Target = "limit" | "removal" | "refine";

/** A stroke being drawn, which belongs to no mask until the pointer comes up - then to `target`'s. */
export type LiveStroke = { path: string; target: Target; stroke: Stroke };

type MaskStore = {
    // **By path**, like the enhancement stack it limits: the mask belongs to what is done to this file. Strokes are
    // fractions of the *framed* photograph, so a change of framing takes it away - see `takeMask`.
    /** Each open photograph's limiting mask, keyed by the file's path. */
    masks: Map<string, MaskEntry>;
    /**
     * Each open photograph's removal, keyed the same way: the region its Remove takes out. Belongs to the Remove in the
     * stack, so taking that off forgets it - see {@link MaskStore.forgetTarget}.
     */
    removals: Map<string, MaskEntry>;
    /** Each open photograph's lens blur refinement, keyed the same way, and forgotten with the Lens Blur likewise. */
    refines: Map<string, MaskEntry>;
    tool: Tool;
    /**
     * The photograph the preview is painting on, and which of its regions, or absent. The preview is in paint mode
     * exactly while this is set, for that photograph and no other. Left when the photograph changes, the stack empties,
     * Escape is pressed or Done is.
     */
    editing: { path: string; target: Target } | undefined;
    /** Whether Space is held, which turns a drag from a stroke into a pan. Written by the panel's key handling. */
    panning: boolean;
    /** The stroke under the pointer, if any. Not part of any mask, so drawing it re-runs nothing. */
    live: LiveStroke | undefined;

    setMode: (mode: Tool["mode"]) => void;
    /** Paints `paints` on the refinement, and leaves erasing for it. */
    setPaints: (paints: Tool["paints"]) => void;
    /** Swaps focus and blur. */
    swapPaints: () => void;
    /** Swaps paint and erase. */
    swapMode: () => void;
    setSize: (size: number) => void;
    stepSize: (direction: 1 | -1) => void;
    setSoftness: (softness: number) => void;
    stepSoftness: (direction: 1 | -1) => void;
    setOverlay: (overlay: boolean) => void;
    setPanning: (panning: boolean) => void;

    /** Paints `target`'s region of `path` - the limiting mask unless told otherwise - leaving any other. */
    startEditing: (path: string, target?: Target) => void;
    stopEditing: () => void;

    /**
     * Starts a stroke at (`x`, `y`) with the tool's size and softness, and erasing if the tool is, or `erase` is. It goes
     * to the region being edited, which is fixed for the stroke: one never ends in another region than it began in.
     */
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

    /**
     * Turns the limiting mask's weight over, as one undoable step. Nothing for a mask that paints nothing. The limiting
     * mask's alone: a removal turned over would take out everything but what was painted.
     */
    invert: (path: string) => void;
    /**
     * Switches one of the limiting mask's uses under a lens blur on or off, as one undoable step: limiting the
     * enhancements to it, or keeping it in focus. Nothing where it already is so. Kept by every other change, Clear
     * included: it is what the mask is for, not what is painted.
     */
    setUse: (path: string, use: Use, on: boolean) => void;
    /** Takes every stroke of `target`'s region away, as one undoable step. */
    clear: (path: string, target?: Target) => void;
    undo: (path: string, target?: Target) => void;
    redo: (path: string, target?: Target) => void;

    /**
     * Takes the mask, and its history, away, and hands it back.
     *
     * For a change of framing, which the strokes do not survive. Not an undoable step - the history it would belong to
     * is the old framing's - so what comes back is what {@link MaskStore.restoreMask} gives back, to undo the crop
     * with. The entry is always taken, but `undefined` comes back for a mask that paints nothing: there is nothing to
     * offer back, though its history was the old framing's all the same.
     */
    takeMask: (path: string, target?: Target) => MaskEntry | undefined;
    /** Gives a mask taken by {@link MaskStore.takeMask} back, with its history, as a change of its own. */
    restoreMask: (path: string, entry: MaskEntry, target?: Target) => void;

    /**
     * Forgets one region of one photograph, with its history, and stops painting it: what taking Remove off the stack
     * does to the strokes that were its whole setting, as taking off an upscale forgets its scale.
     */
    forgetTarget: (path: string, target: Target) => void;
    /** Forgets all of a photograph's regions: the file closed. */
    forget: (path: string) => void;
    forgetAll: () => void;
};

/** The count {@link MaskEntry.version} is taken from. */
let changes = 0;

/** What an entry is a copy of, which is all a step of history holds. */
const snapshot = ({ strokes, inverted, ...uses }: Snapshot): Snapshot => ({ strokes, inverted, ...usesOf(uses) });

/** `history` with `step` on the end, the oldest dropped past the limit. */
const pushed = (history: Snapshot[], step: Snapshot) => [...history, step].slice(-HISTORY_LIMIT);

/**
 * `entry`'s state, changed to `next`, with the step it came from on the undo stack and nothing to redo. The uses are
 * `entry`'s where `next` does not say.
 */
const changed = (entry: MaskEntry | undefined, next: Snapshot): MaskEntry => ({
    ...usesOf(entry),
    ...next,
    undo: pushed(entry?.undo ?? [], snapshot(entry ?? { strokes: [], inverted: false })),
    redo: [],
    version: ++changes,
});

const withEntry = (masks: Map<string, MaskEntry>, path: string, entry: MaskEntry) => new Map(masks).set(path, entry);

/** The store's key for `target`'s map of entries. */
const KEY = { limit: "masks", removal: "removals", refine: "refines" } as const satisfies Record<
    Target,
    keyof MaskStore
>;

/** The store's maps of entries, one a region. */
type Regions = Pick<MaskStore, "masks" | "removals" | "refines">;

/** `state`'s map of `target`'s entries. */
const mapOf = (state: Regions, target: Target) => state[KEY[target]];

/** The change to the store that puts `entry` in as `path`'s `target` region. */
const putting = (state: Regions, target: Target, path: string, entry: MaskEntry) => ({
    [KEY[target]]: withEntry(mapOf(state, target), path, entry),
});

/** The change to the store that takes `path`'s `target` region out, or `undefined` where it has none. */
const dropping = (state: Regions, target: Target, path: string) => {
    const map = mapOf(state, target);
    if (!map.has(path)) return;

    const left = new Map(map);
    left.delete(path);

    return { [KEY[target]]: left };
};

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
    removals: new Map<string, MaskEntry>(),
    refines: new Map<string, MaskEntry>(),
    tool: { mode: "paint", paints: "focus", size: SIZE_DEFAULT, softness: SOFTNESS_DEFAULT, overlay: true },
    editing: undefined,
    panning: false,
    live: undefined,

    setMode: (mode) => set((state) => ({ tool: { ...state.tool, mode } })),
    swapMode: () =>
        set((state) => ({ tool: { ...state.tool, mode: state.tool.mode === "paint" ? "erase" : "paint" } })),
    setPaints: (paints) => set((state) => ({ tool: { ...state.tool, mode: "paint", paints } })),
    swapPaints: () =>
        set((state) => ({
            tool: { ...state.tool, mode: "paint", paints: state.tool.paints === "focus" ? "blur" : "focus" },
        })),
    setSize: (size) => set((state) => ({ tool: { ...state.tool, size } })),
    stepSize: (direction) => set((state) => ({ tool: { ...state.tool, size: stepSize(state.tool.size, direction) } })),
    setSoftness: (softness) => set((state) => ({ tool: { ...state.tool, softness: clampTo(softness, 0, 1) } })),
    stepSoftness: (direction) =>
        set((state) => ({ tool: { ...state.tool, softness: stepSoftness(state.tool.softness, direction) } })),
    setOverlay: (overlay) => set((state) => ({ tool: { ...state.tool, overlay } })),
    setPanning: (panning) => set((state) => (state.panning === panning ? state : { panning })),

    // A region left for another is left as a stroke would be by a pointer that went away: nothing half-drawn moves.
    startEditing: (path, target = "limit") => set({ editing: { path, target }, live: undefined }),
    stopEditing: () => set({ editing: undefined, panning: false, live: undefined }),

    beginStroke: (path, x, y, erase) => {
        const { tool, editing } = get();
        const target = editing?.path === path ? editing.target : "limit";
        const erasing = erase || tool.mode === "erase";

        set({
            live: {
                path,
                target,
                stroke: {
                    erase: erasing,
                    // Only the refinement's strokes say which they paint, and only a blur says so.
                    ...(target === "refine" && !erasing && tool.paints === "blur" && { blur: true }),
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

            const entry = mapOf(state, live.target).get(live.path);

            return {
                live: undefined,
                ...putting(
                    state,
                    live.target,
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

    setUse: (path, use, on) =>
        set((state) => {
            const entry = state.masks.get(path);
            if (isUsedFor(entry, use) === on) return state;

            const strokes = entry?.strokes ?? [];
            return {
                masks: withEntry(
                    state.masks,
                    path,
                    changed(entry, { strokes, inverted: entry?.inverted ?? false, [use]: on }),
                ),
            };
        }),

    clear: (path, target = "limit") =>
        set((state) => {
            const entry = mapOf(state, target).get(path);
            if (!entry || isEmpty(entry)) return state;

            return putting(state, target, path, changed(entry, { strokes: [], inverted: false }));
        }),

    undo: (path, target = "limit") =>
        set((state) => {
            const entry = mapOf(state, target).get(path);
            const previous = entry?.undo.at(-1);
            if (!entry || !previous) return state;

            return putting(state, target, path, {
                ...previous,
                undo: entry.undo.slice(0, -1),
                redo: [...entry.redo, snapshot(entry)],
                version: ++changes,
            });
        }),

    redo: (path, target = "limit") =>
        set((state) => {
            const entry = mapOf(state, target).get(path);
            const next = entry?.redo.at(-1);
            if (!entry || !next) return state;

            return putting(state, target, path, {
                ...next,
                undo: pushed(entry.undo, snapshot(entry)),
                redo: entry.redo.slice(0, -1),
                version: ++changes,
            });
        }),

    takeMask: (path, target = "limit") => {
        const entry = mapOf(get(), target).get(path);
        if (!entry) return;

        // Taken whether or not it paints anything: an emptied mask still holds the old framing's history, and an Undo
        // after the crop must not step back into it.
        set((state) => dropping(state, target, path) ?? state);

        return isEmpty(entry) ? undefined : entry;
    },

    restoreMask: (path, entry, target = "limit") =>
        set((state) => putting(state, target, path, { ...entry, version: ++changes })),

    forgetTarget: (path, target) =>
        set((state) => {
            const left =
                state.editing?.path === path && state.editing.target === target
                    ? { editing: undefined, panning: false }
                    : {};
            const live = state.live?.path === path && state.live.target === target ? { live: undefined } : {};
            const dropped = dropping(state, target, path);
            if (!dropped && Object.keys(left).length === 0 && Object.keys(live).length === 0) return state;

            return { ...dropped, ...left, ...live };
        }),

    forget: (path) =>
        set((state) => {
            const left = state.editing?.path === path ? { editing: undefined, panning: false } : {};
            const live = state.live?.path === path ? { live: undefined } : {};
            const masks = dropping(state, "limit", path);
            const removals = dropping(state, "removal", path);
            const refines = dropping(state, "refine", path);
            if (!masks && !removals && !refines && Object.keys(left).length === 0 && Object.keys(live).length === 0)
                return state;

            return { ...masks, ...removals, ...refines, ...left, ...live };
        }),

    forgetAll: () =>
        set({
            masks: new Map<string, MaskEntry>(),
            removals: new Map<string, MaskEntry>(),
            refines: new Map<string, MaskEntry>(),
            editing: undefined,
            panning: false,
            live: undefined,
        }),
}));

/** One photograph's limiting mask - or, asked for, its removal - or `undefined` for one with none. */
export const useFileMask = (path: string | undefined, target: Target = "limit"): MaskEntry | undefined =>
    useMaskStore((state) => (path === undefined ? undefined : mapOf(state, target).get(path)));

/** Whether one photograph has a limiting mask - or, asked for, a removal - that paints something. */
export const useHasMask = (path: string | undefined, target: Target = "limit"): boolean =>
    useMaskStore((state) => path !== undefined && !isEmpty(mapOf(state, target).get(path)));

/** Whether the preview is painting on the photograph at `path`: on `target`'s region, or on either where none is named. */
export const useIsEditing = (path: string | undefined, target?: Target): boolean =>
    useMaskStore(
        (state) =>
            path !== undefined &&
            state.editing?.path === path &&
            (target === undefined || state.editing.target === target),
    );

/** Which region of `path` the preview is painting, or `undefined` where it paints none of them. */
export const useEditingTarget = (path: string | undefined): Target | undefined =>
    useMaskStore((state) => (path !== undefined && state.editing?.path === path ? state.editing.target : undefined));

/*
 * A per-file owner keyed by path, so a closed photograph's mask goes with it, and a preview painting on it stops. The
 * identity is ignored: the mask belongs to what is done to the file, like its stack.
 */
registerFileOwner({
    forget: (path) => useMaskStore.getState().forget(path),
    forgetAll: () => useMaskStore.getState().forgetAll(),
});
