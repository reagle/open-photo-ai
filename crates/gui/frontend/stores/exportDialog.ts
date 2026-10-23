import { create } from "zustand";
import { useFileStore } from "./files";

/** Whether Export can be asked for: at least one image is picked - what the sidebar's Export button is gated on. */
export const canExport = (selected: ReadonlySet<string>) => selected.size > 0;

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
export const useCanExport = () => useFileStore((state) => canExport(state.selectedPaths));
