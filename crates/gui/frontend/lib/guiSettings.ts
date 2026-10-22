import { type PublishedSettings, setGuiSettings } from "@/ipc/guiSettings";
import { report } from "@/lib/report";
import { type SettingsData, useSettingsStore } from "@/stores/settings";

/** The three preferences the command line can follow, out of a store state. */
const published = ({ processor, autopilotExcluded, models }: SettingsData): PublishedSettings => ({
    processor,
    autopilotExcluded,
    models,
});

/** Hands the preferences to Rust, reporting and dropping a rejection: only the command line's copy is stale. */
const publish = (settings: PublishedSettings) => {
    setGuiSettings(settings).catch((error: unknown) => {
        // A browser without Tauri underneath it - `vite` opened directly - rejects every call, as it does
        // the analytics mirror's.
        report("Failed to publish the settings the command line follows", error);
    });
};

/**
 * Keeps the command line's copy of the preferences it can follow - the processor, the enhancements
 * Autopilot may not suggest, and each enhancement's default model - in step with the settings store.
 *
 * Sends them once, now, and again whenever a Save changes one of them, as `mirrorAnalytics` does for the
 * Analytics choice. The store is written only by a Save (and the TensorRT answer), so a draft or a
 * cancelled dialog never reaches the file. **Nothing reads the file back**: the store stays the only
 * source of what the GUI does.
 *
 * Returns the unsubscribe.
 */
export const mirrorGuiSettings = (): (() => void) => {
    publish(published(useSettingsStore.getState()));

    return useSettingsStore.subscribe((state, previous) => {
        // By value rather than by reference: Reset to defaults hands back new, equal records, and a
        // Save of another page's preference carries these three along unchanged.
        const next = published(state);
        if (JSON.stringify(next) !== JSON.stringify(published(previous))) {
            publish(next);
        }
    });
};
