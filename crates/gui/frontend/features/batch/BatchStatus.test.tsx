import { act, fireEvent, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
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
import { BatchStatus } from "./BatchStatus";

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

/** Ends the window's run over the photograph on screen. */
const windowRunEnds = async () => {
    const file = useFileStore.getState().files[useFileStore.getState().currentIndex];
    if (file) useBatchStore.getState().record(file.path, requestOf(file)?.key ?? "", false);
    await act(async () => {});
};

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

describe("the batch's status in the navbar", () => {
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
        expect(screen.getByRole("button", { name: "Resume (2 left)" })).toBeInTheDocument();
    });

    it("resumes from the next unfinished photograph", async () => {
        opened(HOLIDAY, SUNSET);
        draw();
        act(() => startBatch([HOLIDAY.path, SUNSET.path]));
        await windowRunEnds();
        fireEvent.click(screen.getByRole("button", { name: "Pause" }));
        act(() => useFileStore.getState().setCurrentIndex(0));

        fireEvent.click(screen.getByRole("button", { name: "Resume (1 left)" }));
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
        expect(screen.getByRole("button", { name: "Resume (3 left)" })).toBeInTheDocument();
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
