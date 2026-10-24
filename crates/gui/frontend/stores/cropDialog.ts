import { create } from "zustand";

/**
 * Whether the Crop/Rotate dialog is open.
 *
 * A store rather than state in `SidebarMiniature` because the window's key listener opens it (`C` by
 * default), and a listener has no component in reach. The miniature still mounts the dialog. Not
 * persisted: the dialog never reopens on start.
 */
export const useCropDialogStore = create<{ open: boolean; setOpen: (open: boolean) => void }>()((set) => ({
    open: false,
    setOpen: (open: boolean) => set({ open }),
}));
