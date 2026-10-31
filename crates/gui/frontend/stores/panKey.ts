import { create } from "zustand";

// A store rather than state in the preview, because the key is heard on the window and answered by every pane: the
// pane under the pointer has no say in where the focus is. Not persisted: a key held is a moment.
/**
 * Whether Space is held as the preview's pan modifier, which turns a press on the photograph into a pan - a brush's
 * stroke and a focus pick's click alike. Written by `features/preview/useSpacePan.ts` alone.
 */
export const usePanKeyStore = create<{ held: boolean; setHeld: (held: boolean) => void }>()((set) => ({
    held: false,
    setHeld: (held) => set((state) => (state.held === held ? state : { held })),
}));
