import { create } from "zustand";
import { persist } from "zustand/middleware";

type InfoCardStore = {
    /** Whether the card of what the photograph was taken with, and what the stack makes of it, is drawn. */
    visible: boolean;

    toggle: () => void;
};

// Its own store, as the comparison mode has one: it is about the canvas, not a file, and it outlives every image being
// closed and the application being quit. Hidden by default: it is over the photograph.
/** Whether the information card over the preview is showing. Remembered across restarts; hidden at first. */
export const useInfoCardStore = create<InfoCardStore>()(
    persist(
        (set) => ({
            visible: false,

            toggle: () => set((state) => ({ visible: !state.visible })),
        }),
        { name: "info-card-storage" },
    ),
);
