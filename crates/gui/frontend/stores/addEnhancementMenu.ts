import { create } from "zustand";

/**
 * Whether the Add enhancement menu is open.
 *
 * A store rather than state in `AddEnhancement` because the window's key listener opens it (`E` by
 * default), and a listener has no component in reach. Not persisted: a menu never reopens on start.
 */
export const useAddEnhancementMenuStore = create<{ open: boolean; setOpen: (open: boolean) => void }>()((set) => ({
    open: false,
    setOpen: (open: boolean) => set({ open }),
}));
