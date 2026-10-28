import { act } from "@testing-library/react";
import { toast } from "sonner";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "@/i18n";
import type { ImageRecord } from "@/ipc/images";
import { useFileStore } from "@/stores/files";
import { useRecentStore } from "@/stores/recent";
import { HOLIDAY, openFiles, resetFileStore, SUNSET } from "@/test/support";
import { openRecent, watchRecent } from "./recent";

// The wire is pinned by `ipc/recent.test.ts`; here, what is told to Rust and what is done with its answers.
let publish: ((paths: string[]) => void) | undefined;
vi.mock("@/ipc/recent", () => ({
    recentImages: vi.fn(() => Promise.resolve(["/a.jpg", "/b.jpg"])),
    noteRecent: vi.fn(() => Promise.resolve()),
    checkRecent: vi.fn(() => Promise.resolve(true)),
    onRecentChanged: vi.fn((handler: (paths: string[]) => void) => {
        publish = handler;
        return Promise.resolve(() => {});
    }),
}));
vi.mock("@/ipc/images", async (original) => ({
    ...(await original<typeof import("@/ipc/images")>()),
    inputExtensions: vi.fn(() => Promise.resolve(["jpg", "png", "nef"])),
    describeImages: vi.fn((paths: string[]) =>
        Promise.resolve(paths.map((path): ImageRecord => ({ path, identity: `id:${path}`, extension: "jpg" }))),
    ),
}));
vi.mock("@/lib/faro", () => ({ track: vi.fn(), sendError: vi.fn(), pauseFaro: vi.fn() }));
vi.mock("sonner", () => ({ toast: { warning: vi.fn() } }));

const { checkRecent, noteRecent } = await import("@/ipc/recent");
const { describeImages } = await import("@/ipc/images");
const { track } = await import("@/lib/faro");

const paths = () => useFileStore.getState().files.map((file) => file.path);
const settle = () => act(async () => {});

let stop: (() => void) | undefined;

beforeEach(() => {
    vi.clearAllMocks();
    resetFileStore();
    useRecentStore.setState({ paths: [] });
});

afterEach(() => {
    stop?.();
    stop = undefined;
});

describe("watchRecent", () => {
    it("fills the list from Rust's, and follows each change it publishes", async () => {
        stop = watchRecent();
        await settle();
        expect(useRecentStore.getState().paths).toEqual(["/a.jpg", "/b.jpg"]);

        act(() => publish?.(["/c.jpg"]));
        expect(useRecentStore.getState().paths).toEqual(["/c.jpg"]);
    });

    it("notes what enters the gallery, in the gallery's order, whatever opened it", () => {
        stop = watchRecent();

        openFiles(HOLIDAY, SUNSET);

        expect(noteRecent).toHaveBeenCalledExactlyOnceWith([
            { path: HOLIDAY.path, identity: HOLIDAY.identity },
            { path: SUNSET.path, identity: SUNSET.identity },
        ]);
    });

    it("notes nothing for a batch that adds nothing new, or for a close", () => {
        openFiles(HOLIDAY, SUNSET);
        stop = watchRecent();

        openFiles(HOLIDAY);
        useFileStore.getState().closeFile(SUNSET.path);
        useFileStore.getState().setCurrentIndex(0);

        expect(noteRecent).not.toHaveBeenCalled();
    });

    it("notes a photograph with no identity by its path alone", () => {
        stop = watchRecent();

        openFiles({ path: "/broken.jpg", extension: "jpg" });

        expect(noteRecent).toHaveBeenCalledExactlyOnceWith([{ path: "/broken.jpg" }]);
    });
});

describe("openRecent", () => {
    it("opens a photograph that is not open, adds it to the gallery and makes it current", async () => {
        openFiles(HOLIDAY);
        stop = watchRecent();

        await act(() => openRecent("/photos/beach.jpg"));

        expect(paths()).toEqual([HOLIDAY.path, "/photos/beach.jpg"]);
        expect(useFileStore.getState().currentIndex).toBe(1);
        expect(track).toHaveBeenCalledWith("files_added", expect.objectContaining({ source: "recent", count: 1 }));
        expect(noteRecent).toHaveBeenCalledExactlyOnceWith([
            { path: "/photos/beach.jpg", identity: "id:/photos/beach.jpg" },
        ]);
    });

    it("makes a photograph that is already open current, and moves it to the top, without opening it again", async () => {
        openFiles(HOLIDAY, SUNSET);
        useFileStore.getState().setCurrentIndex(0);

        await act(() => openRecent(SUNSET.path));

        expect(paths()).toEqual([HOLIDAY.path, SUNSET.path]);
        expect(useFileStore.getState().currentIndex).toBe(1);
        expect(checkRecent).not.toHaveBeenCalled();
        expect(describeImages).not.toHaveBeenCalled();
        expect(noteRecent).toHaveBeenCalledExactlyOnceWith([{ path: SUNSET.path, identity: SUNSET.identity }]);
    });

    it("says a photograph that has gone could not be found, and opens nothing", async () => {
        vi.mocked(checkRecent).mockResolvedValueOnce(false);

        await act(() => openRecent("/photos/gone.jpg"));

        expect(paths()).toEqual([]);
        expect(describeImages).not.toHaveBeenCalled();
        expect(toast.warning).toHaveBeenCalledExactlyOnceWith(
            '"gone.jpg" could not be found. It may have been moved or deleted.',
        );
    });
});
