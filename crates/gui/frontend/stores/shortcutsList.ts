import { create } from "zustand";

/**
 * Whether the keyboard shortcuts list is open.
 *
 * A store rather than state in the dialog because the window's key listener opens it, and a listener
 * has no component in reach. Not persisted: the list is something glanced at, never reopened on start.
 */
export const useShortcutsListStore = create<{ open: boolean; setOpen: (open: boolean) => void }>()((set) => ({
    open: false,
    setOpen: (open: boolean) => set({ open }),
}));
