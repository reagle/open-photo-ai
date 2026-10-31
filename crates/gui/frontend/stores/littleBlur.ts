import { create } from "zustand";
import { registerFileOwner } from "@/stores/files";

type LittleBlurStore = {
    /**
     * The photographs, by identity, whose last finished run carried a lens blur too small to see: `opai`'s
     * `ChainMatte::LittleBlur`, which the Lens Blur panel says with what to change.
     */
    little: ReadonlySet<string>;

    /** Records what the latest run over `identity` came to: whether its lens blur added too little blur to see. */
    ran: (identity: string, little: boolean) => void;
};

// A store rather than state in the run hook, because the run is the preview's and the notice is the Lens Blur panel's,
// drawn in a popover in the sidebar: they share nothing but the window.
//
// **Cleared by the next run, not by a timer or by the panel**: every finished run over a photograph records its answer,
// so a change of focus or f-number that gives a visible blur takes the notice down as the result that shows it arrives,
// and one that still gives none leaves it up. Not persisted: it is a fact about the last render, not about the stack.
/** Which photographs' lens blurs last rendered too little blur to see. */
export const useLittleBlurStore = create<LittleBlurStore>()((set) => ({
    little: new Set(),

    ran: (identity, little) =>
        set((state) => {
            if (state.little.has(identity) === little) return state;

            const next = new Set(state.little);
            if (little) next.add(identity);
            else next.delete(identity);

            return { little: next };
        }),
}));

/** Whether the last run over the photograph with identity `identity` carried a lens blur too small to see. */
export const useLittleBlur = (identity: string | undefined): boolean =>
    useLittleBlurStore((state) => identity !== undefined && state.little.has(identity));

/* A photograph that closes takes its notice with it, keyed by identity as the store is. */
registerFileOwner({
    forget: (_path, identity) => {
        if (identity !== undefined) useLittleBlurStore.getState().ran(identity, false);
    },
    forgetAll: () => useLittleBlurStore.setState({ little: new Set() }),
});
