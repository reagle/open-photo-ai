import { create } from "zustand";
import { useBatchStore } from "./batch";
import { useFileStore } from "./files";

/**
 * Whether Export can be asked for: at least one image is picked - what the sidebar's Export button is gated on - and no
 * batch is paging through the photographs. An export runs a chain of its own beside the window's, and during a batch
 * that is two heavy runs at once: it waits until the batch is paused or through.
 */
export const canExport = (selected: ReadonlySet<string>, batching = useBatchStore.getState().phase === "running") =>
    selected.size > 0 && !batching;

/**
 * Whether the export dialog is open.
 *
 * A store rather than state in the sidebar because the application's own Export action - the macOS menu's
 * Export… and its keys - opens it from outside the sidebar, and a listener has no component in reach. The sidebar
 * still mounts the dialog, and the dialog still snapshots its queue as it mounts.
 *
 * Not persisted: the dialog never reopens on start.
 */
export const useExportDialogStore = create<{
    open: boolean;
    /** Opens the dialog, as the sidebar's button does - and, like that button, does nothing with nothing picked. */
    openExport: () => void;
    closeExport: () => void;
}>()((set) => ({
    open: false,
    openExport: () => {
        if (canExport(useFileStore.getState().selectedPaths)) set({ open: true });
    },
    closeExport: () => set({ open: false }),
}));

/** Whether Export can be asked for now, as a hook - what the button and the menu item are both enabled by. */
export const useCanExport = () => {
    const batching = useBatchStore((state) => state.phase === "running");

    return useFileStore((state) => canExport(state.selectedPaths, batching));
};

/** Whether Export is held back by a batch that is running, which is what its button's tooltip then says. */
export const useExportHeldByBatch = () => useBatchStore((state) => state.phase === "running");
