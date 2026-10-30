import { type DockProgress, setDockProgress } from "@/ipc/dock";
import { progressOf } from "@/lib/batch";
import { report } from "@/lib/report";
import { useBatchStore } from "@/stores/batch";
import { useCropStore } from "@/stores/crop";
import { useEnhancementStore } from "@/stores/enhancements";
import { useFacesStore } from "@/stores/faces";
import { useFileStore } from "@/stores/files";
import { useMaskStore } from "@/stores/mask";
import { useSettingsStore } from "@/stores/settings";

/** Every store where the batch stands is read from - the ones `hooks/useBatch.ts` watches for the pill. */
const READ = [
    useBatchStore,
    useFileStore,
    useEnhancementStore,
    useCropStore,
    useFacesStore,
    useMaskStore,
    useSettingsStore,
];

/**
 * What the Dock tile should show for the batch now: its bar and the count left while it runs, the same in the paused
 * state while it is paused, and nothing once no photograph is left or there is no batch - no batch, or one through, whose end the
 * toast and the chime have already said.
 */
export const dockProgressOf = (): DockProgress | null => {
    const { phase } = useBatchStore.getState();
    if (phase !== "running" && phase !== "paused") return null;

    const { done, failed, left, total } = progressOf();
    // None left is a batch that is through, a moment before its phase says so: cleared rather than drawn full.
    if (left === 0) return null;

    return { state: phase, percent: Math.round((100 * (done + failed)) / total), left };
};

const same = (a: DockProgress | null, b: DockProgress | null) =>
    a === b || (a !== null && b !== null && a.state === b.state && a.percent === b.percent && a.left === b.left);

/**
 * Keeps the Dock tile (the taskbar button on Windows) in step with the batch: sends what {@link dockProgressOf}
 * answers whenever the answer changes, and nothing in between - a stroke on a mask costs one comparison, not a
 * request. Nothing is sent at the start, when there is no batch and the tile has nothing on it.
 *
 * Returns the unsubscribe.
 */
export const mirrorDock = (): (() => void) => {
    let shown: DockProgress | null = null;

    const refresh = () => {
        const next = dockProgressOf();
        if (same(shown, next)) return;

        shown = next;
        setDockProgress(next).catch((error: unknown) => {
            // A browser without Tauri underneath it rejects every call, as it does the analytics mirror's.
            report("Failed to draw the batch's progress on the Dock", error);
        });
    };

    const unsubscribes = READ.map((store) => (store.subscribe as (listener: () => void) => () => void)(refresh));

    return () => {
        for (const unsubscribe of unsubscribes) unsubscribe();
    };
};
