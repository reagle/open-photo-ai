import { act } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { HISTORY_LIMIT, SIZE_MAX, SIZE_MIN } from "@/lib/mask";
import { useFileStore } from "@/stores/files";
import { HOLIDAY, openFiles, resetFileStore, SUNSET } from "@/test/support";
import { useMaskStore } from "./mask";

const PATH = HOLIDAY.path;

const store = () => useMaskStore.getState();
const entry = (path = PATH) => store().masks.get(path);

/** One stroke the way the preview draws it: pressed at (`x`, 0.5), moved on, released. */
const stroke = (x: number, path = PATH, erase = false) => {
    store().beginStroke(path, x, 0.5, erase);
    store().extendStroke(x + 0.2, 0.5, 1.5);
    store().endStroke(x + 0.2, 0.5);
};

beforeEach(() => {
    useMaskStore.setState(useMaskStore.getInitialState(), true);
    resetFileStore();
});

describe("a stroke", () => {
    it("is the live stroke until the pointer comes up, and in no mask", () => {
        store().beginStroke(PATH, 0.1, 0.2, false);
        store().extendStroke(0.5, 0.2, 1.5);

        expect(store().live?.stroke.points).toEqual([0.1, 0.2, 0.5, 0.2]);
        expect(entry()).toBeUndefined();
    });

    it("is committed whole when it ends, with the tool it began with", () => {
        store().setSize(0.1);
        store().setSoftness(0.25);
        store().beginStroke(PATH, 0.1, 0.5, false);
        // Changed mid-stroke: the stroke keeps what it began with.
        store().setSize(0.2);
        store().endStroke(0.3, 0.5);

        expect(store().live).toBeUndefined();
        expect(entry()?.strokes).toEqual([
            { erase: false, radius: 0.05, softness: 0.25, points: [0.1, 0.5, 0.3, 0.5] },
        ]);
    });

    it("is a dot where the pointer never moved", () => {
        store().beginStroke(PATH, 0.4, 0.4, false);
        store().endStroke(0.4, 0.4);

        expect(entry()?.strokes[0]?.points).toEqual([0.4, 0.4]);
    });

    it("erases when the tool does and when the modifier was held", () => {
        store().beginStroke(PATH, 0.1, 0.1, true);
        store().endStroke(0.1, 0.1);
        store().setMode("erase");
        store().beginStroke(PATH, 0.2, 0.2, false);
        store().endStroke(0.2, 0.2);

        expect(entry()?.strokes.map((one) => one.erase)).toEqual([true, true]);
    });

    it("drops the points closer than a tenth of the radius, but keeps where it ended", () => {
        store().setSize(0.1);
        store().beginStroke(PATH, 0.5, 0.5, false);

        // Radius 0.05, so 0.005 apart is too close in a square photograph.
        store().extendStroke(0.502, 0.5, 1);
        store().extendStroke(0.504, 0.5, 1);
        store().extendStroke(0.51, 0.5, 1);
        store().endStroke(0.511, 0.5);

        expect(entry()?.strokes[0]?.points).toEqual([0.5, 0.5, 0.51, 0.5, 0.511, 0.5]);
    });

    it("is dropped by a cancel, which commits nothing", () => {
        store().beginStroke(PATH, 0.1, 0.1, false);
        store().cancelStroke();

        expect(store().live).toBeUndefined();
        expect(entry()).toBeUndefined();
    });
});

describe("undo and redo", () => {
    it("steps back through the strokes and forward again", () => {
        stroke(0.1);
        stroke(0.3);

        store().undo(PATH);
        expect(entry()?.strokes).toHaveLength(1);

        store().undo(PATH);
        expect(entry()?.strokes).toHaveLength(0);

        store().redo(PATH);
        store().redo(PATH);
        expect(entry()?.strokes).toHaveLength(2);
    });

    it("does nothing past either end", () => {
        stroke(0.1);

        store().redo(PATH);
        expect(entry()?.strokes).toHaveLength(1);

        store().undo(PATH);
        const before = entry();
        store().undo(PATH);

        expect(entry()).toBe(before);
    });

    it("loses what could be redone to a new stroke", () => {
        stroke(0.1);
        stroke(0.3);
        store().undo(PATH);
        stroke(0.5);

        expect(entry()?.redo).toEqual([]);
        expect(entry()?.strokes).toHaveLength(2);
    });

    it("takes Invert as a step", () => {
        stroke(0.1);
        store().invert(PATH);
        expect(entry()?.inverted).toBe(true);

        store().undo(PATH);
        expect(entry()?.inverted).toBe(false);
        expect(entry()?.strokes).toHaveLength(1);

        store().redo(PATH);
        expect(entry()?.inverted).toBe(true);
    });

    it("takes Clear as a step, and Clear turns the weight the right way up too", () => {
        stroke(0.1);
        stroke(0.3);
        store().invert(PATH);
        store().clear(PATH);
        expect(entry()?.strokes).toEqual([]);
        expect(entry()?.inverted).toBe(false);

        store().undo(PATH);
        expect(entry()?.strokes).toHaveLength(2);
        expect(entry()?.inverted).toBe(true);
    });

    it("has nothing to invert or clear in a mask that paints nothing", () => {
        store().invert(PATH);
        store().clear(PATH);
        expect(entry()).toBeUndefined();

        stroke(0.1);
        store().undo(PATH);
        const before = entry();
        store().invert(PATH);
        store().clear(PATH);

        expect(entry()).toBe(before);
    });

    it("keeps at most a hundred steps, the oldest dropped", () => {
        for (let step = 0; step < HISTORY_LIMIT + 5; step++) stroke(0.1);

        expect(entry()?.undo).toHaveLength(HISTORY_LIMIT);

        for (let step = 0; step < HISTORY_LIMIT + 5; step++) store().undo(PATH);

        // The first five strokes can no longer be undone.
        expect(entry()?.strokes).toHaveLength(5);
    });
});

describe("the version", () => {
    it("is new for every change, so a re-run can tell, and never repeats across photographs", () => {
        stroke(0.1);
        const first = entry()?.version;
        stroke(0.2, SUNSET.path);
        const other = entry(SUNSET.path)?.version;
        store().undo(PATH);

        expect(new Set([first, other, entry()?.version]).size).toBe(3);
    });

    it("is a new entry, not an edited one", () => {
        stroke(0.1);
        const before = entry();
        stroke(0.2);

        expect(entry()).not.toBe(before);
        expect(before?.strokes).toHaveLength(1);
    });
});

describe("the tool", () => {
    it("steps its size and softness inside their ranges", () => {
        store().setSize(SIZE_MAX);
        store().stepSize(1);
        expect(store().tool.size).toBe(SIZE_MAX);

        store().setSize(SIZE_MIN);
        store().stepSize(-1);
        expect(store().tool.size).toBe(SIZE_MIN);

        store().setSoftness(0.95);
        store().stepSoftness(1);
        expect(store().tool.softness).toBe(1);
    });

    it("swaps paint and erase, and is the window's rather than the photograph's", () => {
        store().swapMode();
        expect(store().tool.mode).toBe("erase");
        store().swapMode();
        expect(store().tool.mode).toBe("paint");
    });
});

describe("editing", () => {
    it("is for one photograph, and stopping leaves Space and the live stroke behind", () => {
        store().startEditing(PATH);
        store().setPanning(true);
        store().beginStroke(PATH, 0.1, 0.1, false);
        store().stopEditing();

        expect(store().editing).toBeUndefined();
        expect(store().panning).toBe(false);
        expect(store().live).toBeUndefined();
    });
});

describe("taking the mask away for a change of crop", () => {
    it("hands back the mask with its history and removes it", () => {
        stroke(0.1);
        stroke(0.3);
        const had = entry();

        const taken = store().takeMask(PATH);

        expect(taken).toBe(had);
        expect(entry()).toBeUndefined();
    });

    it("hands back nothing for a mask that paints nothing, and for a photograph with none", () => {
        stroke(0.1);
        store().undo(PATH);

        expect(store().takeMask(PATH)).toBeUndefined();
        expect(store().takeMask(SUNSET.path)).toBeUndefined();
    });

    it("takes the history of a mask that paints nothing, so no Undo reaches back into the old framing", () => {
        stroke(0.1);
        store().clear(PATH);
        expect(entry()?.undo.length).toBeGreaterThan(0);

        expect(store().takeMask(PATH)).toBeUndefined();
        expect(entry()).toBeUndefined();

        store().undo(PATH);
        expect(entry()).toBeUndefined();
    });

    it("gives it back with its history, as a change of its own", () => {
        stroke(0.1);
        stroke(0.3);
        const taken = store().takeMask(PATH);
        if (!taken) throw new Error("a painted mask was not taken");

        store().restoreMask(PATH, taken);

        expect(entry()?.strokes).toEqual(taken.strokes);
        expect(entry()?.version).not.toBe(taken.version);

        store().undo(PATH);
        expect(entry()?.strokes).toHaveLength(1);
    });
});

describe("closing a photograph", () => {
    beforeEach(() => openFiles(HOLIDAY, SUNSET));

    it("forgets its mask and stops painting on it, and leaves the other alone", () => {
        stroke(0.1);
        stroke(0.1, SUNSET.path);
        store().startEditing(PATH);

        act(() => useFileStore.getState().closeFile(PATH));

        expect(entry()).toBeUndefined();
        expect(entry(SUNSET.path)).toBeDefined();
        expect(store().editing).toBeUndefined();
    });

    it("forgets every mask when every photograph is closed", () => {
        stroke(0.1);
        stroke(0.1, SUNSET.path);

        act(() => useFileStore.getState().closeAll());

        expect(store().masks.size).toBe(0);
    });
});

describe("the area painted for Remove", () => {
    const removal = (path = PATH) => store().removals.get(path);

    /** One stroke painted on the removal, as the preview paints it while the removal is being edited. */
    const removed = (x: number, path = PATH) => {
        store().startEditing(path, "removal");
        stroke(x, path);
    };

    it("takes the strokes painted while it is edited, and the limiting mask none of them", () => {
        removed(0.1);

        expect(removal()?.strokes).toHaveLength(1);
        expect(entry()).toBeUndefined();
        expect(store().editing).toEqual({ path: PATH, target: "removal" });
    });

    it("keeps a stroke in the region it began in, whichever is edited when it ends", () => {
        store().startEditing(PATH, "removal");
        store().beginStroke(PATH, 0.1, 0.5, false);
        expect(store().live?.target).toBe("removal");

        // Switching regions mid-stroke drops the stroke rather than committing it to the other one.
        store().startEditing(PATH, "limit");
        store().endStroke(0.3, 0.5);

        expect(removal()).toBeUndefined();
        expect(entry()).toBeUndefined();
    });

    it("has a history of its own, which the limiting mask's undo never reaches", () => {
        store().startEditing(PATH, "limit");
        stroke(0.1);
        removed(0.5);
        removed(0.6);

        store().undo(PATH);
        expect(entry()?.strokes).toHaveLength(0);
        expect(removal()?.strokes).toHaveLength(2);

        store().undo(PATH, "removal");
        expect(removal()?.strokes).toHaveLength(1);
        store().redo(PATH, "removal");
        expect(removal()?.strokes).toHaveLength(2);

        store().clear(PATH, "removal");
        expect(removal()?.strokes).toEqual([]);
        expect(removal()?.undo).toHaveLength(3);
    });

    it("is taken away and given back by region for a change of crop", () => {
        removed(0.1);
        store().startEditing(PATH, "limit");
        stroke(0.5);

        const taken = store().takeMask(PATH, "removal");
        expect(taken?.strokes).toHaveLength(1);
        expect(removal()).toBeUndefined();
        expect(entry()?.strokes).toHaveLength(1);

        store().restoreMask(PATH, taken as NonNullable<typeof taken>, "removal");
        expect(removal()?.strokes).toEqual(taken?.strokes);
    });

    it("is forgotten alone when its Remove is taken off, and stops being painted", () => {
        removed(0.1);
        store().startEditing(PATH, "limit");
        stroke(0.5);
        store().startEditing(PATH, "removal");

        store().forgetTarget(PATH, "removal");

        expect(removal()).toBeUndefined();
        expect(entry()?.strokes).toHaveLength(1);
        expect(store().editing).toBeUndefined();
    });

    it("leaves the limiting mask being painted when the removal is forgotten", () => {
        removed(0.1);
        store().startEditing(PATH, "limit");

        store().forgetTarget(PATH, "removal");

        expect(store().editing).toEqual({ path: PATH, target: "limit" });
    });

    it("goes with its photograph when it closes, as the mask does", () => {
        openFiles(HOLIDAY, SUNSET);
        removed(0.1);
        removed(0.1, SUNSET.path);

        act(() => useFileStore.getState().closeFile(PATH));
        expect(removal()).toBeUndefined();
        expect(removal(SUNSET.path)).toBeDefined();

        act(() => useFileStore.getState().closeAll());
        expect(store().removals.size).toBe(0);
    });
});
