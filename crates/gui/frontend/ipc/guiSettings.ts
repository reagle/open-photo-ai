import type { Family } from "@/ipc/catalogue";
import { call } from "./invoke";

// Written a second time in `crates/gui/src/gui_settings.rs`, as a `#[tauri::command]` function's name. A
// rename on one side alone is not a type error but an `invoke` that rejects at runtime:
// `guiSettings.test.ts` pins this half, and `generate_handler![]` the Rust one by refusing to compile
// against a function that does not exist.
const SET_GUI_SETTINGS_COMMAND = "set_gui_settings";

/**
 * The preferences the command line can follow, in the store's own spellings: Rust's `Published`.
 *
 * `models` holds the store's `<codename>_<precision>` values unparsed - resolving them against the
 * catalogue is the library's job, beside the reader that uses them.
 */
export type PublishedSettings = {
    processor: string;
    autopilotExcluded: Family[];
    models: Partial<Record<Family, string>>;
};

/**
 * Why the preferences could not be published: Rust's `GuiSettingsError`, tagged like every other
 * command's. `message` is the reason in full, in English and untranslated.
 */
export type GuiSettingsError = {
    kind: "setGuiSettings";
    message: string;
};

/**
 * Publish the preferences the command line can follow, for it to read at its next run.
 *
 * Nothing in the GUI reads them back: the settings store stays the only source of what it does.
 */
export const setGuiSettings = (settings: PublishedSettings) => call<void>(SET_GUI_SETTINGS_COMMAND, { settings });
