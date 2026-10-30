import { act, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "@/i18n";
import type { Operation } from "@/ipc/enhance";
import type { ImageRecord } from "@/ipc/images";
import { requestOf, startBatch } from "@/lib/batch";
import { useBatchStore } from "@/stores/batch";
import { useEnhancementStore } from "@/stores/enhancements";
import { useFileStore } from "@/stores/files";
import { useSettingsStore } from "@/stores/settings";
import { APPLY_ORDER, HOLIDAY, openFiles, render, resetCropStore, resetFileStore, SUNSET } from "@/test/support";
import { BatchPauseDialog } from "./BatchPauseDialog";
import { BatchStatus, DONE_FADE_MS, DONE_SHOWN_MS } from "./BatchStatus";

// The global Tauri's init script installs, which jsdom has none of: the list draws each photograph's thumbnail.
vi.mock("@tauri-apps/api/core", () => ({
    convertFileSrc: vi.fn((identity: string) => `opai://localhost/${identity}`),
}));
vi.mock("@/lib/report", () => ({ report: vi.fn() }));
vi.mock("@/lib/chime", () => ({ chime: vi.fn() }));
vi.mock("@/hooks/useAutopilot", () => ({ awaitAnalysis: vi.fn() }));

const HARBOUR: ImageRecord = { ...HOLIDAY, path: "/Users/someone/Pictures/harbour.png", identity: "00112233aabbccdd" };
const sharpen: Operation = { family: "sharpen", codename: "moscow", precision: "fp16", parameters: { strength: 0.5 } };

const opened = (...files: ImageRecord[]) => {
    openFiles(...files);
    for (const file of files) useEnhancementStore.getState().addEnhancement(file.path, sharpen, APPLY_ORDER);
};

const current = () => useFileStore.getState().files[useFileStore.getState().currentIndex]?.path;

/** Ends the window's run over the photograph on screen, as a failure if `failed`. */
const windowRunEnds = async (failed = false) => {
    const file = useFileStore.getState().files[useFileStore.getState().currentIndex];
    if (file) useBatchStore.getState().record(file.path, requestOf(file)?.key ?? "", failed);
    await act(async () => {});
};

const pill = () => document.querySelector("[data-slot='batch-status']");

const draw = () =>
    render(
        <>
            <BatchStatus />
            <BatchPauseDialog />
        </>,
    );

beforeEach(async () => {
    useBatchStore.getState().pause();
    await act(async () => {});
    resetFileStore();
    resetCropStore();
    useBatchStore.getState().forgetAll();
    useEnhancementStore.setState({ enhancements: new Map(), autopilot: false });
    useSettingsStore.setState({ pauseBatchWithoutAsking: false });
});

describe("the batch's pill in the navbar", () => {
    it("offers to process every photograph with enhancements the window has not run, from two of them", async () => {
        opened(HOLIDAY);
        draw();
        expect(screen.queryByRole("button", { name: /Process all/ })).not.toBeInTheDocument();

        act(() => opened(SUNSET, HARBOUR));
        expect(screen.getByRole("button", { name: "Process all (3)" })).toBeInTheDocument();

        fireEvent.click(screen.getByRole("button", { name: "Process all (3)" }));
        await act(async () => {});

        expect(useBatchStore.getState().phase).toBe("running");
        expect(current()).toBe(HOLIDAY.path);
    });

    it("says how far it has got while it runs, and Pause pauses it", async () => {
        opened(HOLIDAY, SUNSET, HARBOUR);
        draw();
        act(() => startBatch([HOLIDAY.path, SUNSET.path, HARBOUR.path]));
        await windowRunEnds();

        expect(screen.getByRole("status")).toHaveTextContent("Processing 2 of 3");

        fireEvent.click(screen.getByRole("button", { name: "Pause" }));

        expect(useBatchStore.getState().phase).toBe("paused");
        expect(screen.getByRole("status")).toHaveTextContent("Paused — 2 left");
        expect(screen.getByRole("button", { name: "Resume" })).toBeInTheDocument();
    });

    it("resumes from the next unfinished photograph", async () => {
        opened(HOLIDAY, SUNSET);
        draw();
        act(() => startBatch([HOLIDAY.path, SUNSET.path]));
        await windowRunEnds();
        fireEvent.click(screen.getByRole("button", { name: "Pause" }));
        act(() => useFileStore.getState().setCurrentIndex(0));

        fireEvent.click(screen.getByRole("button", { name: "Resume" }));
        await act(async () => {});

        expect(current()).toBe(SUNSET.path);
    });
});

describe("the question a click during the batch asks", () => {
    it("is asked, and Pause & view pauses the batch and shows the photograph", async () => {
        opened(HOLIDAY, SUNSET, HARBOUR);
        draw();
        act(() => startBatch([HOLIDAY.path, SUNSET.path, HARBOUR.path]));
        await act(async () => {});

        act(() => useFileStore.getState().setCurrentIndex(2));

        expect(screen.getByRole("dialog", { name: "Pause processing and view this photo?" })).toBeInTheDocument();
        expect(current()).toBe(HOLIDAY.path);

        fireEvent.click(screen.getByRole("button", { name: "Pause & view" }));
        await act(async () => {});

        expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
        expect(current()).toBe(HARBOUR.path);
        expect(screen.getByRole("status")).toHaveTextContent("Paused — 3 left");
    });

    it("leaves the batch running on Keep processing", async () => {
        opened(HOLIDAY, SUNSET);
        draw();
        act(() => startBatch([HOLIDAY.path, SUNSET.path]));
        await act(async () => {});
        act(() => useFileStore.getState().setCurrentIndex(1));

        fireEvent.click(screen.getByRole("button", { name: "Keep processing" }));

        expect(useBatchStore.getState().phase).toBe("running");
        expect(current()).toBe(HOLIDAY.path);
    });

    it("remembers Don't ask again with Pause & view", async () => {
        opened(HOLIDAY, SUNSET);
        draw();
        act(() => startBatch([HOLIDAY.path, SUNSET.path]));
        await act(async () => {});
        act(() => useFileStore.getState().setCurrentIndex(1));

        fireEvent.click(screen.getByRole("checkbox", { name: "Don't ask again" }));
        fireEvent.click(screen.getByRole("button", { name: "Pause & view" }));

        expect(useSettingsStore.getState().pauseBatchWithoutAsking).toBe(true);
    });
});

describe("the pill's look", () => {
    afterEach(() => {
        vi.useRealTimers();
    });

    it("is absent with no batch and nothing to process", () => {
        opened(HOLIDAY);
        draw();

        expect(pill()).toBeNull();
        expect(screen.queryByRole("status")).not.toBeInTheDocument();
    });

    it("is the accent while it runs, with a ring for how much is through, and amber, saying Paused, when paused", async () => {
        opened(HOLIDAY, SUNSET, HARBOUR);
        draw();
        act(() => startBatch([HOLIDAY.path, SUNSET.path, HARBOUR.path]));
        await windowRunEnds();

        expect(pill()).toHaveAttribute("data-tone", "running");
        expect(pill()?.querySelector("[data-slot='batch-ring']")).toHaveAttribute("data-fraction", "0.33");

        fireEvent.click(screen.getByRole("button", { name: "Pause" }));

        expect(pill()).toHaveAttribute("data-tone", "paused");
        expect(screen.queryByRole("button", { name: "Pause" })).not.toBeInTheDocument();
        // A live region of its own, polite, so each step is read out without interrupting.
        expect(screen.getByRole("status")).toHaveTextContent("Paused — 2 left");
    });

    it("says Done with the counts once through, fades after a few seconds and then goes", async () => {
        vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
        opened(HOLIDAY, SUNSET);
        draw();
        act(() => startBatch([HOLIDAY.path, SUNSET.path]));
        await windowRunEnds(true);
        await windowRunEnds();

        expect(useBatchStore.getState().phase).toBe("finished");
        expect(pill()).toHaveAttribute("data-tone", "done");
        expect(screen.getByRole("status")).toHaveTextContent("Done — 2 processed (1 failed)");

        act(() => vi.advanceTimersByTime(DONE_SHOWN_MS));
        expect(pill()).toHaveClass("opacity-0");

        act(() => vi.advanceTimersByTime(DONE_FADE_MS));
        expect(pill()).toBeNull();
    });

    it("goes at once when the Done pill is pressed", async () => {
        opened(HOLIDAY, SUNSET);
        draw();
        act(() => startBatch([HOLIDAY.path, SUNSET.path]));
        await windowRunEnds();
        await windowRunEnds();

        fireEvent.click(screen.getByRole("button", { name: "Done — 2 processed" }));

        expect(pill()).toBeNull();
    });
});

describe("the list the pill opens", () => {
    const rows = () => within(screen.getByRole("list", { name: "Photos in this batch" })).getAllByRole("button");

    it("lists the batch's photographs in the strip's order, each with where it stands", async () => {
        opened(HOLIDAY, SUNSET, HARBOUR);
        draw();
        act(() => startBatch([HOLIDAY.path, SUNSET.path, HARBOUR.path]));
        await windowRunEnds(true);

        fireEvent.click(screen.getByRole("button", { name: "Processing 2 of 3" }));

        expect(rows().map((row) => [row.textContent, row.dataset.state])).toEqual([
            ["holiday.pngFailed", "failed"],
            ["sunset over the harbour.nefProcessing", "processing"],
            ["harbour.pngPending", "pending"],
        ]);
    });

    it("says done for a photograph the batch has finished, and processing for none while paused", async () => {
        opened(HOLIDAY, SUNSET);
        draw();
        act(() => startBatch([HOLIDAY.path, SUNSET.path]));
        await windowRunEnds();
        fireEvent.click(screen.getByRole("button", { name: "Pause" }));

        fireEvent.click(screen.getByRole("button", { name: "Paused — 1 left" }));

        expect(rows().map((row) => row.dataset.state)).toEqual(["done", "pending"]);
    });

    it("goes to a photograph through the question while the batch runs", async () => {
        opened(HOLIDAY, SUNSET, HARBOUR);
        draw();
        act(() => startBatch([HOLIDAY.path, SUNSET.path, HARBOUR.path]));
        await act(async () => {});

        fireEvent.click(screen.getByRole("button", { name: "Processing 1 of 3" }));
        fireEvent.click(screen.getByRole("button", { name: /harbour\.png/ }));

        expect(screen.getByRole("dialog", { name: "Pause processing and view this photo?" })).toBeInTheDocument();
        expect(current()).toBe(HOLIDAY.path);
    });

    it("goes straight to a photograph while the batch is paused", async () => {
        opened(HOLIDAY, SUNSET, HARBOUR);
        draw();
        act(() => startBatch([HOLIDAY.path, SUNSET.path, HARBOUR.path]));
        await act(async () => {});
        fireEvent.click(screen.getByRole("button", { name: "Pause" }));

        fireEvent.click(screen.getByRole("button", { name: "Paused — 3 left" }));
        fireEvent.click(screen.getByRole("button", { name: /harbour\.png/ }));

        expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
        expect(current()).toBe(HARBOUR.path);
    });
});
