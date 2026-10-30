import { useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { isMacOs } from "@/ipc/os";
import {
    cyclePreview,
    openAddEnhancement,
    openCropRotate,
    openOptionsAt,
    removeFocusedEnhancement,
    revealCurrent,
    toggleAutopilot,
    toggleDrawer,
    toggleInfo,
} from "@/lib/editorActions";
import {
    actionFor,
    ariaKeyShortcuts,
    formatBinding,
    isWindowShortcut,
    resolveShortcuts,
    type ShortcutAction,
    type ShortcutTable,
} from "@/lib/shortcuts";
import { stepImage } from "@/stores/files";
import { usePeekStore } from "@/stores/peek";
import { useSettingsStore } from "@/stores/settings";
import { useShortcutsListStore } from "@/stores/shortcutsList";
import { actualPixels, fitCurrent, panCurrent, stepZoom, toggleZoom } from "@/stores/transform";

/**
 * What each one-shot action does. `showOriginal` is not one: it lasts for as long as its key is held.
 *
 * One that answers `false` did nothing - Delete away from the enhancement list, a digit past the end of
 * it, a pan with the photograph drawn whole - and its key press is left alone rather than prevented.
 */
const PRESS: Record<Exclude<ShortcutAction, "showOriginal">, () => unknown> = {
    zoomIn: () => stepZoom(1),
    zoomOut: () => stepZoom(-1),
    fit: fitCurrent,
    actualPixels,
    toggleZoom,
    panLeft: () => panCurrent("left"),
    panRight: () => panCurrent("right"),
    panUp: () => panCurrent("up"),
    panDown: () => panCurrent("down"),
    cyclePreview,
    toggleInfo,
    toggleDrawer,
    previousImage: () => stepImage(-1),
    nextImage: () => stepImage(1),
    revealImage: revealCurrent,
    toggleAutopilot,
    addEnhancement: openAddEnhancement,
    removeEnhancement: () => removeFocusedEnhancement(),
    openOptions1: () => openOptionsAt(1),
    openOptions2: () => openOptionsAt(2),
    openOptions3: () => openOptionsAt(3),
    openOptions4: () => openOptionsAt(4),
    openOptions5: () => openOptionsAt(5),
    openOptions6: () => openOptionsAt(6),
    openOptions7: () => openOptionsAt(7),
    openOptions8: () => openOptionsAt(8),
    cropRotate: openCropRotate,
    showShortcuts: () => useShortcutsListStore.getState().setOpen(true),
};

// Held down, a toggle would flip on every repeat, Delete would take the list away row by row and a
// digit would reopen what the first press opened. The steps, the pans and the image keys are meant to
// repeat; fitting, actual pixels and the list do the same thing again, harmlessly.
/** The actions key repeat does nothing further for. */
const ONCE_PER_PRESS: ReadonlySet<ShortcutAction> = new Set([
    "toggleZoom",
    "cyclePreview",
    "toggleInfo",
    "toggleDrawer",
    "revealImage",
    "toggleAutopilot",
    "addEnhancement",
    "removeEnhancement",
    "openOptions1",
    "openOptions2",
    "openOptions3",
    "openOptions4",
    "openOptions5",
    "openOptions6",
    "openOptions7",
    "openOptions8",
    "cropRotate",
]);

/**
 * The window's keyboard shortcuts: one `keydown`/`keyup` pair on `window`, mounted once by the shell.
 *
 * A key press is offered to the bindings only when `isWindowShortcut` says it is the window's, and a
 * press that acts has its default prevented, so a bound `Space` does not scroll and `Mod+=` does nothing
 * else. Every other press is left alone.
 *
 * Show-original lasts while its key is held. Key repeat does nothing further, and it ends on that
 * key's `keyup` - and on the window losing focus or being hidden, because a key released while another
 * application has the focus delivers no `keyup` here, and the original would otherwise stay on screen.
 */
export const useShortcuts = () => {
    useEffect(() => {
        // Once per mount: the platform does not change under a running application.
        const mac = isMacOs();
        const { setShowingOriginal } = usePeekStore.getState();

        // The physical key that started the glance. Its release ends it whatever the modifiers are by
        // then: matching the `keyup` against the binding instead would leave the original stuck on
        // screen if Shift went down while `\` was held, since the release would then read `|`.
        let held: string | undefined;

        const release = () => {
            if (held === undefined) return;

            held = undefined;
            setShowingOriginal(false);
        };

        const onKeyDown = (event: KeyboardEvent) => {
            if (!isWindowShortcut(event)) return;

            // Read at each press rather than once, so a Save takes effect at the next key without the
            // listener being mounted again.
            const action = actionFor(event, resolveShortcuts(useSettingsStore.getState().shortcuts, mac), mac);
            if (!action) return;

            if (action !== "showOriginal") {
                if (event.repeat && ONCE_PER_PRESS.has(action)) {
                    event.preventDefault();
                    return;
                }

                if (PRESS[action]() !== false) event.preventDefault();
                return;
            }

            event.preventDefault();

            // A repeat is the same key still down, so the glance already showing is left as it is.
            if (event.repeat || held !== undefined) return;

            held = event.code || event.key;
            setShowingOriginal(true);
        };

        // Not subject to `isWindowShortcut`: a release has to land even if a dialog opened, or the
        // focus moved into a field, while the key was held.
        //
        // Command's own release ends it too: macOS delivers no `keyup` for a key let go while Command
        // is down, so `\` released under a held Command would otherwise leave the original up.
        const onKeyUp = (event: KeyboardEvent) => {
            if (held === undefined) return;
            if ((event.code || event.key) === held || event.key === "Meta") release();
        };

        const onVisibilityChange = () => {
            if (document.visibilityState === "hidden") release();
        };

        window.addEventListener("keydown", onKeyDown);
        window.addEventListener("keyup", onKeyUp);
        window.addEventListener("blur", release);
        document.addEventListener("visibilitychange", onVisibilityChange);

        return () => {
            window.removeEventListener("keydown", onKeyDown);
            window.removeEventListener("keyup", onKeyUp);
            window.removeEventListener("blur", release);
            document.removeEventListener("visibilitychange", onVisibilityChange);
            release();
        };
    }, []);
};

/**
 * The bindings in force, for a component that names them - the zoom buttons' `aria-keyshortcuts`.
 * Follows the settings store, so a saved rebinding is what they report.
 */
export const useShortcutBindings = (): ShortcutTable => {
    const stored = useSettingsStore((state) => state.shortcuts);

    return useMemo(() => resolveShortcuts(stored, isMacOs()), [stored]);
};

/**
 * The key a hint names for an action, written as this platform writes it, and every binding as
 * `aria-keyshortcuts` takes them.
 *
 * `key` is the first binding the action has now - the first only, since a hint is a glance and the
 * shortcuts list holds the rest - or `undefined` when it has none. **On macOS a `Backspace` binding is
 * preferred to the first**, because the key a Mac prints `delete` on sends `Backspace`: Remove's defaults
 * list `Delete` first, and `⌦` would name a key most Mac keyboards do not have.
 */
export const useShortcutKeys = (
    action: ShortcutAction | undefined,
): { key: string | undefined; aria: string | undefined } => {
    const { t } = useTranslation();
    const table = useShortcutBindings();
    const bindings = action === undefined ? [] : table[action];
    const mac = isMacOs();
    const shown = mac && bindings.includes("Backspace") ? "Backspace" : bindings[0];

    return {
        key: shown === undefined ? undefined : formatBinding(shown, mac, t("settings.keyboard.space")),
        aria: ariaKeyShortcuts(bindings, mac) || undefined,
    };
};

/**
 * What a control standing for a shortcut's action says about its keys: a tooltip naming the control and
 * its key (see {@link useShortcutKeys}), or the name alone when the action has none, and
 * `aria-keyshortcuts` naming every binding (`undefined` for none, so the attribute is left off).
 *
 * An `undefined` action has no keys, for a control whose key depends on where it is - an enhancement's row
 * past the seventh has none. `tooltipKey` is the catalogue string the two are joined with: `shortcuts.tooltip` ("Zoom in (⌘ =)"),
 * or `shortcuts.tooltipCycle` for a key that steps through a group rather than choosing the one control.
 */
export const useShortcutHint = (
    action: ShortcutAction | undefined,
    label: string,
    tooltipKey: "shortcuts.tooltip" | "shortcuts.tooltipCycle" = "shortcuts.tooltip",
): { tooltip: string; ariaKeyShortcuts: string | undefined } => {
    const { t } = useTranslation();
    const { key, aria } = useShortcutKeys(action);

    return { tooltip: key === undefined ? label : t(tooltipKey, { name: label, key }), ariaKeyShortcuts: aria };
};
