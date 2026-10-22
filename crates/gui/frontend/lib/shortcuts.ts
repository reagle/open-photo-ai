/**
 * The window's keyboard shortcuts: which actions have one, the keys they are bound to by default, and
 * the two questions every key press is asked - is it this binding, and is it the window's at all.
 *
 * Pure functions over a key event and the document, so every rule here is tested without mounting
 * anything. `hooks/useShortcuts.ts` is the one listener that applies them.
 */

/** Every action the keyboard can perform on the window. */
// The order the Keyboard settings page and the shortcuts list draw them in, and the order in which two
// stored actions claiming one key press are resolved - see `resolveShortcuts`.
export const SHORTCUT_ACTIONS = [
    "zoomIn",
    "zoomOut",
    "fit",
    "actualPixels",
    "toggleZoom",
    "showOriginal",
    "previousImage",
    "nextImage",
    "showShortcuts",
] as const;

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
    // Topaz's 100%, and Lightroom's `Z` - held here to fitted and actual pixels, not to the last zoom.
    actualPixels: ["Mod+1"],
    toggleZoom: ["z"],
    showOriginal: ["\\"],
    // Lightroom's filmstrip keys.
    previousImage: ["ArrowLeft"],
    nextImage: ["ArrowRight"],
    // A symbol, so Shift is ignored: it matches on every layout that types a `?`.
    showShortcuts: ["?"],
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

// The same shape as the `Space` rule: a control operated by arrows keeps them, whether or not it
// remembers to prevent their default. `separator` is the split comparison's divider; `radio` and
// `radiogroup` the comparison toggles; `[data-arrow-keys]` the marker for an in-house control that
// moves by arrows and has none of these roles.
/** Where the arrow keys, Home, End and the page keys move something inside the focused control. */
const ARROWS_OPERATE = [
    "[role='slider']",
    "[role='radiogroup']",
    "[role='radio']",
    "[role='tablist']",
    "[role='tab']",
    "[role='separator']",
    "[role='listbox']",
    "[role='grid']",
    "[role='tree']",
    "[role='menu']",
    "[role='menubar']",
    "[role='toolbar']",
    "[role='spinbutton']",
    "[role='scrollbar']",
    "[data-arrow-keys]",
].join(",");

/** The keys {@link ARROWS_OPERATE} keeps. */
const ARROW_KEYS = new Set(["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End", "PageUp", "PageDown"]);

/**
 * Whether a key press belongs to the window rather than to whatever has the focus.
 *
 * Not when it lands in a field the user types into or an editable region; not while a dialog or a
 * menu is open; not when the key is `Space` and the focus is on a control `Space` operates, nor an
 * arrow key on a control arrows operate; not while
 * an input method is composing; and not once a control has already handled it.
 */
export const isWindowShortcut = (event: KeyboardEvent): boolean => {
    if (event.defaultPrevented || event.isComposing) return false;

    const target = event.target instanceof Element ? event.target : undefined;
    if (target?.closest(EDITABLE) || (target instanceof HTMLElement && target.isContentEditable)) return false;

    const document = target?.ownerDocument ?? globalThis.document;
    if (document.querySelector(OVERLAY)) return false;

    if (ARROW_KEYS.has(event.key) && target?.closest(ARROWS_OPERATE)) return false;

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

/** The bindings each action has, as the user stored them: only the actions they changed. */
export type StoredShortcuts = Partial<Record<ShortcutAction, string[]>>;

/** The bindings every action has in force: the user's where they chose, the defaults elsewhere. */
export type ShortcutTable = Record<ShortcutAction, readonly string[]>;

/** The most bindings one action keeps. Enough for every reference's alternatives, few enough to list. */
export const MAX_BINDINGS = 4;

/** Why a binding cannot be had: a key the window needs to be operated, or one the system takes. */
export type Refusal = "navigation" | "system";

const isShortcutAction = (value: string): value is ShortcutAction =>
    (SHORTCUT_ACTIONS as readonly string[]).includes(value);

/** macOS's own window keys with Command: quit, close, hide and minimise. */
const MAC_SYSTEM_KEYS = new Set(["q", "w", "h", "m"]);

/**
 * Why a binding is refused on this platform, or `undefined` when it may be bound.
 *
 * `navigation` everywhere: `Escape` and `Tab` with any modifiers, and `Enter` with nothing but Shift -
 * taking them would leave a keyboard user unable to close a dialog, move the focus or press a button.
 * `system`: on macOS, Command with `Q`, `W`, `H` or `M`, which quit, close, hide or minimise the
 * window before the page sees them; elsewhere, `Alt` with `F4`. A string that is not a binding is
 * not refused here - `parseBinding` is the question for that.
 */
export const refusal = (binding: string, mac: boolean): Refusal | undefined => {
    const parsed = parseBinding(binding);
    if (!parsed) return undefined;

    const { modifiers, key } = parsed;
    if (key === "Escape" || key === "Tab") return "navigation";
    if (key === "Enter" && [...modifiers].every((name) => name === "Shift")) return "navigation";

    const command = modifiers.has("Meta") || (mac && modifiers.has("Mod"));
    if (mac && command && MAC_SYSTEM_KEYS.has(key)) return "system";
    if (!mac && key === "F4" && modifiers.has("Alt")) return "system";

    return undefined;
};

/** Every combination of the four modifiers, as the key event fields that report them. */
const MODIFIER_STATES: Omit<KeyLike, "key">[] = Array.from({ length: 16 }, (_, bits) => ({
    ctrlKey: (bits & 1) !== 0,
    metaKey: (bits & 2) !== 0,
    altKey: (bits & 4) !== 0,
    shiftKey: (bits & 8) !== 0,
}));

/** The `KeyboardEvent.key` a parsed key arrives as. `canonicalKey` maps it back to itself. */
const eventKey = (key: string) => (key === "Space" ? " " : key);

/**
 * Whether one key press would match both bindings on this platform - which is what "the same binding"
 * means, so `Mod+=` and `Meta+=` are one on macOS, and `?` and `Shift+?` are one everywhere.
 *
 * Asked of `matches` itself, over every combination of the modifiers, rather than of a normalised
 * form: a second statement of the matching rules is a second thing to keep in step with the first.
 */
export const overlaps = (a: string, b: string, mac: boolean): boolean => {
    const first = parseBinding(a);
    const second = parseBinding(b);
    if (!first || !second || first.key !== second.key) return false;

    const key = eventKey(first.key);

    return MODIFIER_STATES.some((state) => matches({ key, ...state }, a, mac) && matches({ key, ...state }, b, mac));
};

/** Keys a recorder never offers: the modifiers on their own, and what a dead key or an input method sends. */
const UNRECORDABLE = new Set([
    "Control",
    "Shift",
    "Alt",
    "AltGraph",
    "Meta",
    "OS",
    "Hyper",
    "Super",
    "Fn",
    "FnLock",
    "CapsLock",
    "NumLock",
    "ScrollLock",
    "Dead",
    "Unidentified",
    "Process",
]);

/**
 * The binding a key press would be recorded as, or `undefined` for one that names no key yet - a
 * modifier on its own, a dead key, an input method at work.
 *
 * Written in `parseBinding`'s grammar, so it is matched by the same rules it will be judged by:
 * Command on macOS and Control elsewhere become `Mod` and the other platform's modifier is written out
 * as `Ctrl` or `Meta`; AltGr is never a modifier; Shift is left out for a punctuation key (it is part
 * of typing `?`), and so is Alt on one with no Command or Control (a German Mac types `\` with
 * Option); a letter is written lower-case, and `" "` as `Space`.
 */
export const bindingFromEvent = (event: KeyLike, mac: boolean): string | undefined => {
    if (UNRECORDABLE.has(event.key)) return undefined;

    const key = event.key === " " ? "Space" : event.key.length === 1 ? event.key.toLowerCase() : event.key;
    const altGraph = event.getModifierState?.("AltGraph") === true;
    const ctrl = event.ctrlKey && !altGraph;
    const symbol = isSymbol(event.key);

    const modifiers: Modifier[] = [];
    if (mac ? event.metaKey : ctrl) modifiers.push("Mod");
    if (mac && ctrl) modifiers.push("Ctrl");
    if (!mac && event.metaKey) modifiers.push("Meta");
    if (event.altKey && !altGraph && !(symbol && modifiers.length === 0)) modifiers.push("Alt");
    if (event.shiftKey && !symbol) modifiers.push("Shift");

    const binding = [...modifiers, key].join("+");

    return parseBinding(binding) ? binding : undefined;
};

/** The key a binding names, as it was written rather than as it is compared: `+` stays `+`. */
const writtenKey = (binding: string) =>
    binding === "+" || binding.endsWith("++") ? "+" : binding.slice(binding.lastIndexOf("+") + 1);

/** The arrows as they are printed on the keys. */
const ARROWS: Record<string, string> = { ArrowLeft: "←", ArrowRight: "→", ArrowUp: "↑", ArrowDown: "↓" };

/** macOS's modifier glyphs, in the order its menus print them. */
const MAC_GLYPHS: [Modifier, string][] = [
    ["Ctrl", "⌃"],
    ["Alt", "⌥"],
    ["Shift", "⇧"],
    ["Mod", "⌘"],
    ["Meta", "⌘"],
];

/** The other platforms' modifier names, in the order their menus print them. */
const PC_NAMES: [Modifier, string][] = [
    ["Mod", "Ctrl"],
    ["Ctrl", "Ctrl"],
    ["Meta", "Meta"],
    ["Alt", "Alt"],
    ["Shift", "Shift"],
];

/**
 * A binding as the user presses it on this platform: `⌘ =` and `⌥⇧ Z` on macOS, `Ctrl+=` and
 * `Alt+Shift+Z` elsewhere. `spaceName` is the `Space` key in the user's language. A string that is
 * not a binding is shown as it is.
 */
export const formatBinding = (binding: string, mac: boolean, spaceName: string): string => {
    const parsed = parseBinding(binding);
    if (!parsed) return binding;

    const written = writtenKey(binding);
    const key =
        parsed.key === "Space"
            ? spaceName
            : (ARROWS[written] ?? (written.length === 1 ? written.toUpperCase() : written));

    // A set, because on macOS `Mod` and `Meta` are both Command and one glyph says so.
    const names = new Set(
        (mac ? MAC_GLYPHS : PC_NAMES).filter(([name]) => parsed.modifiers.has(name)).map(([, shown]) => shown),
    );

    if (mac) return names.size === 0 ? key : `${[...names].join("")} ${key}`;

    return [...names, key].join("+");
};

/**
 * A stored set of bindings, repaired: what the settings store reads back on start.
 *
 * Platform-independent, because the store rehydrates at import, before anything may ask which
 * platform this is. Anything that is not an object reads as no choices; an action this application
 * does not offer is dropped, and so is a value that is not a list of strings (that action reads its
 * defaults). Within a list, a string that is not a binding, one refused on every platform, and an
 * exact repeat are dropped, and at most {@link MAX_BINDINGS} are kept. A list that had bindings and
 * loses every one reads as the defaults rather than as none: an empty list is a choice the user made,
 * and nothing honoured of what they chose is not that choice.
 */
export const repairShortcuts = (value: unknown): StoredShortcuts => {
    if (value === null || typeof value !== "object" || Array.isArray(value)) return {};

    const repaired: StoredShortcuts = {};
    for (const [action, list] of Object.entries(value)) {
        if (!isShortcutAction(action) || !Array.isArray(list)) continue;
        if (!list.every((binding): binding is string => typeof binding === "string")) continue;

        const kept = [...new Set(list)]
            .filter((binding) => parseBinding(binding) && !(refusal(binding, true) && refusal(binding, false)))
            .slice(0, MAX_BINDINGS);

        if (list.length === 0 || kept.length > 0) repaired[action] = kept;
    }

    return repaired;
};

/**
 * The bindings in force on this platform: the only way anything reads them.
 *
 * Each action the user changed takes its stored list, minus a binding refused here or one that
 * overlaps a binding an earlier action already took; every other action takes its defaults, minus one
 * that overlaps a binding already placed. So the user's choice beats a default, and no key press is
 * claimed twice - whatever is on disk, and whatever a later version does to the defaults.
 */
export const resolveShortcuts = (stored: StoredShortcuts, mac: boolean): ShortcutTable => {
    const taken: string[] = [];
    const place = (bindings: readonly string[]) => {
        const placed = bindings.filter(
            (binding) =>
                parseBinding(binding) !== undefined &&
                refusal(binding, mac) === undefined &&
                !taken.some((other) => overlaps(other, binding, mac)),
        );
        taken.push(...placed);

        return placed;
    };

    const table = {} as Record<ShortcutAction, readonly string[]>;
    for (const action of SHORTCUT_ACTIONS) {
        const own = stored[action];
        if (own) table[action] = place(own);
    }
    for (const action of SHORTCUT_ACTIONS) {
        if (!stored[action]) table[action] = place(DEFAULT_SHORTCUTS[action]);
    }

    return table;
};
