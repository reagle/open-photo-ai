import { describe, expect, it } from "vitest";
import { returnAccepts } from "./layers";

/** A Return pressed at `target`, with whatever else a case is about. */
const enter = (target: Element, init: Partial<KeyboardEventInit> & { defaultPrevented?: boolean } = {}) => {
    const { defaultPrevented = false, ...rest } = init;
    const event = new KeyboardEvent("keydown", { key: "Enter", cancelable: true, ...rest });
    Object.defineProperty(event, "target", { value: target });
    if (defaultPrevented) event.preventDefault();

    return event;
};

const element = (html: string) => {
    const host = document.createElement("div");
    host.innerHTML = html;

    return host.firstElementChild as Element;
};

describe("returnAccepts", () => {
    it("is a plain Return on the surface itself, a slider or a single-line field", () => {
        expect(returnAccepts(enter(document.body))).toBe(true);
        expect(returnAccepts(enter(element("<span role='slider'></span>")))).toBe(true);
        expect(returnAccepts(enter(element("<input type='text'>")))).toBe(true);
    });

    it("is not a Return the focused control operates itself", () => {
        for (const html of [
            "<button>Cancel</button>",
            "<a href='#'>Link</a>",
            "<textarea></textarea>",
            "<select></select>",
            "<button role='switch'></button>",
            "<div role='combobox'></div>",
            "<div role='option'></div>",
            "<div role='menuitem'></div>",
            "<div contenteditable='true'></div>",
        ]) {
            expect(returnAccepts(enter(element(html))), html).toBe(false);
        }
    });

    it("is not a Return with a modifier, one an input method is composing, or one already handled", () => {
        expect(returnAccepts(enter(document.body, { shiftKey: true }))).toBe(false);
        expect(returnAccepts(enter(document.body, { metaKey: true }))).toBe(false);
        expect(returnAccepts(enter(document.body, { isComposing: true }))).toBe(false);
        expect(returnAccepts(enter(document.body, { defaultPrevented: true }))).toBe(false);
        expect(returnAccepts(enter(document.body, { key: "a" }))).toBe(false);
    });
});
