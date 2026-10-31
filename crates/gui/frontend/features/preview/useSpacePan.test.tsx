import { act, fireEvent } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LIVE_SURFACE } from "@/lib/layers";
import { useMaskStore } from "@/stores/mask";
import { usePanKeyStore } from "@/stores/panKey";
import { useSettingsStore } from "@/stores/settings";
import { render, resetSettingsStore } from "@/test/support";
import { useSpacePan } from "./useSpacePan";

vi.mock("@/ipc/os", () => ({ isMacOs: vi.fn(() => false), isWindows: vi.fn(() => false) }));

const Probe = () => {
    useSpacePan();

    return (
        <>
            <input aria-label="note" />
            <button type="button">Press</button>
        </>
    );
};

const held = () => usePanKeyStore.getState().held;

const space = (target: Element = document.body) => fireEvent.keyDown(target, { key: " ", code: "Space" });
const release = (target: Element = document.body) => fireEvent.keyUp(target, { key: " ", code: "Space" });

beforeEach(() => {
    usePanKeyStore.setState(usePanKeyStore.getInitialState(), true);
    useMaskStore.setState(useMaskStore.getInitialState(), true);
    resetSettingsStore();
});

describe("Space held as the preview's pan modifier", () => {
    it("is held while Space is down, outside any brush, and the press is taken so the page does not scroll", () => {
        render(<Probe />);

        const down = space();
        expect(held()).toBe(true);
        expect(down).toBe(false);

        release();
        expect(held()).toBe(false);
    });

    it("lets go when the window loses the focus", () => {
        render(<Probe />);
        space();

        fireEvent.blur(window);

        expect(held()).toBe(false);
    });

    it("leaves a focused button its Space, and its release, so the button is pressed as it always is", () => {
        const { getByRole } = render(<Probe />);
        const button = getByRole("button", { name: "Press" });

        const down = space(button);
        const up = release(button);

        expect(held()).toBe(false);
        expect(down).toBe(true);
        expect(up).toBe(true);
    });

    it("leaves a text field its Space", () => {
        const { getByLabelText } = render(<Probe />);

        space(getByLabelText("note"));

        expect(held()).toBe(false);
    });

    it("stands aside for a dialog, but not for an options panel", () => {
        const { rerender } = render(
            <>
                <Probe />
                <div role="dialog" />
            </>,
        );
        space();
        expect(held()).toBe(false);

        rerender(
            <>
                <Probe />
                <div role="dialog" {...{ [LIVE_SURFACE]: "" }} />
            </>,
        );
        space();
        expect(held()).toBe(true);
    });

    it("leaves Space to the action a user bound it to, except while a brush is up", () => {
        act(() => useSettingsStore.getState().apply({ shortcuts: { showOriginal: ["Space"] } }));
        render(<Probe />);

        space();
        expect(held()).toBe(false);

        act(() => useMaskStore.getState().startEditing("/pictures/holiday.jpg"));
        space();
        expect(held()).toBe(true);
    });

    it("lets go when the preview goes", () => {
        const { unmount } = render(<Probe />);
        space();

        unmount();

        expect(held()).toBe(false);
    });
});
