import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { useSettingsDialog } from "@/features/settings/useSettingsDialog";
import { useSettingsStore } from "@/stores/settings";
import { useSettingsDialogStore } from "./settingsDialog";

beforeEach(() => {
    localStorage.clear();
    useSettingsStore.setState(useSettingsStore.getInitialState(), true);
});

describe("the settings dialog's open state", () => {
    it("opens and closes", () => {
        const { openSettings, closeSettings } = useSettingsDialogStore.getState();

        openSettings();
        expect(useSettingsDialogStore.getState().open).toBe(true);

        closeSettings();
        expect(useSettingsDialogStore.getState().open).toBe(false);
    });

    it("opens from outside the navbar on a fresh draft of what is in force", () => {
        const { result } = renderHook(useSettingsDialog);

        // A visit that changed something and was cancelled.
        act(() => result.current.openSettings());
        act(() => result.current.draft.update({ analytics: false }));
        act(() => result.current.close(false));
        expect(result.current.draft.values.analytics).toBe(false);

        // Then a change to what is in force, and an open from the application's Settings action.
        act(() => useSettingsStore.getState().apply({ background: "dotted" }));
        act(() => useSettingsDialogStore.getState().openSettings());

        expect(result.current.open).toBe(true);
        expect(result.current.draft.values.analytics).toBe(true);
        expect(result.current.draft.values.background).toBe("dotted");
    });

    it("keeps the draft when asked to open while it already is", () => {
        const { result } = renderHook(useSettingsDialog);

        act(() => useSettingsDialogStore.getState().openSettings());
        act(() => result.current.draft.update({ analytics: false }));
        act(() => useSettingsDialogStore.getState().openSettings());

        expect(result.current.draft.values.analytics).toBe(false);
    });
});
