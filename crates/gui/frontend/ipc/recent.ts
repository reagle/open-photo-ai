import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call } from "./invoke";

// Written a second time in `crates/gui/src/recent.rs`, as `#[tauri::command]` function names and its `CHANGED`
// constant, and in `crates/gui/src/menu.rs` as `OPEN_RECENT`. A rename on one side alone is not a type error but a
// list that never fills: `recent.test.ts` pins this half, and `generate_handler![]` the commands' Rust half.
const RECENT_IMAGES_COMMAND = "recent_images";
const NOTE_RECENT_COMMAND = "note_recent";
const CHECK_RECENT_COMMAND = "check_recent";
const CLEAR_RECENT_COMMAND = "clear_recent";
const RECENT_CHANGED_EVENT = "recent-changed";
const OPEN_RECENT_EVENT = "open-recent";

/** One photograph that has entered the gallery, as Rust's `Recent` reads it. */
export type RecentImage = { path: string; identity?: string };

/** The photographs opened most recently, newest first. */
export const recentImages = () => call<string[]>(RECENT_IMAGES_COMMAND);

/** Puts `images`, which have just entered the gallery, at the top of the list in their order. */
export const noteRecent = (images: RecentImage[]) => call<void>(NOTE_RECENT_COMMAND, { images });

/** Whether the photograph at `path` is still there; one that is not leaves the list. */
export const checkRecent = (path: string) => call<boolean>(CHECK_RECENT_COMMAND, { path });

/** Empties the list. */
export const clearRecent = () => call<void>(CLEAR_RECENT_COMMAND);

/** Calls `handler` with the list, newest first, every time it changes. Resolves with the function that stops. */
export const onRecentChanged = (handler: (paths: string[]) => void): Promise<UnlistenFn> =>
    listen<string[]>(RECENT_CHANGED_EVENT, (event) => handler(event.payload));

/**
 * Calls `handler` with the path of each Open Recent entry chosen from the macOS menu bar. Resolves with the function
 * that stops listening.
 */
export const onOpenRecent = (handler: (path: unknown) => void): Promise<UnlistenFn> =>
    listen<unknown>(OPEN_RECENT_EVENT, (event) => handler(event.payload));
