import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { useFileStore } from "@/stores/files";
import { HOLIDAY, openFiles, resetFileStore, SUNSET } from "@/test/support";
import { useFocusPickStore, useIsArmed } from "./focusPick";

const state = () => useFocusPickStore.getState();

/** The identity of a fixture, which every one of them has. */
const identity = (record: { identity?: string }) => record.identity ?? "";

beforeEach(() => {
    useFocusPickStore.setState(useFocusPickStore.getInitialState(), true);
    resetFileStore();
});

describe("the focus pick", () => {
    it("starts unarmed, with nothing picked", () => {
        expect(state().armed).toBe(false);
        expect(state().picks).toBe(0);
    });

    it("is armed for one photograph by arming it for that photograph", () => {
        state().arm(identity(HOLIDAY));

        expect(state().armed).toBe(true);
        expect(state().imageId).toBe(identity(HOLIDAY));
    });

    it("leaves the armed state with nothing picked when it is disarmed", () => {
        state().arm(identity(HOLIDAY));
        state().disarm();

        expect(state().armed).toBe(false);
        expect(state().picks).toBe(0);
    });

    it("leaves the armed state and counts a pick when one lands", () => {
        state().arm(identity(HOLIDAY));
        state().picked();
        state().arm(identity(HOLIDAY));
        state().picked();

        expect(state().armed).toBe(false);
        expect(state().picks).toBe(2);
    });
});

describe("whether a photograph is the one armed for", () => {
    it("is true for that photograph alone", () => {
        const holiday = renderHook(() => useIsArmed(identity(HOLIDAY)));
        const sunset = renderHook(() => useIsArmed(identity(SUNSET)));
        const unread = renderHook(() => useIsArmed(undefined));

        expect([holiday.result.current, sunset.result.current, unread.result.current]).toEqual([false, false, false]);

        act(() => state().arm(identity(HOLIDAY)));

        expect([holiday.result.current, sunset.result.current, unread.result.current]).toEqual([true, false, false]);

        act(() => state().disarm());

        expect(holiday.result.current).toBe(false);
    });
});

describe("closing the photograph armed for", () => {
    it("disarms, and leaves a pick for another photograph alone", () => {
        openFiles(HOLIDAY, SUNSET);

        state().arm(identity(SUNSET));
        act(() => useFileStore.getState().closeFile(HOLIDAY.path));
        expect(state().armed).toBe(true);

        act(() => useFileStore.getState().closeFile(SUNSET.path));
        expect(state().armed).toBe(false);
    });

    it("disarms when every image is closed", () => {
        openFiles(HOLIDAY, SUNSET);

        state().arm(identity(HOLIDAY));
        act(() => useFileStore.getState().closeAll());

        expect(state().armed).toBe(false);
    });
});
