import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import { checkRecent, clearRecent, noteRecent, onOpenRecent, onRecentChanged, recentImages } from "./recent";

// Mocked at the boundary, so the names asserted below are the ones that would go on the wire: the frontend half of a
// contract whose Rust half is `crates/gui/src/recent.rs` and `OPEN_RECENT` in `crates/gui/src/menu.rs`.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

beforeEach(() => {
    vi.clearAllMocks();
});

describe("the recent images' commands", () => {
    it("call the commands Rust registers, by name, with their arguments", () => {
        recentImages();
        noteRecent([{ path: "/a.jpg", identity: "abc" }]);
        checkRecent("/a.jpg");
        clearRecent();

        const calls = (invoke as unknown as Mock).mock.calls.map(([name, args]) => [name, args]);
        expect(calls).toEqual([
            ["recent_images", undefined],
            ["note_recent", { images: [{ path: "/a.jpg", identity: "abc" }] }],
            ["check_recent", { path: "/a.jpg" }],
            ["clear_recent", undefined],
        ]);
    });
});

describe("the recent images' events", () => {
    it("listen for the events Rust emits and hand over their payloads", () => {
        const changed = vi.fn();
        const chosen = vi.fn();
        onRecentChanged(changed);
        onOpenRecent(chosen);

        const [[first, onChanged], [second, onChosen]] = (listen as unknown as Mock).mock.calls as [
            [string, (event: object) => void],
            [string, (event: object) => void],
        ];
        expect([first, second]).toEqual(["recent-changed", "open-recent"]);

        onChanged({ payload: ["/a.jpg"] });
        onChosen({ payload: "/a.jpg" });
        expect(changed).toHaveBeenCalledWith(["/a.jpg"]);
        expect(chosen).toHaveBeenCalledWith("/a.jpg");
    });
});
