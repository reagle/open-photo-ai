import { act, fireEvent } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SIZE_DEFAULT } from "@/lib/mask";
import { useMaskStore } from "@/stores/mask";
import { render } from "@/test/support";
import { useMaskKeys } from "./useMaskKeys";

const PATH = "/pictures/holiday.jpg";

const Probe = () => {
    useMaskKeys(PATH);

    return <input aria-label="note" />;
};

const store = () => useMaskStore.getState();

const paint = () =>
    act(() => {
        store().beginStroke(PATH, 0.2, 0.2, false);
        store().endStroke(0.4, 0.2);
    });

/** A key pressed on `target`, which bubbles up to the window as a real one does. */
const press = (key: string, init: KeyboardEventInit = {}, target: Element = document.body) =>
    fireEvent.keyDown(target, { key, ...init });

beforeEach(() => {
    useMaskStore.setState(useMaskStore.getInitialState(), true);
    store().startEditing(PATH);
});

describe("the keys of editing a mask", () => {
    it("choose paint and erase, and swap them", () => {
        render(<Probe />);

        press("e", { code: "KeyE" });
        expect(store().tool.mode).toBe("erase");

        press("b", { code: "KeyB" });
        expect(store().tool.mode).toBe("paint");

        press("x", { code: "KeyX" });
        expect(store().tool.mode).toBe("erase");

        press("X", { code: "KeyX" });
        expect(store().tool.mode).toBe("paint");
    });

    it("are the keys where they are, whatever the layout spells on them", () => {
        render(<Probe />);

        // A Russian layout: the keys that are B, E and O on a US one type "и", "у" and "щ".
        press("у", { code: "KeyE" });
        expect(store().tool.mode).toBe("erase");

        press("и", { code: "KeyB" });
        expect(store().tool.mode).toBe("paint");

        press("щ", { code: "KeyO" });
        expect(store().tool.overlay).toBe(false);

        // A letter that is a brush key on some other layout's key is not one here.
        press("b", { code: "KeyN" });
        expect(store().tool.mode).toBe("paint");
        press("e", { code: "KeyK" });
        expect(store().tool.mode).toBe("paint");
    });

    it("show and hide the tint", () => {
        render(<Probe />);

        press("o", { code: "KeyO" });
        expect(store().tool.overlay).toBe(false);

        press("o", { code: "KeyO" });
        expect(store().tool.overlay).toBe(true);
    });

    it("change the size with the brackets, and the softness with Shift", () => {
        render(<Probe />);

        press("]", { code: "BracketRight" });
        expect(store().tool.size).toBeGreaterThan(SIZE_DEFAULT);

        press("[", { code: "BracketLeft" });
        press("[", { code: "BracketLeft" });
        expect(store().tool.size).toBeLessThan(SIZE_DEFAULT);

        const size = store().tool.size;

        // Shift+[ types `{` on a US keyboard: it is the key that counts, not the character.
        press("}", { code: "BracketRight", shiftKey: true });
        expect(store().tool.softness).toBeCloseTo(0.6);

        press("{", { code: "BracketLeft", shiftKey: true });
        press("{", { code: "BracketLeft", shiftKey: true });
        expect(store().tool.softness).toBeCloseTo(0.4);
        expect(store().tool.size).toBe(size);
    });

    it("undo and redo with Cmd or Ctrl and Z, and Shift", () => {
        render(<Probe />);
        paint();
        paint();

        press("z", { metaKey: true });
        expect(store().masks.get(PATH)?.strokes).toHaveLength(1);

        press("z", { ctrlKey: true });
        expect(store().masks.get(PATH)?.strokes).toHaveLength(0);

        press("z", { metaKey: true, shiftKey: true });
        expect(store().masks.get(PATH)?.strokes).toHaveLength(1);

        press("Z", { ctrlKey: true, shiftKey: true });
        expect(store().masks.get(PATH)?.strokes).toHaveLength(2);

        press("z", { ctrlKey: true });
        press("y", { ctrlKey: true });
        expect(store().masks.get(PATH)?.strokes).toHaveLength(2);
    });

    it("keep Undo and Redo from every other handler, so Cmd+Z undoes a stroke and nothing else", () => {
        render(<Probe />);
        paint();
        const elsewhere = vi.fn();
        window.addEventListener("keydown", elsewhere);
        document.body.addEventListener("keydown", elsewhere);

        press("z", { metaKey: true });

        expect(elsewhere).not.toHaveBeenCalled();
        window.removeEventListener("keydown", elsewhere);
        document.body.removeEventListener("keydown", elsewhere);
    });

    it("leave editing on Escape", () => {
        render(<Probe />);

        press("Escape");

        expect(store().editing).toBeUndefined();
    });

    it("hold Space to pan, and let go with it", () => {
        render(<Probe />);

        press(" ", { code: "Space" });
        expect(store().panning).toBe(true);

        fireEvent.keyUp(document.body, { key: " ", code: "Space" });
        expect(store().panning).toBe(false);
    });

    it("let go of Space when the window loses the focus", () => {
        render(<Probe />);
        press(" ", { code: "Space" });

        fireEvent.blur(window);

        expect(store().panning).toBe(false);
    });

    it("leave a button its Space, and its release", () => {
        const { getByRole } = render(
            <>
                <Probe />
                <button type="button">Press</button>
            </>,
        );
        const button = getByRole("button", { name: "Press" });

        const down = fireEvent.keyDown(button, { key: " ", code: "Space" });
        const up = fireEvent.keyUp(button, { key: " ", code: "Space" });

        expect(store().panning).toBe(false);
        // Not cancelled, so the button is pressed as it always is.
        expect(down).toBe(true);
        expect(up).toBe(true);
    });

    it("do nothing while a text field has the keys", () => {
        const { getByLabelText } = render(<Probe />);
        const field = getByLabelText("note");

        press("e", { code: "KeyE" }, field);
        press(" ", { code: "Space" }, field);
        press("Escape", {}, field);

        expect(store().tool.mode).toBe("paint");
        expect(store().panning).toBe(false);
        expect(store().editing).toEqual({ path: PATH });
    });

    it("do nothing while a dialog or a menu is open", () => {
        render(
            <>
                <Probe />
                <div role="dialog" />
            </>,
        );

        press("e", { code: "KeyE" });
        press("Escape");

        expect(store().tool.mode).toBe("paint");
        expect(store().editing).toEqual({ path: PATH });
    });

    it("leave a chord to whoever it belongs to", () => {
        render(<Probe />);

        press("e", { code: "KeyE", metaKey: true });
        press("b", { code: "KeyB", ctrlKey: true });
        press("e", { code: "KeyE", altKey: true });
        press("e", { code: "KeyE", shiftKey: true });

        expect(store().tool.mode).toBe("paint");
    });

    it("stop listening, and stop panning, when the panel goes", () => {
        const { unmount } = render(<Probe />);
        press(" ", { code: "Space" });

        unmount();
        expect(store().panning).toBe(false);

        press("e", { code: "KeyE" });
        expect(store().tool.mode).toBe("paint");
    });
});
