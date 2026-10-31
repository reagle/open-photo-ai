import type { ParseKeys, TFunction } from "i18next";
import { formatBinding, parseBinding, type ShortcutAction } from "@/lib/shortcuts";

/**
 * How the shortcuts list lays out the window's actions: under three headings, with actions that differ
 * only in direction or position (pan left and right, previous and next image, the seven enhancement
 * rows) on one row. The Keyboard settings page keeps one row per action, because that is where each is
 * rebound; this list is read at a glance.
 */

/** One row of the list: one action, or several alike ones shown together. */
export type ListRow = {
    id: string;
    label: ParseKeys;
    actions: readonly ShortcutAction[];
};

/** One headed group of rows. */
export type ListSection = {
    id: "view" | "images" | "enhancements";
    rows: readonly ListRow[];
};

const single = (action: ShortcutAction): ListRow => ({
    id: action,
    label: `settings.keyboard.actions.${action}` as ParseKeys,
    actions: [action],
});

/** Every action in `SHORTCUT_ACTIONS` exactly once, as the tests check. */
export const LIST_SECTIONS: readonly ListSection[] = [
    {
        id: "view",
        rows: [
            { id: "zoom", label: "shortcuts.rows.zoom", actions: ["zoomIn", "zoomOut"] },
            single("fit"),
            single("actualPixels"),
            single("toggleZoom"),
            { id: "pan", label: "shortcuts.rows.pan", actions: ["panLeft", "panRight", "panUp", "panDown"] },
            single("showOriginal"),
            single("cyclePreview"),
            single("toggleDrawer"),
            single("showShortcuts"),
        ],
    },
    {
        id: "images",
        rows: [
            { id: "images", label: "shortcuts.rows.images", actions: ["previousImage", "nextImage"] },
            single("revealImage"),
            single("cropRotate"),
        ],
    },
    {
        id: "enhancements",
        rows: [
            single("toggleAutopilot"),
            single("addEnhancement"),
            single("removeEnhancement"),
            {
                id: "options",
                label: "shortcuts.rows.options",
                actions: [
                    "openOptions1",
                    "openOptions2",
                    "openOptions3",
                    "openOptions4",
                    "openOptions5",
                    "openOptions6",
                    "openOptions7",
                    "openOptions8",
                ],
            },
        ],
    },
];

/** The key a binding names, after its modifiers: `Mod+ArrowLeft` is `ArrowLeft`, `Mod++` is `+`. */
const keyOf = (binding: string) => binding.slice(binding.lastIndexOf("+", binding.length - 2) + 1);

/**
 * The keys one row shows, as this platform writes them.
 *
 * Every key cap is one whole combination, as a menu, Lightroom's and Photoshop's shortcut sheets and
 * Zed's keymap write them: panning is `⌘ ←` `⌘ →` `⌘ ↑` `⌘ ↓`, not one cap a reader could take for
 * `⌘ ←` and three bare arrows. The one exception is a run of three or more consecutive digits with the
 * same modifiers, one binding each, which is written as a range: `1–8`. A row the user has rebound
 * unevenly lists every binding, as a single action's row does.
 */
export const rowKeys = (lists: readonly (readonly string[])[], mac: boolean, spaceName: string): string[] => {
    const all = lists.flat();
    const shown = all.map((binding) => formatBinding(binding, mac, spaceName));
    if (lists.length < 3 || lists.some((list) => list.length !== 1)) return shown;

    const modifiers = new Set(all.map((binding) => [...(parseBinding(binding)?.modifiers ?? ["?"])].sort().join("+")));
    const keys = all.map((binding) => formatBinding(keyOf(binding), mac, spaceName));
    const [first = "", last = ""] = [keys[0], keys.at(-1)];
    const run = keys.every((key, index) => /^\d$/.test(key) && Number(key) === Number(first) + index);
    if (modifiers.size !== 1 || modifiers.has("?") || !run || !shown[0]?.endsWith(first)) return shown;

    const prefix = shown[0].slice(0, shown[0].length - first.length);

    return [`${prefix}${first}–${last}`];
};

/** One row of the list's fixed part: what the pointer and the dialogs answer, which no setting rebinds. */
export type InputRow = { id: string; label: string; keys: string[] };

/**
 * **The input rules every surface follows** (`lib/layers.ts`), as the list's last section: Return accepts a dialog and
 * finishes a panel or a brush, Escape cancels a dialog and finishes a panel or a brush, a pinch or the modifier with
 * the wheel zooms, and a scroll, a drag and Space with a drag pan - or, under the Scroll wheel zooms setting, a scroll
 * zooms and the pan is the drag alone. Fixed, so written here rather than read from the bindings.
 */
export const inputRows = (t: TFunction, mac: boolean, wheelZooms: boolean): InputRow[] => {
    const mod = mac ? "⌘" : "Ctrl";
    const space = t("settings.keyboard.space");
    const scroll = t("shortcuts.input.scroll");

    return [
        { id: "accept", label: t("shortcuts.input.accept"), keys: [mac ? "↩" : "Enter"] },
        { id: "escape", label: t("shortcuts.input.escape"), keys: [mac ? "⎋" : "Esc"] },
        {
            id: "zoom",
            label: t("shortcuts.input.zoom"),
            keys: [
                t("shortcuts.input.pinch"),
                t("shortcuts.input.modScroll", { key: mod }),
                ...(wheelZooms ? [scroll] : []),
            ],
        },
        {
            id: "pan",
            label: t("shortcuts.input.pan"),
            keys: [
                ...(wheelZooms ? [] : [scroll]),
                t("shortcuts.input.drag"),
                t("shortcuts.input.spaceDrag", { key: space }),
            ],
        },
    ];
};
