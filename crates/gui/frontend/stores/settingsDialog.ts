import { create } from "zustand";

/**
 * Whether the settings dialog is open, and how many times it has been asked to open.
 *
 * A store rather than state in the navbar because the application's own Settings action - the macOS
 * menu's Settings… and Ctrl+comma elsewhere - opens it from outside the navbar, and a listener has no
 * component in reach. The draft stays where it was, in `useSettingsDialog`: `opened` is how that hook
 * hears of an open it did not make, so it can re-seed the draft before the dialog is drawn.
 *
 * Not persisted: the dialog never reopens on start.
 */
export const useSettingsDialogStore = create<{
    open: boolean;
    /** Counts the opens, so the hook holding the draft sees each one, whoever made it. */
    opened: number;
    /** Opens the dialog on a draft of what is in force. */
    openSettings: () => void;
    /** Closes it; what happens to the draft is `useSettingsDialog`'s `close`. */
    closeSettings: () => void;
}>()((set) => ({
    open: false,
    opened: 0,
    // A request while it is open changes nothing: re-seeding then would throw away the user's edits.
    openSettings: () => set((state) => (state.open ? state : { open: true, opened: state.opened + 1 })),
    closeSettings: () => set({ open: false }),
}));
