import { create } from "zustand";

type RecentStore = {
    /** The photographs opened most recently, newest first, as Rust last published them. */
    paths: string[];
    setPaths: (paths: string[]) => void;
};

// A mirror rather than an owner: the list is Rust's (`crates/gui/src/recent.rs`), which writes it to the state file
// and shows it in the macOS menu bar. This copy is what the Windows and Linux Open Recent button draws, filled at boot
// and on every `recent-changed` by `watchRecent` in `lib/recent.ts`.
/**
 * The photographs opened most recently, as Rust holds them. Written only by `watchRecent`.
 *
 * **Not persisted** here: Rust keeps the list.
 */
export const useRecentStore = create<RecentStore>()((set) => ({
    paths: [],
    setPaths: (paths) => set({ paths }),
}));
