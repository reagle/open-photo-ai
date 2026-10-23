import { beforeEach, describe, expect, it } from "vitest";
import { useOptionsPanelStore } from "./optionsPanel";

describe("useOptionsPanelStore", () => {
    beforeEach(() => useOptionsPanelStore.setState(useOptionsPanelStore.getInitialState(), true));

    it("holds no panel open until asked, and one at a time", () => {
        expect(useOptionsPanelStore.getState().family).toBeUndefined();

        useOptionsPanelStore.getState().openPanel("denoise");
        useOptionsPanelStore.getState().openPanel("upscale");

        expect(useOptionsPanelStore.getState().family).toBe("upscale");
    });

    it("ignores a close reported late by the panel a second one replaced", () => {
        const { openPanel, closePanel } = useOptionsPanelStore.getState();
        openPanel("denoise");
        openPanel("upscale");

        closePanel("denoise");
        expect(useOptionsPanelStore.getState().family).toBe("upscale");

        closePanel("upscale");
        expect(useOptionsPanelStore.getState().family).toBeUndefined();
    });
});
