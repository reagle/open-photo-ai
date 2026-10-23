import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call } from "./invoke";

// Written a second time in `crates/gui/src/menu.rs`, as the `#[tauri::command]` function's name and as
// its `EVENT` constant. A rename on one side alone is not a type error but a menu that never answers:
// `menu.test.ts` pins this half, and `generate_handler![]` the command's Rust half.
const CONFIGURE_MENU_COMMAND = "configure_menu";
const MENU_EVENT = "app-menu";

/**
 * Relabels the macOS menu bar in the interface language, and makes Close Image and Close All Images
 * available only while `hasFiles`.
 *
 * `labels` is keyed as the catalogue's `menu.bar` is; Rust ignores a key its menu does not hold. On
 * Windows and Linux, which have no menu bar, the command does nothing.
 */
export const configureMenu = (labels: Record<string, string>, hasFiles: boolean) =>
    call<void>(CONFIGURE_MENU_COMMAND, { labels, hasFiles });

/**
 * Calls `handler` with the id of each macOS menu item of this application's own that is chosen, by
 * click or by its key. Resolves with the function that stops listening.
 */
export const onMenuItem = (handler: (id: unknown) => void): Promise<UnlistenFn> =>
    listen<unknown>(MENU_EVENT, (event) => handler(event.payload));
