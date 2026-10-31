import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, type Mock, vi } from "vitest";
import { setDockProgress } from "./dock";

// Mocked at the `invoke` boundary, so the name asserted below is the one that would actually go on the wire. This is
// the frontend half of a contract whose Rust half is `#[tauri::command] fn dock_progress` in crates/gui/src/dock.rs,
// where `the_progress_crosses_the_boundary_as_the_window_writes_it` reads the same shape.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invoked = invoke as unknown as Mock;

describe("setDockProgress", () => {
    it("sends the batch's progress to the command Rust registers, by name", () => {
        setDockProgress({ state: "paused", percent: 50, left: 2 });

        expect(invoked).toHaveBeenCalledWith("dock_progress", { progress: { state: "paused", percent: 50, left: 2 } });
    });

    it("sends null to clear the tile", () => {
        setDockProgress(null);

        expect(invoked).toHaveBeenCalledWith("dock_progress", { progress: null });
    });
});
