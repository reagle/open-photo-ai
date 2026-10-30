import { invoke } from "@tauri-apps/api/core";
import { act } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import type { Operation } from "@/ipc/enhance";
import type { ImageRecord } from "@/ipc/images";
import { useBatchStore } from "@/stores/batch";
import { useEnhancementStore } from "@/stores/enhancements";
import { useFileStore } from "@/stores/files";
import { APPLY_ORDER, HOLIDAY, openFiles, resetCropStore, resetFileStore, SUNSET } from "@/test/support";

// Mocked at the `invoke` boundary, so what is asserted is what would go to `dock_progress` in crates/gui/src/dock.rs.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock("@/lib/report", () => ({ report: vi.fn() }));
vi.mock("@/lib/chime", () => ({ chime: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn() } }));
vi.mock("@/hooks/useAutopilot", () => ({ awaitAnalysis: vi.fn() }));

const { pauseBatch, requestOf, resumeBatch, startBatch } = await import("./batch");
const { mirrorDock } = await import("./dock");

const invoked = invoke as unknown as Mock;

const HARBOUR: ImageRecord = { ...HOLIDAY, path: "/Users/someone/Pictures/harbour.png", identity: "00112233aabbccdd" };
const sharpen: Operation = { family: "sharpen", codename: "moscow", precision: "fp16", parameters: { strength: 0.5 } };

const opened = (...files: ImageRecord[]) => {
    openFiles(...files);
    for (const file of files) useEnhancementStore.getState().addEnhancement(file.path, sharpen, APPLY_ORDER);
};

const settle = () => act(async () => {});

const windowRunEnds = async () => {
    const file = useFileStore.getState().files[useFileStore.getState().currentIndex];
    if (file) useBatchStore.getState().record(file.path, requestOf(file)?.key ?? "", false);
    await settle();
};

/** Every progress the tile was sent, in order. */
const sent = () =>
    invoked.mock.calls.filter(([command]) => command === "dock_progress").map(([, args]) => args.progress);

let stop: () => void = () => {};

beforeEach(async () => {
    useBatchStore.getState().pause();
    await settle();
    resetFileStore();
    resetCropStore();
    useBatchStore.getState().forgetAll();
    useEnhancementStore.setState({ enhancements: new Map(), autopilot: false });
    invoked.mockClear();
});

afterEach(() => stop());

describe("the batch on the Dock tile", () => {
    it("sends nothing without a batch, however the photographs change", () => {
        stop = mirrorDock();
        opened(HOLIDAY, SUNSET);

        expect(sent()).toEqual([]);
    });

    it("draws the bar and the count left as the batch goes, the paused state while paused, and clears both at the end", async () => {
        opened(HOLIDAY, SUNSET, HARBOUR);
        stop = mirrorDock();

        startBatch([HOLIDAY.path, SUNSET.path, HARBOUR.path]);
        await settle();
        await windowRunEnds();
        pauseBatch();
        resumeBatch();
        await settle();
        await windowRunEnds();
        await windowRunEnds();

        expect(sent()).toEqual([
            { state: "running", percent: 0, left: 3 },
            { state: "running", percent: 33, left: 2 },
            { state: "paused", percent: 33, left: 2 },
            { state: "running", percent: 33, left: 2 },
            { state: "running", percent: 67, left: 1 },
            null,
        ]);
    });

    it("sends each change once, not once per store write", async () => {
        opened(HOLIDAY, SUNSET);
        stop = mirrorDock();
        startBatch([HOLIDAY.path, SUNSET.path]);
        await settle();

        // Writes that change nothing the tile shows.
        useBatchStore.getState().showing(HOLIDAY.path);
        useBatchStore.getState().started();
        useFileStore.getState().toggleSelected(SUNSET.path);

        expect(sent()).toEqual([{ state: "running", percent: 0, left: 2 }]);
    });
});
