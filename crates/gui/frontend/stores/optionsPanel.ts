import { create } from "zustand";
import type { Operation } from "@/ipc/enhance";

type Family = Operation["family"];

/**
 * Which enhancement's options panel is open, if any - one at a time, as the popovers already behave.
 *
 * A store rather than state in each `EnhancementRow` because the window's key listener opens one by its
 * place in the list (`1` to `8` by default), and a listener has no component in reach. Keyed by family,
 * which the one-entry-per-family rule makes unique within a stack. Not persisted.
 */
export const useOptionsPanelStore = create<{
    family: Family | undefined;
    openPanel: (family: Family) => void;
    /**
     * Closes this family's panel, and only this family's: when a press on a second row opens its panel,
     * the first panel's close can be reported after the second one's open, and must not undo it.
     */
    closePanel: (family: Family) => void;
}>()((set) => ({
    family: undefined,
    openPanel: (family: Family) => set({ family }),
    closePanel: (family: Family) => set((state) => (state.family === family ? { family: undefined } : state)),
}));
