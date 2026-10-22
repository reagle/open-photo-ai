import { beforeEach, describe, expect, it } from "vitest";
import { usePeekStore } from "./peek";

beforeEach(() => {
    localStorage.clear();
    usePeekStore.setState(usePeekStore.getInitialState(), true);
});

describe("usePeekStore", () => {
    it("shows the result until a key is held", () => {
        expect(usePeekStore.getState().showingOriginal).toBe(false);
    });

    it("shows the original while held and the result once released", () => {
        usePeekStore.getState().setShowingOriginal(true);
        expect(usePeekStore.getState().showingOriginal).toBe(true);

        usePeekStore.getState().setShowingOriginal(false);
        expect(usePeekStore.getState().showingOriginal).toBe(false);
    });

    it("writes nothing to storage, because a held key is not a preference", async () => {
        usePeekStore.getState().setShowingOriginal(true);
        await Promise.resolve();

        expect(localStorage.length).toBe(0);
    });
});
