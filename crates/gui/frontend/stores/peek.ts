import { create } from "zustand";

type PeekStore = {
    /** Whether the show-original key is held: every pane that draws the result draws the source instead. */
    showingOriginal: boolean;

    setShowingOriginal: (showing: boolean) => void;
};

// **Its own store, not a field on `stores/preview.ts`**, which persists its whole state into the Wails
// application's `app-storage` key and has to hold exactly the one field that application wrote. This
// is not a preference at all: it is true only for as long as a key is physically held down, so it is
// never persisted and starts false on every launch.
/** A glance at the original while a key is held. Written by `hooks/useShortcuts.ts`, read by the canvas. */
export const usePeekStore = create<PeekStore>()((set) => ({
    showingOriginal: false,

    setShowingOriginal: (showing: boolean) => set({ showingOriginal: showing }),
}));
