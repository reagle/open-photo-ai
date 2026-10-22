import { afterEach, describe, expect, it } from "vitest";
import {
    actionFor,
    ariaKeyShortcuts,
    DEFAULT_SHORTCUTS,
    isWindowShortcut,
    type KeyLike,
    matches,
    parseBinding,
} from "./shortcuts";

/** A key event with every modifier up unless named. */
const key = (value: string, modifiers: Partial<Omit<KeyLike, "key">> = {}): KeyLike => ({
    key: value,
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    shiftKey: false,
    ...modifiers,
});

const MAC = true;
const PC = false;

describe("parseBinding", () => {
    it("takes a binding apart into its modifiers and its key", () => {
        const parsed = parseBinding("Mod+Shift+=");

        expect(parsed?.key).toBe("=");
        expect([...(parsed?.modifiers ?? [])].sort()).toEqual(["Mod", "Shift"]);
    });

    it("reads a bare key, a named key, and `+` as a key", () => {
        expect(parseBinding("\\")?.key).toBe("\\");
        expect(parseBinding("Space")?.key).toBe("Space");
        expect(parseBinding("Mod++")?.key).toBe("=");
        expect(parseBinding("+")?.key).toBe("=");
    });

    it.each(["", "Mod+", "+=", "++", "Hyper+=", "Mod+Mod+=", "Shift", "Mod+space bar", "Mod+=+"])(
        "refuses %j",
        (binding) => {
            expect(parseBinding(binding)).toBeUndefined();
        },
    );

    it("parses every default binding", () => {
        for (const bindings of Object.values(DEFAULT_SHORTCUTS)) {
            for (const binding of bindings) expect(parseBinding(binding)).toBeDefined();
        }
    });
});

describe("matches", () => {
    it("zooms in with Command on macOS and Control elsewhere", () => {
        expect(actionFor(key("=", { metaKey: true }), DEFAULT_SHORTCUTS, MAC)).toBe("zoomIn");
        expect(actionFor(key("=", { ctrlKey: true }), DEFAULT_SHORTCUTS, PC)).toBe("zoomIn");
    });

    it("zooms out and fits with the platform's modifier", () => {
        expect(actionFor(key("-", { metaKey: true }), DEFAULT_SHORTCUTS, MAC)).toBe("zoomOut");
        expect(actionFor(key("-", { ctrlKey: true }), DEFAULT_SHORTCUTS, PC)).toBe("zoomOut");
        expect(actionFor(key("0", { metaKey: true }), DEFAULT_SHORTCUTS, MAC)).toBe("fit");
        expect(actionFor(key("0", { ctrlKey: true }), DEFAULT_SHORTCUTS, PC)).toBe("fit");
    });

    it("shows the original on backslash, with no modifier, on both platforms, and not on Space", () => {
        for (const mac of [MAC, PC]) {
            expect(actionFor(key("\\"), DEFAULT_SHORTCUTS, mac)).toBe("showOriginal");
            // A focused trigger button reopens its menu on Space, so it is no default; a user may bind it.
            expect(actionFor(key(" "), DEFAULT_SHORTCUTS, mac)).toBeUndefined();
        }
    });

    it("counts Shift with `=`, and the keypad's `+` and `-`", () => {
        // Shift with `=` is `+` on a US keyboard.
        expect(matches(key("+", { metaKey: true, shiftKey: true }), "Mod+=", MAC)).toBe(true);
        expect(matches(key("+", { ctrlKey: true, shiftKey: true }), "Mod+=", PC)).toBe(true);
        // The keypad's: the character with no Shift.
        expect(matches(key("+", { ctrlKey: true }), "Mod+=", PC)).toBe(true);
        expect(matches(key("-", { ctrlKey: true }), "Mod+-", PC)).toBe(true);
        expect(matches(key("_", { metaKey: true, shiftKey: true }), "Mod+-", MAC)).toBe(true);
    });

    it("does not take the other platform's modifier for Mod", () => {
        expect(matches(key("=", { ctrlKey: true }), "Mod+=", MAC)).toBe(false);
        expect(matches(key("=", { metaKey: true }), "Mod+=", PC)).toBe(false);
        // Nor both at once.
        expect(matches(key("=", { metaKey: true, ctrlKey: true }), "Mod+=", MAC)).toBe(false);
    });

    it("refuses an extra Alt on a binding with Mod, and the bare key without Mod", () => {
        expect(matches(key("=", { metaKey: true, altKey: true }), "Mod+=", MAC)).toBe(false);
        expect(matches(key("0", { ctrlKey: true, altKey: true }), "Mod+0", PC)).toBe(false);
        expect(matches(key("="), "Mod+=", PC)).toBe(false);
    });

    it("refuses Space with any modifier, and Shift with a digit", () => {
        expect(matches(key(" ", { shiftKey: true }), "Space", MAC)).toBe(false);
        expect(matches(key(" ", { altKey: true }), "Space", PC)).toBe(false);
        expect(matches(key(" ", { metaKey: true }), "Space", MAC)).toBe(false);
        expect(matches(key("0", { metaKey: true, shiftKey: true }), "Mod+0", MAC)).toBe(false);
    });

    it("refuses backslash with Command or Control", () => {
        expect(matches(key("\\", { metaKey: true }), "\\", MAC)).toBe(false);
        expect(matches(key("\\", { ctrlKey: true }), "\\", PC)).toBe(false);
    });

    it("takes a backslash however the layout types it: Option on a German Mac, AltGr on Windows", () => {
        expect(matches(key("\\", { altKey: true, shiftKey: true }), "\\", MAC)).toBe(true);

        const altGraph = key("\\", {
            ctrlKey: true,
            altKey: true,
            getModifierState: (name: string) => name === "AltGraph",
        });
        expect(matches(altGraph, "\\", PC)).toBe(true);
    });

    it("compares letters without case", () => {
        expect(matches(key("A", { shiftKey: true }), "Shift+a", PC)).toBe(true);
        expect(matches(key("A", { shiftKey: true }), "a", PC)).toBe(false);
    });

    it("matches nothing against a malformed binding", () => {
        expect(matches(key("=", { metaKey: true }), "Mod+Mod+=", MAC)).toBe(false);
        expect(matches(key(""), "", MAC)).toBe(false);
    });

    it("finds no action for an unbound key", () => {
        expect(actionFor(key("z"), DEFAULT_SHORTCUTS, MAC)).toBeUndefined();
        expect(actionFor(key("1", { metaKey: true }), DEFAULT_SHORTCUTS, MAC)).toBeUndefined();
    });
});

describe("isWindowShortcut", () => {
    afterEach(() => {
        document.body.replaceChildren();
    });

    /** Dispatches a key press at `target` and answers what the guard made of it. */
    const pressAt = (target: EventTarget, init: KeyboardEventInit) => {
        let answer: boolean | undefined;
        const listener = (event: Event) => {
            answer = isWindowShortcut(event as KeyboardEvent);
        };

        window.addEventListener("keydown", listener);
        target.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init }));
        window.removeEventListener("keydown", listener);

        return answer;
    };

    const mount = (html: string) => {
        document.body.innerHTML = html;

        return document.body.firstElementChild as HTMLElement;
    };

    it("takes a key pressed on the page itself", () => {
        expect(pressAt(document.body, { key: " " })).toBe(true);
        expect(pressAt(document.body, { key: "=", metaKey: true })).toBe(true);
    });

    it.each([
        ["a text field", "<input type='text'>"],
        ["a number field", "<input type='number'>"],
        ["a field with no type", "<input>"],
        ["a text area", "<textarea></textarea>"],
        ["a select", "<select><option>one</option></select>"],
        ["an editable region", "<div contenteditable='true'><span>text</span></div>"],
    ])("leaves every key in %s to it", (_name, html) => {
        const field = mount(html);
        const target = field.firstElementChild instanceof HTMLSpanElement ? field.firstElementChild : field;

        expect(pressAt(target, { key: " " })).toBe(false);
        expect(pressAt(target, { key: "-", ctrlKey: true })).toBe(false);
        expect(pressAt(target, { key: "\\" })).toBe(false);
    });

    it.each([
        "<div role='dialog'></div>",
        "<div role='alertdialog'></div>",
        "<div role='menu'></div>",
        "<dialog open></dialog>",
    ])("leaves every key alone while %s is open", (html) => {
        mount(`${html}<button>behind</button>`);

        expect(pressAt(document.body, { key: "0", metaKey: true })).toBe(false);
        expect(pressAt(document.body, { key: "\\" })).toBe(false);
    });

    it.each([
        ["a button", "<button>Zoom in</button>"],
        ["a native checkbox", "<input type='checkbox'>"],
        ["a native range", "<input type='range'>"],
        ["a Radix checkbox", "<button role='checkbox'></button>"],
        ["a slider", "<span role='slider' tabindex='0'></span>"],
        ["a tab", "<div role='tab' tabindex='0'></div>"],
        ["a toggle group's radio", "<button role='radio'></button>"],
        ["a switch", "<button role='switch'></button>"],
        ["a menu item", "<div role='menuitem' tabindex='0'></div>"],
    ])("leaves Space on %s to it, and takes backslash and Mod keys there", (_name, html) => {
        const control = mount(html);

        expect(pressAt(control, { key: " " })).toBe(false);
        expect(pressAt(control, { key: "\\" })).toBe(true);
        expect(pressAt(control, { key: "=", ctrlKey: true })).toBe(true);
    });

    it("leaves a key another control already handled", () => {
        const divider = mount("<div role='slider' tabindex='0'></div>");
        divider.addEventListener("keydown", (event) => event.preventDefault());

        expect(pressAt(divider, { key: "\\" })).toBe(false);
    });

    it("leaves a key pressed while an input method is composing", () => {
        expect(pressAt(document.body, { key: " ", isComposing: true })).toBe(false);
    });
});

describe("ariaKeyShortcuts", () => {
    it("names Command as Meta on macOS and Control elsewhere", () => {
        expect(ariaKeyShortcuts(DEFAULT_SHORTCUTS.zoomIn, MAC)).toBe("Meta+=");
        expect(ariaKeyShortcuts(DEFAULT_SHORTCUTS.zoomIn, PC)).toBe("Control+=");
        expect(ariaKeyShortcuts(DEFAULT_SHORTCUTS.zoomOut, MAC)).toBe("Meta+-");
        expect(ariaKeyShortcuts(DEFAULT_SHORTCUTS.zoomOut, PC)).toBe("Control+-");
    });

    it("separates alternatives with spaces and leaves out what is not a binding", () => {
        expect(ariaKeyShortcuts(["Space", "\\"], MAC)).toBe("Space \\");
        expect(ariaKeyShortcuts(["Mod+Shift+0", "Mod+Mod+0"], PC)).toBe("Control+Shift+0");
    });
});
