import { call } from "./invoke";

// Written a second time in `crates/gui/src/dock.rs`, as the `#[tauri::command]` function's name. A rename on one side
// alone is not a type error but an `invoke` that rejects at runtime: `dock.test.ts` pins this half, and
// `generate_handler![]` the Rust one by refusing to compile against a function that does not exist.
const DOCK_PROGRESS_COMMAND = "dock_progress";

/** Where the batch is, as Rust's `DockProgress` reads it: going or paused, how much is through, how many are left. */
export type DockProgress = {
    state: "running" | "paused";
    /** 0 to 100. */
    percent: number;
    left: number;
};

/**
 * Draws the batch's progress on the Dock tile (macOS) or taskbar button (Windows), with the count left as the tile's
 * badge on macOS - or, given `null`, clears both.
 */
export const setDockProgress = (progress: DockProgress | null) => call<void>(DOCK_PROGRESS_COMMAND, { progress });
