import { useTranslation } from "react-i18next";
import { isMacOs, isWindows } from "@/ipc/os";
import { APP_ACTIONS, type AppAction, appShortcuts, formatBinding, type Platform } from "@/lib/shortcuts";

/** The platform as far as the application's keys differ. Asked again on each call; it never changes. */
export const currentPlatform = (): Platform => (isMacOs() ? "macos" : isWindows() ? "windows" : "linux");

/**
 * The application's own actions with their fixed keys as this platform writes them - `⌘ W` on macOS,
 * `Ctrl+W` elsewhere - and each action's name, in the order the Keyboard page and the shortcuts list
 * draw them. Read-only: these keys are not the user's to change.
 */
export const useAppShortcuts = (): { action: AppAction; name: string; keys: string }[] => {
    const { t } = useTranslation();
    const platform = currentPlatform();
    const table = appShortcuts(platform);

    return APP_ACTIONS.map((action) => ({
        action,
        name: t(`settings.keyboard.appActions.${action}`),
        keys: formatBinding(table[action], platform === "macos", t("settings.keyboard.space")),
    }));
};
