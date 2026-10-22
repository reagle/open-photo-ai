/**
 * The window's keyboard shortcuts: which actions have one, the keys they are bound to by default, and
 * the two questions every key press is asked - is it this binding, and is it the window's at all.
 *
 * Pure functions over a key event and the document, so every rule here is tested without mounting
 * anything. `hooks/useShortcuts.ts` is the one listener that applies them.
 */

/** Every action the keyboard can perform on the window. */
export const SHORTCUT_ACTIONS = ["zoomIn", "zoomOut", "fit", "showOriginal"] as const;

export type ShortcutAction = (typeof SHORTCUT_ACTIONS)[number];

// **Strings rather than objects**, because a string is what a later settings page persists and what a
// control recording a new binding produces - one representation from the first slice rather than a
// second one to convert to. The grammar is `parseBinding`'s.
//
// One binding per variant the user *names*, not per variant the keyboard produces: `Mod+=` already
// covers Shift with `=` and the keypad's `+`, through the equivalences in `canonicalKey`.
//
// Topaz Photo AI first, Lightroom Classic second: `Mod 0` fits as Topaz's does, `Mod =`/`Mod -` step as
// Lightroom's do, and `\` - Lightroom's Before key, held rather than toggled here - shows the original.
// Not `Space` as in Topaz: a Radix menu hands the focus back to its trigger button when it closes, the
// button takes `Space`, and so the press after picking an enhancement would reopen the menu and the
// next would add its first item. A user who wants `Space` can still bind it.
/** The bindings each action has until the user changes them. `Mod` is Command on macOS, Control elsewhere. */
export const DEFAULT_SHORTCUTS: Record<ShortcutAction, readonly string[]> = {
    zoomIn: ["Mod+="],
    zoomOut: ["Mod+-"],
    fit: ["Mod+0"],
    showOriginal: ["\\"],
};

/** The modifiers a binding may name, before the key. */
const MODIFIERS = ["Mod", "Ctrl", "Meta", "Alt", "Shift"] as const;

type Modifier = (typeof MODIFIERS)[number];

/** A binding taken apart: which modifiers it names, and the key in the form `canonicalKey` answers. */
export type ParsedBinding = {
    modifiers: ReadonlySet<Modifier>;
    key: string;
};

const isModifier = (value: string): value is Modifier => (MODIFIERS as readonly string[]).includes(value);

/** A named key, as `KeyboardEvent.key` spells one: `Space`, `Escape`, `ArrowUp`, `F2`. */
const NAMED_KEY = /^[A-Z][A-Za-z0-9]+$/;

/**
 * The key a binding or an event names, in the one form the two are compared in.
 *
 * `=` and `+` are one key, as are `-` and `_`: which of each pair a key produces depends on the layout
 * and on Shift, and the numeric keypad produces `+` and `-` - so the pair is what the user means.
 * Letters compare without case, so Shift with `a` is still `a`.
 */
const canonicalKey = (key: string) => {
    if (key === " ") return "Space";
    if (key === "+") return "=";
    if (key === "_") return "-";

    return key.length === 1 ? key.toLowerCase() : key;
};

/**
 * Whether a key is a punctuation character, whose Shift (and on some layouts whose Option or AltGr)
 * is part of *typing* it rather than a modifier on top of it: `+` needs Shift on a US keyboard and
 * not on a German one, and `\` needs Option on a German Mac.
 */
const isSymbol = (key: string) => key.length === 1 && !/[\p{L}\p{N}\s]/u.test(key);

/**
 * Takes a binding apart, or answers `undefined` for one that is not a binding.
 *
 * The grammar: modifiers from `Mod`, `Ctrl`, `Meta`, `Alt` and `Shift`, each at most once, joined by
 * `+`, then one key - a single character, or a named key such as `Space`. `+` is a key too, so
 * `Mod++` is Mod with `+`.
 */
export const parseBinding = (binding: string): ParsedBinding | undefined => {
    let rest: string;
    let key: string;

    if (binding === "+") {
        rest = "";
        key = "+";
    } else if (binding.endsWith("++")) {
        rest = binding.slice(0, -2);
        key = "+";
        // `++` alone is a modifier with no name, not a key.
        if (rest === "") return undefined;
    } else {
        const at = binding.lastIndexOf("+");
        rest = at < 0 ? "" : binding.slice(0, at);
        key = binding.slice(at + 1);
        if (at === 0) return undefined;
    }

    if (key === "" || isModifier(key) || (key.length > 1 && !NAMED_KEY.test(key))) return undefined;

    const modifiers = new Set<Modifier>();
    for (const name of rest === "" ? [] : rest.split("+")) {
        if (!isModifier(name) || modifiers.has(name)) return undefined;

        modifiers.add(name);
    }

    return { modifiers, key: canonicalKey(key) };
};

/** The parts of a key event a binding is matched against. */
export type KeyLike = Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey"> & {
    getModifierState?: (key: string) => boolean;
};

/**
 * Whether a key event is the given binding, on macOS (`mac`) or elsewhere.
 *
 * Compared on `event.key`, the character, rather than `event.code`, the physical key: a binding means
 * what is printed on the key, which is what a user of a non-US layout expects from `Mod -`.
 *
 * Every modifier the binding does not name must be up, with two exceptions for a punctuation key,
 * where the character already says how it was typed: Shift is ignored (so `Mod+=` matches Shift with
 * `=`, which is `+`), and on a binding with no Control or Command, so is Alt (a German Mac types `\`
 * with Option). AltGr, which Windows reports as Control with Alt, is never a modifier. A binding that
 * names `Mod` does not match while the *other* platform's modifier is held instead: Control with `=`
 * is not a zoom on macOS.
 */
export const matches = (event: KeyLike, binding: string, mac: boolean): boolean => {
    const parsed = parseBinding(binding);
    if (!parsed || canonicalKey(event.key) !== parsed.key) return false;

    const { modifiers } = parsed;
    const altGraph = event.getModifierState?.("AltGraph") === true;
    const ctrl = event.ctrlKey && !altGraph;
    const alt = event.altKey && !altGraph;

    const wantMeta = modifiers.has("Meta") || (mac && modifiers.has("Mod"));
    const wantCtrl = modifiers.has("Ctrl") || (!mac && modifiers.has("Mod"));
    const symbol = isSymbol(event.key);

    if (event.metaKey !== wantMeta || ctrl !== wantCtrl) return false;
    if (!(symbol && !modifiers.has("Shift")) && event.shiftKey !== modifiers.has("Shift")) return false;

    const altIsTyping = symbol && !wantMeta && !wantCtrl && !modifiers.has("Alt");

    return altIsTyping || alt === modifiers.has("Alt");
};

/** The first action one of whose bindings the event is, if any. */
export const actionFor = (
    event: KeyLike,
    bindings: Record<ShortcutAction, readonly string[]>,
    mac: boolean,
): ShortcutAction | undefined =>
    SHORTCUT_ACTIONS.find((action) => bindings[action].some((binding) => matches(event, binding, mac)));

/** Where typing goes: a key pressed in one of these is the field's, never the window's. */
const EDITABLE = [
    "input:not([type='checkbox'], [type='radio'], [type='range'])",
    "textarea",
    "select",
    "[contenteditable]:not([contenteditable='false'])",
].join(",");

/** What is open over the window: while one is, every key is that surface's. */
const OVERLAY = "[role='dialog'], [role='alertdialog'], [role='menu'], dialog[open]";

// `Space` presses, ticks, drags and selects these, and taking it from them would break operating the
// whole window from the keyboard. `radio` because the drawer's comparison toggles are Radix radios,
// and `switch` because Autopilot is one.
/** Where `Space` operates the control that has the focus. */
const SPACE_OPERATES = [
    "button",
    "input",
    "summary",
    "[role='button']",
    "[role='checkbox']",
    "[role='radio']",
    "[role='switch']",
    "[role='slider']",
    "[role='tab']",
    "[role='option']",
    "[role='menuitem']",
    "[role='menuitemcheckbox']",
    "[role='menuitemradio']",
].join(",");

/**
 * Whether a key press belongs to the window rather than to whatever has the focus.
 *
 * Not when it lands in a field the user types into or an editable region; not while a dialog or a
 * menu is open; not when the key is `Space` and the focus is on a control `Space` operates; not while
 * an input method is composing; and not once a control has already handled it.
 */
export const isWindowShortcut = (event: KeyboardEvent): boolean => {
    if (event.defaultPrevented || event.isComposing) return false;

    const target = event.target instanceof Element ? event.target : undefined;
    if (target?.closest(EDITABLE) || (target instanceof HTMLElement && target.isContentEditable)) return false;

    const document = target?.ownerDocument ?? globalThis.document;
    if (document.querySelector(OVERLAY)) return false;

    return !(event.key === " " && target?.closest(SPACE_OPERATES));
};

/** How `aria-keyshortcuts` spells each modifier, on this platform. */
const ARIA_MODIFIERS: Record<Modifier, (mac: boolean) => string> = {
    Mod: (mac) => (mac ? "Meta" : "Control"),
    Ctrl: () => "Control",
    Meta: () => "Meta",
    Alt: () => "Alt",
    Shift: () => "Shift",
};

/**
 * Bindings in the form `aria-keyshortcuts` takes: modifiers named as the platform's (`Meta+=` on
 * macOS, `Control+=` elsewhere), alternatives separated by spaces. Malformed bindings are left out.
 */
export const ariaKeyShortcuts = (bindings: readonly string[], mac: boolean): string =>
    bindings
        .map(parseBinding)
        .filter((parsed) => parsed !== undefined)
        .map(({ modifiers, key }) =>
            [...MODIFIERS.filter((name) => modifiers.has(name)).map((name) => ARIA_MODIFIERS[name](mac)), key].join(
                "+",
            ),
        )
        .join(" ");
