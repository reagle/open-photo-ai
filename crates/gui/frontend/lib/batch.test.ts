import { act } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Operation } from "@/ipc/enhance";
import type { ImageRecord } from "@/ipc/images";
import { useBatchStore } from "@/stores/batch";
import { useCropStore } from "@/stores/crop";
import { useEnhancementStore } from "@/stores/enhancements";
import { canExport } from "@/stores/exportDialog";
import { useFileStore } from "@/stores/files";
import { useSettingsStore } from "@/stores/settings";
import { APPLY_ORDER, FRAMING, HOLIDAY, openFiles, resetCropStore, resetFileStore, SUNSET } from "@/test/support";

vi.mock("@/lib/report", () => ({ report: vi.fn() }));
vi.mock("@/lib/chime", () => ({ chime: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn() } }));
// Autopilot's analysis, answered when a test says: what it adds is what the batch then runs.
vi.mock("@/hooks/useAutopilot", () => ({ awaitAnalysis: vi.fn() }));

const { awaitAnalysis } = await import("@/hooks/useAutopilot");
const { toast } = await import("sonner");
const { chime } = await import("@/lib/chime");
const { answerPause, badgeOf, drive, pauseBatch, processAll, progressOf, requestOf, resumeBatch, startBatch } =
    await import("./batch");

/** A third photograph, so "the next unfinished one" is not simply "the other one". */
const HARBOUR: ImageRecord = { ...HOLIDAY, path: "/Users/someone/Pictures/harbour.png", identity: "00112233aabbccdd" };

const sharpen: Operation = { family: "sharpen", codename: "moscow", precision: "fp16", parameters: { strength: 0.5 } };

const current = () => {
    const { files, currentIndex } = useFileStore.getState();
    return files[currentIndex]?.path;
};

/** `files` open, each with a Sharpen. */
const opened = (...files: ImageRecord[]) => {
    openFiles(...files);
    for (const file of files) useEnhancementStore.getState().addEnhancement(file.path, sharpen, APPLY_ORDER);
};

/** Lets the batch's promises and store subscriptions run. */
const settle = () => act(async () => {});

/** Ends the window's run over the photograph on screen, as `useEnhancementRun` records it. */
const windowRunEnds = async (failed = false) => {
    const file = useFileStore.getState().files[useFileStore.getState().currentIndex];
    const request = file && requestOf(file);
    if (!file || !request) throw new Error("nothing on screen to run");

    useBatchStore.getState().record(file.path, request.key, failed);
    await settle();
};

beforeEach(async () => {
    vi.mocked(toast.success).mockReset();
    vi.mocked(chime).mockReset();
    vi.mocked(awaitAnalysis).mockReset();
    // A batch left running by a test would go on driving into the next one.
    useBatchStore.getState().pause();
    await settle();
    resetFileStore();
    resetCropStore();
    useBatchStore.getState().forgetAll();
    useEnhancementStore.setState({ enhancements: new Map(), autopilot: false });
    useSettingsStore.setState({ pauseBatchWithoutAsking: false });
});

describe("the batch", () => {
    it("pages through the photographs in the strip's order, one window run at a time, and says when it is through", async () => {
        opened(HOLIDAY, SUNSET, HARBOUR);

        startBatch([HOLIDAY.path, SUNSET.path, HARBOUR.path]);
        await settle();

        // The view follows the batch: what is being worked on is what is on screen.
        expect(current()).toBe(HOLIDAY.path);
        expect(progressOf()).toMatchObject({ done: 0, left: 3, total: 3 });

        await windowRunEnds();
        expect(current()).toBe(SUNSET.path);
        await windowRunEnds();
        expect(current()).toBe(HARBOUR.path);
        await windowRunEnds();

        expect(useBatchStore.getState().phase).toBe("finished");
        expect(toast.success).toHaveBeenCalledWith("3 photos processed", expect.anything());
        expect(chime).toHaveBeenCalledOnce();
    });

    it("passes over a photograph already done for what it would be sent now, without showing it", async () => {
        opened(HOLIDAY, SUNSET);
        const holiday = requestOf(HOLIDAY);
        useBatchStore.getState().record(HOLIDAY.path, holiday?.key ?? "", false);
        useFileStore.getState().setCurrentIndex(1);
        const shown: string[] = [];
        const unsubscribe = useFileStore.subscribe(() => shown.push(current() ?? ""));

        startBatch([HOLIDAY.path, SUNSET.path]);
        await settle();
        unsubscribe();

        expect(current()).toBe(SUNSET.path);
        expect(shown).not.toContain(HOLIDAY.path);
    });

    it("passes over a photograph with nothing to run", async () => {
        openFiles(HOLIDAY, SUNSET);
        useEnhancementStore.getState().addEnhancement(SUNSET.path, sharpen, APPLY_ORDER);

        startBatch([HOLIDAY.path, SUNSET.path]);
        await settle();

        expect(current()).toBe(SUNSET.path);
        expect(progressOf().total).toBe(1);
    });

    it("marks a photograph whose run failed and moves on, and says so at the end", async () => {
        opened(HOLIDAY, SUNSET);

        startBatch([HOLIDAY.path, SUNSET.path]);
        await settle();
        await windowRunEnds(true);

        expect(badgeOf(HOLIDAY)).toBe("failed");
        expect(current()).toBe(SUNSET.path);

        await windowRunEnds();
        expect(toast.success).toHaveBeenCalledWith("2 photos processed, 1 failed", expect.anything());
    });

    it("pauses where it is and resumes from the next unfinished photograph, which it makes current", async () => {
        opened(HOLIDAY, SUNSET, HARBOUR);
        startBatch([HOLIDAY.path, SUNSET.path, HARBOUR.path]);
        await settle();
        await windowRunEnds();

        pauseBatch();
        await settle();
        // The photograph on screen stays; nothing moves the view while paused.
        useFileStore.getState().setCurrentIndex(0);
        await settle();
        expect(current()).toBe(HOLIDAY.path);

        resumeBatch();
        await settle();

        expect(useBatchStore.getState().phase).toBe("running");
        expect(current()).toBe(SUNSET.path);
    });

    it("takes an edited photograph back on: it is unfinished again", async () => {
        opened(HOLIDAY, SUNSET);
        startBatch([HOLIDAY.path, SUNSET.path]);
        await settle();
        await windowRunEnds();
        pauseBatch();
        await settle();
        expect(badgeOf(HOLIDAY)).toBe("done");

        // A framing is part of every request.
        useCropStore.setState({ crops: new Map([[HOLIDAY.identity ?? "", FRAMING]]) });
        resumeBatch();
        await settle();

        expect(current()).toBe(HOLIDAY.path);
        expect(progressOf()).toMatchObject({ done: 0, left: 2 });
    });

    it("shows a photograph Autopilot has not looked at, waits for its suggestions, and runs them", async () => {
        openFiles(HOLIDAY, SUNSET);
        useEnhancementStore.setState({ autopilot: true });
        vi.mocked(awaitAnalysis).mockImplementation(async (file) => {
            useEnhancementStore.getState().addEnhancement(file.path, sharpen, APPLY_ORDER);
            return "added";
        });

        processAll();
        await settle();

        expect(awaitAnalysis).toHaveBeenCalledWith(HOLIDAY, undefined);
        expect(current()).toBe(HOLIDAY.path);
        expect(requestOf(HOLIDAY)).toBeDefined();

        await windowRunEnds();
        expect(current()).toBe(SUNSET.path);
    });

    it("passes over a photograph Autopilot suggested nothing for", async () => {
        openFiles(HOLIDAY, SUNSET);
        useEnhancementStore.setState({ autopilot: true });
        vi.mocked(awaitAnalysis).mockImplementation(async (file) => {
            if (file.path === SUNSET.path)
                useEnhancementStore.getState().addEnhancement(file.path, sharpen, APPLY_ORDER);
            return "added";
        });

        processAll();
        await settle();

        expect(current()).toBe(SUNSET.path);
        expect(awaitAnalysis).toHaveBeenCalledTimes(2);
    });

    it("runs once however often it is set going", async () => {
        opened(HOLIDAY, SUNSET);
        startBatch([HOLIDAY.path]);
        startBatch([SUNSET.path]);
        void drive();
        await settle();

        await windowRunEnds();
        expect(current()).toBe(SUNSET.path);
        await windowRunEnds();
        expect(chime).toHaveBeenCalledOnce();
    });

    it("forgets a photograph that is closed, and goes on to the next", async () => {
        opened(HOLIDAY, SUNSET);
        startBatch([HOLIDAY.path, SUNSET.path]);
        await settle();

        useFileStore.getState().closeFile(HOLIDAY.path);
        await settle();

        expect(useBatchStore.getState().members.has(HOLIDAY.path)).toBe(false);
        expect(current()).toBe(SUNSET.path);
    });
});

describe("a click during the batch", () => {
    it("asks before it goes anywhere, and Keep processing leaves the batch where it was", async () => {
        opened(HOLIDAY, SUNSET, HARBOUR);
        startBatch([HOLIDAY.path, SUNSET.path, HARBOUR.path]);
        await settle();

        useFileStore.getState().setCurrentIndex(2);

        expect(useBatchStore.getState().asking).toBe(2);
        expect(current()).toBe(HOLIDAY.path);

        answerPause(false, true);
        await settle();

        expect(useBatchStore.getState()).toMatchObject({ asking: undefined, phase: "running" });
        expect(current()).toBe(HOLIDAY.path);
        // Don't ask again is remembered with Pause & view alone.
        expect(useSettingsStore.getState().pauseBatchWithoutAsking).toBe(false);
    });

    it("pauses and goes to the photograph on Pause & view, and offers to resume with what is left", async () => {
        opened(HOLIDAY, SUNSET, HARBOUR);
        startBatch([HOLIDAY.path, SUNSET.path, HARBOUR.path]);
        await settle();

        useFileStore.getState().setCurrentIndex(2);
        answerPause(true, false);
        await settle();

        expect(useBatchStore.getState().phase).toBe("paused");
        expect(current()).toBe(HARBOUR.path);
        expect(progressOf().left).toBe(3);
    });

    it("pauses without asking once told not to ask again", async () => {
        opened(HOLIDAY, SUNSET, HARBOUR);
        startBatch([HOLIDAY.path, SUNSET.path, HARBOUR.path]);
        await settle();
        useFileStore.getState().setCurrentIndex(2);
        answerPause(true, true);
        resumeBatch();
        await settle();
        expect(current()).toBe(HOLIDAY.path);

        useFileStore.getState().setCurrentIndex(1);

        expect(useSettingsStore.getState().pauseBatchWithoutAsking).toBe(true);
        expect(useBatchStore.getState()).toMatchObject({ asking: undefined, phase: "paused" });
        expect(current()).toBe(SUNSET.path);
    });

    it("is not asked about while the batch is paused", async () => {
        opened(HOLIDAY, SUNSET);
        startBatch([HOLIDAY.path, SUNSET.path]);
        await settle();
        pauseBatch();

        useFileStore.getState().setCurrentIndex(1);

        expect(useBatchStore.getState().asking).toBeUndefined();
        expect(current()).toBe(SUNSET.path);
    });
});

describe("the strip's badges", () => {
    it("say queued for what the batch has still to reach, and spin only for the run that has reported", async () => {
        opened(HOLIDAY, SUNSET);
        startBatch([HOLIDAY.path, SUNSET.path]);
        await settle();
        useBatchStore.getState().showing(HOLIDAY.path);

        // Asked for, and not yet reporting: a model loading, or the previous run still stopping. Nothing to spin for.
        expect(badgeOf(HOLIDAY)).toBe("queued");
        expect(badgeOf(SUNSET)).toBe("queued");

        useBatchStore.getState().started();
        expect(badgeOf(HOLIDAY)).toBe("processing");

        useBatchStore.getState().showing(undefined);
        await windowRunEnds();
        expect(badgeOf(HOLIDAY)).toBe("done");
    });

    it("say nothing for a photograph with nothing to run, or outside any batch and never shown", () => {
        openFiles(HOLIDAY);
        expect(badgeOf(HOLIDAY)).toBeUndefined();

        useEnhancementStore.getState().addEnhancement(HOLIDAY.path, sharpen, APPLY_ORDER);
        expect(badgeOf(HOLIDAY)).toBeUndefined();
    });
});

describe("export during the batch", () => {
    it("is held back while the batch runs, and available again once it is paused", async () => {
        opened(HOLIDAY, SUNSET);
        const picked = new Set([HOLIDAY.path]);
        startBatch([HOLIDAY.path, SUNSET.path]);
        await settle();

        expect(canExport(picked)).toBe(false);

        pauseBatch();
        expect(canExport(picked)).toBe(true);
    });
});
