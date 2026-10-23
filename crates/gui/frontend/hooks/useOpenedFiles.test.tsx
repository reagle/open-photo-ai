import { act, screen } from "@testing-library/react";
import { toast as sonner } from "sonner";
import { beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import "@/i18n";
import i18n from "@/i18n";
import type { ImageRecord } from "@/ipc/images";
import { useFileStore } from "@/stores/files";
import { useSetupStore } from "@/stores/setup";
import { HOLIDAY, render, resetFileStore, resetSetupStore, SUNSET } from "@/test/support";
import { useOpenedFiles } from "./useOpenedFiles";

// Mocked at the wrappers: `ipc/images.test.ts` pins the command and event names. What is kept is the
// inbox's one rule - each path is handed over once - and the event's shape: a listener registered
// asynchronously, answering with the function that removes it.
let inbox: string[] = [];
let announce: (() => void) | undefined;
const unlisten = vi.fn();

vi.mock("@/ipc/images", async (original) => ({
    ...(await original<typeof import("@/ipc/images")>()),
    inputExtensions: vi.fn(() => Promise.resolve(["jpg", "png", "nef"])),
    describeImages: vi.fn(() => Promise.resolve<ImageRecord[]>([])),
    takeOpenedFiles: vi.fn(() => {
        const taken = inbox;
        inbox = [];
        return Promise.resolve(taken);
    }),
    onFilesOpened: vi.fn((handler: () => void) => {
        announce = handler;
        return Promise.resolve(unlisten);
    }),
}));

const { describeImages, takeOpenedFiles } = await import("@/ipc/images");
const described = describeImages as Mock;
const taken = takeOpenedFiles as Mock;

// Mocked at `lib/faro.ts`'s own boundary, as `useDroppedImages.test.tsx` mocks it.
vi.mock("@/lib/faro", () => ({
    track: vi.fn(),
    sendError: vi.fn(),
    pauseFaro: vi.fn(),
    traced: (_name: string, send: () => Promise<unknown>) => send(),
}));
const { track } = await import("@/lib/faro");
const tracked = track as Mock;

/** A component that is nothing but the hook. */
const Opener = () => {
    useOpenedFiles();
    return null;
};

const files = () => useFileStore.getState().files;

/** Lets every pending promise settle, inside `act` so the store's updates are flushed. */
const settle = () => act(async () => {});

/** Setup finishing, which is when the window can first use an image. */
const ready = () => act(() => useSetupStore.setState({ status: "ready" }));

beforeEach(() => {
    resetFileStore();
    resetSetupStore();
    inbox = [];
    announce = undefined;
    described.mockClear();
    taken.mockClear();
    tracked.mockClear();
    sonner.dismiss();
});

describe("files the platform asks the application to open", () => {
    it("waits for setup, then opens what arrived while the application was starting", async () => {
        inbox = [HOLIDAY.path];
        described.mockResolvedValueOnce([HOLIDAY]);

        render(<Opener />);
        await settle();

        // Nothing is taken while setup is still running: nothing could use the image yet.
        expect(taken).not.toHaveBeenCalled();
        expect(announce).toBeUndefined();

        await ready();
        await settle();

        expect(described).toHaveBeenCalledWith([HOLIDAY.path]);
        expect(files()).toEqual([HOLIDAY]);
        expect(tracked).toHaveBeenCalledExactlyOnceWith("files_added", {
            count: 1,
            source: "open_with",
            formats: "png",
        });
    });

    it("opens what arrives once it is ready, when Rust announces it", async () => {
        useSetupStore.setState({ status: "ready" });
        render(<Opener />);
        await settle();

        // The first drain found nothing and opened nothing.
        expect(taken).toHaveBeenCalledTimes(1);
        expect(described).not.toHaveBeenCalled();

        inbox = [HOLIDAY.path, SUNSET.path];
        described.mockResolvedValueOnce([HOLIDAY, SUNSET]);
        await act(async () => announce?.());
        await settle();

        expect(described).toHaveBeenCalledWith([HOLIDAY.path, SUNSET.path]);
        expect(files()).toEqual([HOLIDAY, SUNSET]);
    });

    it("opens each file once, however many announcements there are", async () => {
        useSetupStore.setState({ status: "ready" });
        render(<Opener />);
        await settle();

        inbox = [HOLIDAY.path];
        described.mockResolvedValueOnce([HOLIDAY]);
        await act(async () => {
            announce?.();
            announce?.();
        });
        await settle();

        expect(described).toHaveBeenCalledTimes(1);
        expect(files()).toEqual([HOLIDAY]);
    });

    it("names a file it cannot open, as a drop does, and opens the rest", async () => {
        useSetupStore.setState({ status: "ready" });
        inbox = [HOLIDAY.path, "/Users/someone/Documents/notes.txt"];
        described.mockResolvedValueOnce([HOLIDAY]);

        render(<Opener />);
        await settle();

        expect(described).toHaveBeenCalledWith([HOLIDAY.path]);
        expect(files()).toEqual([HOLIDAY]);
        expect(
            await screen.findByText(i18n.t("toasts.unsupportedFiles", { count: 1, names: "notes.txt" })),
        ).toBeInTheDocument();
        expect(tracked.mock.calls).toEqual([
            ["files_refused", { count: 1, formats: "txt" }],
            ["files_added", { count: 1, source: "open_with", formats: "png" }],
        ]);
    });

    it("stops listening when it unmounts", async () => {
        useSetupStore.setState({ status: "ready" });
        const { unmount } = render(<Opener />);
        await settle();

        unmount();
        await settle();

        expect(unlisten).toHaveBeenCalled();
    });
});
