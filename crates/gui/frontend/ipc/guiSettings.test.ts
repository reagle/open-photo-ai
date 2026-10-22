import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, type Mock, vi } from "vitest";
import { type GuiSettingsError, setGuiSettings } from "./guiSettings";

// Mocked at the `invoke` boundary, so the name asserted below is the one that would actually go on
// the wire. This is the frontend half of a contract whose Rust half is `#[tauri::command] fn
// set_gui_settings` in crates/gui/src/gui_settings.rs, whose `Published` reads these field names.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invoked = invoke as unknown as Mock;

describe("setGuiSettings", () => {
    it("calls the command Rust registers, by name, with the settings under the argument Rust reads", () => {
        const settings = {
            processor: "coreml",
            autopilotExcluded: ["color_balance" as const],
            models: { upscale: "osaka_int8" },
        };

        setGuiSettings(settings);

        expect(invoked).toHaveBeenCalledWith("set_gui_settings", { settings });
    });

    it("propagates a failure rather than swallowing it", async () => {
        // The shape Rust's `the_rejection_crosses_under_its_kind` pins from the other side.
        const rejection: GuiSettingsError = {
            kind: "setGuiSettings",
            message: "the settings for the command line could not be saved: disk full",
        };
        invoked.mockRejectedValueOnce(rejection);

        await expect(setGuiSettings({ processor: "auto", autopilotExcluded: [], models: {} })).rejects.toEqual(
            rejection,
        );
    });
});
