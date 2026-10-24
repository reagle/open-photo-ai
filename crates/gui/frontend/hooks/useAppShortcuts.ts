import { useTranslation } from "react-i18next";
import { isMacOs, isWindows } from "@/ipc/os";
import {
    APP_ACTIONS,
    type AppAction,
    appShortcuts,
    ariaKeyShortcuts,
    formatBinding,
    type Platform,
} from "@/lib/shortcuts";

/** The platform as far as the application's keys differ. Asked again on each call; it never changes. */
export const currentPlatform = (): Platform => (isMacOs() ? "macos" : isWindows() ? "windows" : "linux");

/**
 * The application's own actions with their fixed keys as this platform writes them - `⌘ W` on macOS,
 * `Ctrl+W` elsewhere - and each action's name, in the order the Keyboard page and the shortcuts list
 * draw them. Read-only: these keys are not the user's to change.
 */
export const useAppShortcuts = (): { action: AppAction; name: string; keys: string[] }[] => {
    const { t } = useTranslation();
    const platform = currentPlatform();
    const table = appShortcuts(platform);

    return APP_ACTIONS.map((action) => ({
        action,
        name: t(`settings.keyboard.appActions.${action}`),
        keys: table[action].map((binding) =>
            formatBinding(binding, platform === "macos", t("settings.keyboard.space")),
        ),
    }));
};

/**
 * What a control standing for one of the application's actions says about its keys: the action's first
 * key as this platform writes it, `undefined` where it has none, and `aria-keyshortcuts` naming them all.
 * Fixed rather than rebindable, so nothing here follows the settings store.
 */
export const useAppShortcutKeys = (action: AppAction): { key: string | undefined; aria: string | undefined } => {
    const { t } = useTranslation();
    const platform = currentPlatform();
    const bindings = appShortcuts(platform)[action];
    const first = bindings[0];

    return {
        key: first === undefined ? undefined : formatBinding(first, platform === "macos", t("settings.keyboard.space")),
        aria: ariaKeyShortcuts(bindings, platform === "macos") || undefined,
    };
};

/** The tooltip and `aria-keyshortcuts` of a control for an application action - see `useShortcutHint`. */
export const useAppShortcutHint = (
    action: AppAction,
    label: string,
): { tooltip: string; ariaKeyShortcuts: string | undefined } => {
    const { t } = useTranslation();
    const { key, aria } = useAppShortcutKeys(action);

    return {
        tooltip: key === undefined ? label : t("shortcuts.tooltip", { name: label, key }),
        ariaKeyShortcuts: aria,
    };
};
