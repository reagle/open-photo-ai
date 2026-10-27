import { create } from "zustand";
import { registerFileOwner } from "@/stores/files";

type FocusPickStore = {
    /** Whether the preview is waiting for a click that says where a lens blur's focus falls. */
    armed: boolean;
    /**
     * The photograph the pick is for, by identity - the key the preview's panes already draw by. Absent unless
     * {@link FocusPickStore.armed}.
     */
    imageId?: string;
    /**
     * How many picks have landed, so a ring that shows for a while after one can tell a new pick from a render. A
     * counter rather than a time, so nothing here reads a clock.
     */
    picks: number;

    /** Arms the preview for `imageId`. */
    arm: (imageId: string) => void;
    /** Leaves the armed state, with nothing picked. */
    disarm: () => void;
    /** Leaves the armed state with a point picked: the click that landed. */
    picked: () => void;
};

// A store rather than state in the preview, because the control that arms it is a button in a popover drawn in the
// sidebar, and the component that answers is two panes down the canvas: they share nothing but the window.
//
// **Not persisted, and not per photograph**: whether the next click is a pick is a moment, not a property of anything.
// Which photograph it is for is held so that a click on another one - or none - is never taken for it.
/** Whether the preview takes its next click as a focus point, and for which photograph. */
export const useFocusPickStore = create<FocusPickStore>()((set) => ({
    armed: false,
    picks: 0,

    arm: (imageId: string) => set({ armed: true, imageId }),

    disarm: () => set({ armed: false }),

    picked: () => set((state) => ({ armed: false, picks: state.picks + 1 })),
}));

/** Whether the preview is armed for the photograph with identity `identity`. */
export const useIsArmed = (identity: string | undefined): boolean =>
    useFocusPickStore((state) => state.armed && identity !== undefined && state.imageId === identity);

/*
 * A file that closes while the preview is armed for it leaves it unarmed: there is no photograph for the click to
 * mean anything on. Registered as a per-file owner - see `FileOwner` - keyed by identity, so a file whose bytes could
 * not be read, which cannot be armed for, has nothing here to forget.
 */
registerFileOwner({
    forget: (_path, identity) => {
        const { armed, imageId, disarm } = useFocusPickStore.getState();

        if (armed && identity !== undefined && imageId === identity) disarm();
    },
    forgetAll: () => useFocusPickStore.getState().disarm(),
});
