import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import type { PublishedSettings } from "@/ipc/guiSettings";
import { report } from "@/lib/report";
import { useSettingsStore } from "@/stores/settings";
import { mirrorGuiSettings } from "./guiSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/lib/report", () => ({ report: vi.fn() }));

const invoked = invoke as unknown as Mock;
const reported = report as unknown as Mock;

/** The settings sent to `set_gui_settings`, in order. */
const sent = () =>
    invoked.mock.calls
        .filter(([command]) => command === "set_gui_settings")
        .map(([, args]) => (args as { settings: PublishedSettings }).settings);

let unsubscribe: () => void = () => {};

beforeEach(() => {
    localStorage.clear();
    useSettingsStore.setState(useSettingsStore.getInitialState(), true);
    invoked.mockReset();
    invoked.mockResolvedValue(undefined);
    reported.mockReset();
});

afterEach(() => unsubscribe());

describe("mirrorGuiSettings", () => {
    it("sends the stored preferences once at boot", async () => {
        localStorage.setItem(
            "settings-storage",
            JSON.stringify({
                state: {
                    processor: "coreml",
                    models: { upscale: "osaka_int8" },
                    autopilotExcluded: ["colorization", "color_balance"],
                },
                version: 0,
            }),
        );
        await useSettingsStore.persist.rehydrate();

        unsubscribe = mirrorGuiSettings();

        expect(sent()).toEqual([
            {
                processor: "coreml",
                models: { upscale: "osaka_int8" },
                autopilotExcluded: ["colorization", "color_balance"],
            },
        ]);
    });

    it("sends them again when a Save changes one of the three", () => {
        unsubscribe = mirrorGuiSettings();

        useSettingsStore.getState().apply({ autopilotExcluded: ["colorization"] });
        useSettingsStore.getState().apply({ models: { denoise: "stockholm_fp16" } });
        useSettingsStore.getState().apply({ processor: "cpu" });

        expect(sent()).toEqual([
            { processor: "auto", models: {}, autopilotExcluded: [] },
            { processor: "auto", models: {}, autopilotExcluded: ["colorization"] },
            { processor: "auto", models: { denoise: "stockholm_fp16" }, autopilotExcluded: ["colorization"] },
            { processor: "cpu", models: { denoise: "stockholm_fp16" }, autopilotExcluded: ["colorization"] },
        ]);
    });

    it("sends nothing for a Save that changes something else, or hands back equal values", () => {
        unsubscribe = mirrorGuiSettings();

        useSettingsStore.getState().apply({ background: "dotted", analytics: false });
        useSettingsStore.getState().apply({ models: {}, autopilotExcluded: [], processor: "auto" });

        expect(sent()).toHaveLength(1);
    });

    it("reports a rejection and keeps the store as it was", async () => {
        invoked.mockRejectedValue({ kind: "setGuiSettings", message: "disk full" });

        unsubscribe = mirrorGuiSettings();

        await vi.waitFor(() => expect(reported).toHaveBeenCalledTimes(1));
        expect(useSettingsStore.getState().processor).toBe("auto");
    });

    it("stops sending once unsubscribed", () => {
        unsubscribe = mirrorGuiSettings();
        unsubscribe();

        useSettingsStore.getState().apply({ processor: "cpu" });

        expect(sent()).toHaveLength(1);
    });
});
