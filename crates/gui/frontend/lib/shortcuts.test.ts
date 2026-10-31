import { afterEach, describe, expect, it } from "vitest";
import {
    APP_ACTIONS,
    actionFor,
    appActionFor,
    appShortcuts,
    ariaKeyShortcuts,
    bindingFromEvent,
    DEFAULT_SHORTCUTS,
    formatBinding,
    isAppAction,
    isEditing,
    isOverlayOpen,
    isWindowShortcut,
    type KeyLike,
    MAX_BINDINGS,
    matches,
    overlaps,
    parseBinding,
    refusal,
    repairShortcuts,
    resolveShortcuts,
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
        expect(actionFor(key("k"), DEFAULT_SHORTCUTS, MAC)).toBeUndefined();
        expect(actionFor(key("2", { metaKey: true }), DEFAULT_SHORTCUTS, MAC)).toBeUndefined();
    });
});

describe("the defaults added for discovery", () => {
    it.each([
        ["macOS", MAC, { metaKey: true }],
        ["Windows and Linux", PC, { ctrlKey: true }],
    ])("bind actual pixels, the toggle, previous, next and the list on %s", (_platform, mac, mod) => {
        expect(actionFor(key("1", mod), DEFAULT_SHORTCUTS, mac)).toBe("actualPixels");
        expect(actionFor(key("z"), DEFAULT_SHORTCUTS, mac)).toBe("toggleZoom");
        expect(actionFor(key("ArrowLeft"), DEFAULT_SHORTCUTS, mac)).toBe("previousImage");
        expect(actionFor(key("ArrowRight"), DEFAULT_SHORTCUTS, mac)).toBe("nextImage");
        // Typed with Shift on most layouts, which a symbol ignores.
        expect(actionFor(key("?", { shiftKey: true }), DEFAULT_SHORTCUTS, mac)).toBe("showShortcuts");
        // Topaz's key for its shortcuts panel, beside `?`.
        expect(actionFor(key("/", mod), DEFAULT_SHORTCUTS, mac)).toBe("showShortcuts");
    });

    it("leave a bare slash, and the other platform's modifier with it, alone", () => {
        expect(actionFor(key("/"), DEFAULT_SHORTCUTS, MAC)).toBeUndefined();
        expect(actionFor(key("/", { ctrlKey: true }), DEFAULT_SHORTCUTS, MAC)).toBeUndefined();
        expect(actionFor(key("/", { metaKey: true }), DEFAULT_SHORTCUTS, PC)).toBeUndefined();
    });

    it("leave an arrow with Shift or Alt, and Z with Shift, alone", () => {
        // Command with an arrow pans since the editor's keys - see below.
        expect(actionFor(key("ArrowRight", { shiftKey: true }), DEFAULT_SHORTCUTS, MAC)).toBeUndefined();
        expect(actionFor(key("ArrowRight", { altKey: true }), DEFAULT_SHORTCUTS, PC)).toBeUndefined();
        expect(actionFor(key("Z", { shiftKey: true }), DEFAULT_SHORTCUTS, PC)).toBeUndefined();
    });
});

describe("the editor's defaults", () => {
    it("bind the bare letters, Delete and Backspace, and the digits on both platforms", () => {
        for (const mac of [MAC, PC]) {
            expect(actionFor(key("a"), DEFAULT_SHORTCUTS, mac)).toBe("toggleAutopilot");
            expect(actionFor(key("e"), DEFAULT_SHORTCUTS, mac)).toBe("addEnhancement");
            expect(actionFor(key("c"), DEFAULT_SHORTCUTS, mac)).toBe("cropRotate");
            expect(actionFor(key("v"), DEFAULT_SHORTCUTS, mac)).toBe("cyclePreview");
            expect(actionFor(key("i"), DEFAULT_SHORTCUTS, mac)).toBe("toggleInfo");
            expect(actionFor(key("Delete"), DEFAULT_SHORTCUTS, mac)).toBe("removeEnhancement");
            expect(actionFor(key("Backspace"), DEFAULT_SHORTCUTS, mac)).toBe("removeEnhancement");

            for (const place of [1, 2, 3, 4, 5, 6, 7, 8]) {
                expect(actionFor(key(String(place)), DEFAULT_SHORTCUTS, mac)).toBe(`openOptions${place}`);
            }
        }
    });

    it.each([
        ["macOS", MAC, { metaKey: true }],
        ["Windows and Linux", PC, { ctrlKey: true }],
    ])("bind the drawer, reveal and the pans with the platform's modifier on %s", (_platform, mac, mod) => {
        expect(actionFor(key("\\", mod), DEFAULT_SHORTCUTS, mac)).toBe("toggleDrawer");
        expect(actionFor(key("R", { ...mod, shiftKey: true }), DEFAULT_SHORTCUTS, mac)).toBe("revealImage");
        expect(actionFor(key("ArrowLeft", mod), DEFAULT_SHORTCUTS, mac)).toBe("panLeft");
        expect(actionFor(key("ArrowRight", mod), DEFAULT_SHORTCUTS, mac)).toBe("panRight");
        expect(actionFor(key("ArrowUp", mod), DEFAULT_SHORTCUTS, mac)).toBe("panUp");
        expect(actionFor(key("ArrowDown", mod), DEFAULT_SHORTCUTS, mac)).toBe("panDown");
    });

    it("keep a digit with Mod for actual pixels, and backslash without it for the original", () => {
        expect(actionFor(key("1", { metaKey: true }), DEFAULT_SHORTCUTS, MAC)).toBe("actualPixels");
        expect(actionFor(key("1", { ctrlKey: true }), DEFAULT_SHORTCUTS, PC)).toBe("actualPixels");
        expect(actionFor(key("\\"), DEFAULT_SHORTCUTS, MAC)).toBe("showOriginal");
    });

    it("leave a letter with Mod to the menus and the system: Command-C, -V and -A copy, paste and select", () => {
        for (const letter of ["a", "c", "e", "v"]) {
            expect(actionFor(key(letter, { metaKey: true }), DEFAULT_SHORTCUTS, MAC)).toBeUndefined();
            expect(actionFor(key(letter, { ctrlKey: true }), DEFAULT_SHORTCUTS, PC)).toBeUndefined();
        }
        // Mod with R alone is a reload in a browser; only Shift with it reveals.
        expect(actionFor(key("r", { metaKey: true }), DEFAULT_SHORTCUTS, MAC)).toBeUndefined();
    });

    it("leave a digit typed with Shift alone, and Shift with a letter", () => {
        expect(actionFor(key("!", { shiftKey: true }), DEFAULT_SHORTCUTS, PC)).toBeUndefined();
        expect(actionFor(key("1", { shiftKey: true }), DEFAULT_SHORTCUTS, PC)).toBeUndefined();
        expect(actionFor(key("A", { shiftKey: true }), DEFAULT_SHORTCUTS, MAC)).toBeUndefined();
    });

    it("claim no key press twice, and none the system or the application holds, on either platform", () => {
        for (const mac of [MAC, PC]) {
            const all = Object.values(DEFAULT_SHORTCUTS).flat();
            for (const binding of all) expect(refusal(binding, mac)).toBeUndefined();

            for (const [index, binding] of all.entries()) {
                for (const other of all.slice(index + 1)) expect(overlaps(binding, other, mac)).toBe(false);
            }

            // So resolving the defaults drops none of them.
            expect(resolveShortcuts({}, mac)).toEqual(DEFAULT_SHORTCUTS);
        }
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
        ["a slider", "<span role='slider' tabindex='0'></span>"],
        ["a radio in a group", "<div role='radiogroup'><button role='radio'>one</button></div>"],
        ["the split's divider", "<div role='separator' tabindex='0'></div>"],
        ["a tab", "<button role='tab'>one</button>"],
        ["a marked in-house control", "<div data-arrow-keys tabindex='0'></div>"],
    ])("leaves the arrow keys, Home and End to %s", (_name, html) => {
        const root = mount(html);
        const target = root.firstElementChild instanceof HTMLElement ? root.firstElementChild : root;

        for (const value of ["ArrowLeft", "ArrowRight", "ArrowUp", "Home", "End", "PageDown"]) {
            expect(pressAt(target, { key: value })).toBe(false);
        }
        // Other keys are still the window's from there.
        expect(pressAt(target, { key: "1", metaKey: true })).toBe(true);
    });

    it("takes the arrow keys from a plain button", () => {
        const button = mount("<button>Zoom in</button>");

        expect(pressAt(button, { key: "ArrowRight" })).toBe(true);
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

describe("refusal", () => {
    it.each(["Escape", "Shift+Escape", "Mod+Escape", "Tab", "Shift+Tab", "Alt+Tab", "Enter", "Shift+Enter"])(
        "keeps %s for moving around the window on both platforms",
        (binding) => {
            expect(refusal(binding, MAC)).toBe("navigation");
            expect(refusal(binding, PC)).toBe("navigation");
        },
    );

    it("lets Enter with Command or Control be bound", () => {
        expect(refusal("Mod+Enter", MAC)).toBeUndefined();
        expect(refusal("Alt+Enter", PC)).toBeUndefined();
    });

    it.each(["Mod+h", "Mod+m", "Meta+q"])("refuses %s on macOS only", (binding) => {
        expect(refusal(binding, MAC)).toBe("system");
        expect(refusal(binding, PC)).toBeUndefined();
    });

    // macOS's own keys stay the system's there; elsewhere the application's actions hold them.
    it.each(["Mod+q", "Mod+w", "Mod+Shift+w"])(
        "refuses %s as the system's on macOS, the application's elsewhere",
        (binding) => {
            expect(refusal(binding, MAC)).toBe("system");
            expect(refusal(binding, PC)).toBe("application");
        },
    );

    it.each(["Mod+o", "Mod+,", "Mod+Shift+,", "Mod+s", "Mod+Shift+e"])(
        "refuses %s as the application's everywhere",
        (binding) => {
            expect(refusal(binding, MAC)).toBe("application");
            expect(refusal(binding, PC)).toBe("application");
        },
    );

    it("refuses Command-O on macOS, which is Mod there, and leaves Control-O alone", () => {
        expect(refusal("Meta+o", MAC)).toBe("application");
        expect(refusal("Ctrl+o", MAC)).toBeUndefined();
        expect(refusal("Ctrl+o", PC)).toBe("application");
        expect(refusal("Meta+o", PC)).toBeUndefined();
    });

    it("leaves the application's keys with another modifier, or none, alone", () => {
        for (const binding of ["o", "w", "Mod+Alt+o", "Mod+Shift+o", "Ctrl+Alt+w", "Shift+w"]) {
            expect(refusal(binding, MAC)).toBeUndefined();
            expect(refusal(binding, PC)).toBeUndefined();
        }
    });

    it("refuses every key the application's actions hold, on every platform", () => {
        for (const platform of ["macos", "windows", "linux"] as const) {
            for (const binding of Object.values(appShortcuts(platform)).flat()) {
                expect(refusal(binding, platform === "macos")).toBeDefined();
            }
        }
    });

    it("refuses Alt+F4 off macOS only", () => {
        expect(refusal("Alt+F4", PC)).toBe("system");
        expect(refusal("Alt+F4", MAC)).toBeUndefined();
    });

    it("allows every default, bare letters and digits", () => {
        for (const binding of [...Object.values(DEFAULT_SHORTCUTS).flat(), "z", "1", "Shift+z"]) {
            expect(refusal(binding, MAC)).toBeUndefined();
            expect(refusal(binding, PC)).toBeUndefined();
        }
    });
});

describe("overlaps", () => {
    it("treats Mod and Command as one on macOS, and Mod and Control as one elsewhere", () => {
        expect(overlaps("Mod+=", "Meta+=", MAC)).toBe(true);
        expect(overlaps("Mod+=", "Meta+=", PC)).toBe(false);
        expect(overlaps("Mod+=", "Ctrl+=", PC)).toBe(true);
    });

    it("treats a punctuation key with and without Shift as one, and = with +", () => {
        expect(overlaps("?", "Shift+?", MAC)).toBe(true);
        expect(overlaps("Mod+=", "Mod++", PC)).toBe(true);
    });

    it("treats a bare symbol with and without Option as one", () => {
        expect(overlaps("\\", "Alt+\\", MAC)).toBe(true);
    });

    it("tells apart different keys, a letter with and without Shift, and extra modifiers", () => {
        expect(overlaps("Mod+=", "Mod+-", MAC)).toBe(false);
        expect(overlaps("z", "Shift+z", MAC)).toBe(false);
        expect(overlaps("Mod+0", "Mod+Alt+0", PC)).toBe(false);
        expect(overlaps("Space", "Space", PC)).toBe(true);
        expect(overlaps("nonsense+", "Space", PC)).toBe(false);
    });
});

describe("bindingFromEvent", () => {
    it("records Command on macOS and Control elsewhere as Mod", () => {
        expect(bindingFromEvent(key("=", { metaKey: true }), MAC)).toBe("Mod+=");
        expect(bindingFromEvent(key("=", { ctrlKey: true }), PC)).toBe("Mod+=");
    });

    it("writes the other platform's modifier out", () => {
        expect(bindingFromEvent(key("k", { ctrlKey: true }), MAC)).toBe("Ctrl+k");
        expect(bindingFromEvent(key("k", { metaKey: true }), PC)).toBe("Meta+k");
    });

    it("keeps Shift for a letter, lower-cased, and leaves it out for punctuation", () => {
        expect(bindingFromEvent(key("Z", { shiftKey: true }), MAC)).toBe("Shift+z");
        expect(bindingFromEvent(key("?", { shiftKey: true }), PC)).toBe("?");
        expect(bindingFromEvent(key("+", { shiftKey: true, metaKey: true }), MAC)).toBe("Mod++");
    });

    it("never records AltGr, nor Option used to type a bare symbol", () => {
        const altGr = key("\\", { ctrlKey: true, altKey: true, getModifierState: (name) => name === "AltGraph" });
        expect(bindingFromEvent(altGr, PC)).toBe("\\");
        expect(bindingFromEvent(key("\\", { altKey: true }), MAC)).toBe("\\");
        expect(bindingFromEvent(key("0", { altKey: true, ctrlKey: true }), PC)).toBe("Mod+Alt+0");
    });

    it("names Space and other named keys", () => {
        expect(bindingFromEvent(key(" "), MAC)).toBe("Space");
        expect(bindingFromEvent(key("F2", { shiftKey: true }), PC)).toBe("Shift+F2");
    });

    it.each(["Shift", "Control", "Alt", "Meta", "AltGraph", "Dead", "Unidentified", "Process"])(
        "records nothing for %s",
        (value) => {
            expect(bindingFromEvent(key(value), MAC)).toBeUndefined();
        },
    );

    it("records what it then matches", () => {
        for (const [event, mac] of [
            [key("=", { metaKey: true }), MAC],
            [key("Z", { shiftKey: true, ctrlKey: true }), PC],
            [key("?", { shiftKey: true }), MAC],
        ] as const) {
            const binding = bindingFromEvent(event, mac);
            expect(binding && matches(event, binding, mac)).toBe(true);
        }
    });
});

describe("formatBinding", () => {
    it("prints modifier glyphs on macOS", () => {
        expect(formatBinding("Mod+=", MAC, "Space")).toBe("⌘ =");
        expect(formatBinding("Mod+Shift+Alt+z", MAC, "Space")).toBe("⌥⇧⌘ Z");
        expect(formatBinding("Ctrl+ArrowLeft", MAC, "Space")).toBe("⌃ ←");
        expect(formatBinding("\\", MAC, "Space")).toBe("\\");
    });

    it("prints modifier names elsewhere, and + as written", () => {
        expect(formatBinding("Mod+=", PC, "Space")).toBe("Ctrl+=");
        expect(formatBinding("Mod++", PC, "Space")).toBe("Ctrl++");
        expect(formatBinding("Alt+Shift+z", PC, "Space")).toBe("Alt+Shift+Z");
    });

    it("prints the deleting keys as macOS's menus do, and by name elsewhere", () => {
        expect(formatBinding("Backspace", MAC, "Space")).toBe("⌫");
        expect(formatBinding("Delete", MAC, "Space")).toBe("⌦");
        expect(formatBinding("Backspace", PC, "Space")).toBe("Backspace");
        expect(formatBinding("Delete", PC, "Space")).toBe("Delete");
        expect(formatBinding("Mod+\\", MAC, "Space")).toBe("⌘ \\");
        expect(formatBinding("Mod+Shift+r", PC, "Space")).toBe("Ctrl+Shift+R");
    });

    it("names Space in the user's language, and shows junk as it is", () => {
        expect(formatBinding("Space", PC, "Leertaste")).toBe("Leertaste");
        expect(formatBinding("Mod+", PC, "Space")).toBe("Mod+");
    });
});

describe("repairShortcuts", () => {
    it.each([null, 3, "Mod+=", ["Mod+="]])("reads %j as no choices", (value) => {
        expect(repairShortcuts(value)).toEqual({});
    });

    it("drops unknown actions and values that are not lists of strings", () => {
        expect(repairShortcuts({ rotate: ["r"], zoomIn: "Mod+=", zoomOut: [1], fit: ["Mod+9"] })).toEqual({
            fit: ["Mod+9"],
        });
    });

    it("drops malformed, everywhere-refused and repeated bindings, and keeps at most four", () => {
        expect(repairShortcuts({ fit: ["Mod+", "Escape", "Mod+9", "Mod+9", "Mod+h", "1", "2", "3"] })).toEqual({
            // `Mod+h` is refused on macOS only, so it is kept here and dropped where it is resolved.
            fit: ["Mod+9", "Mod+h", "1", "2"],
        });
        expect(MAX_BINDINGS).toBe(4);
    });

    it("drops a stored application key, which is refused on every platform", () => {
        expect(repairShortcuts({ fit: ["Mod+o", "Mod+9"], zoomIn: ["Mod+,", "Mod+Shift+w", "Mod+q"] })).toEqual({
            fit: ["Mod+9"],
        });
    });

    it("keeps an empty list as a choice, and reads a list emptied by repair as the defaults", () => {
        expect(repairShortcuts({ showOriginal: [], fit: ["Escape"] })).toEqual({ showOriginal: [] });
    });
});

describe("resolveShortcuts", () => {
    it("is the defaults when nothing is stored", () => {
        expect(resolveShortcuts({}, MAC)).toEqual(DEFAULT_SHORTCUTS);
    });

    it("lets a stored binding beat another action's default", () => {
        const table = resolveShortcuts({ zoomIn: ["Mod+0"] }, PC);

        expect(table.zoomIn).toEqual(["Mod+0"]);
        expect(table.fit).toEqual([]);
        expect(actionFor(key("0", { ctrlKey: true }), table, PC)).toBe("zoomIn");
    });

    it("gives a press claimed by two stored actions to the earlier one", () => {
        const table = resolveShortcuts({ zoomIn: ["z"], fit: ["Shift+z", "z"] }, MAC);

        expect(table.zoomIn).toEqual(["z"]);
        expect(table.fit).toEqual(["Shift+z"]);
    });

    it("drops what this platform refuses, and honours an empty list", () => {
        const table = resolveShortcuts({ fit: ["Mod+h", "Mod+9"], showOriginal: [] }, MAC);

        expect(table.fit).toEqual(["Mod+9"]);
        expect(table.showOriginal).toEqual([]);
        expect(resolveShortcuts({ fit: ["Mod+h"] }, PC).fit).toEqual(["Mod+h"]);
    });
});

describe("the application's actions", () => {
    it("holds the same keys on every platform but Quit's", () => {
        expect(appShortcuts("macos")).toEqual({
            open: ["Mod+o"],
            export: ["Mod+s", "Mod+Shift+e"],
            settings: ["Mod+,"],
            closeImage: ["Mod+w"],
            closeAll: ["Mod+Shift+w"],
            quit: ["Mod+q"],
        });
        expect(appShortcuts("windows").quit).toEqual(["Alt+F4"]);
        expect(appShortcuts("linux").quit).toEqual(["Mod+q"]);
        expect(APP_ACTIONS).toEqual(["open", "export", "settings", "closeImage", "closeAll", "quit"]);
    });

    it("names an action by its id, and nothing else", () => {
        expect(isAppAction("closeImage")).toBe(true);
        expect(isAppAction("zoomIn")).toBe(false);
        expect(isAppAction(undefined)).toBe(false);
    });

    it.each([
        ["o", {}, "open"],
        [",", {}, "settings"],
        ["w", {}, "closeImage"],
        ["W", { shiftKey: true }, "closeAll"],
        ["s", {}, "export"],
        ["E", { shiftKey: true }, "export"],
    ] as const)("matches Ctrl with %s on Windows and Linux", (pressed, extra, action) => {
        for (const platform of ["windows", "linux"] as const) {
            expect(appActionFor(key(pressed, { ctrlKey: true, ...extra }), platform)).toBe(action);
        }
    });

    it("quits with Ctrl+Q on Linux only, and leaves Alt+F4 to Windows itself", () => {
        expect(appActionFor(key("q", { ctrlKey: true }), "linux")).toBe("quit");
        expect(appActionFor(key("q", { ctrlKey: true }), "windows")).toBeUndefined();
        expect(appActionFor(key("F4", { altKey: true }), "windows")).toBeUndefined();
    });

    it("matches on macOS only a key the menu does not hold: Export's second", () => {
        expect(appActionFor(key("w", { metaKey: true }), "macos")).toBeUndefined();
        expect(appActionFor(key("o", { metaKey: true }), "macos")).toBeUndefined();
        expect(appActionFor(key("s", { metaKey: true }), "macos")).toBeUndefined();
        expect(appActionFor(key("E", { metaKey: true, shiftKey: true }), "macos")).toBe("export");
        expect(appActionFor(key("E", { ctrlKey: true, shiftKey: true }), "macos")).toBeUndefined();
    });

    it("matches Export's keys only with Shift where Shift is named", () => {
        expect(appActionFor(key("e", { ctrlKey: true }), "linux")).toBeUndefined();
        expect(appActionFor(key("S", { ctrlKey: true, shiftKey: true }), "linux")).toBeUndefined();
    });

    it("matches no key without its modifier, or with another", () => {
        expect(appActionFor(key("w"), "linux")).toBeUndefined();
        expect(appActionFor(key("w", { metaKey: true }), "linux")).toBeUndefined();
        expect(appActionFor(key("w", { ctrlKey: true, altKey: true }), "windows")).toBeUndefined();
    });

    it("sees the focus in a text field, and not on a button", () => {
        const field = document.createElement("input");
        const button = document.createElement("button");
        document.body.append(field, button);

        field.focus();
        expect(isEditing(document)).toBe(true);
        button.focus();
        expect(isEditing(document)).toBe(false);
        field.remove();
        button.remove();
    });

    it("sees a dialog or a menu open over the window", () => {
        expect(isOverlayOpen(document)).toBe(false);

        const dialog = document.createElement("div");
        dialog.setAttribute("role", "dialog");
        document.body.append(dialog);
        expect(isOverlayOpen(document)).toBe(true);

        dialog.setAttribute("role", "menu");
        expect(isOverlayOpen(document)).toBe(true);
        dialog.remove();
    });
});
