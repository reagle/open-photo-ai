import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { configureMenu, onMenuItem } from "@/ipc/menu";
import { isMacOs } from "@/ipc/os";
import { quit } from "@/ipc/setup";
import { APP_NAME } from "@/lib/constants";
import { report } from "@/lib/report";
import { type AppAction, appActionFor, isAppAction, isOverlayOpen, isWindowShortcut } from "@/lib/shortcuts";
import { useFileStore, useHasFiles } from "@/stores/files";
import { useSettingsDialogStore } from "@/stores/settingsDialog";
import { currentPlatform } from "./useAppShortcuts";
import { useOpenImages } from "./useOpenImages";

/** Every label in the macOS menu bar the catalogue translates, by its key under `menu.bar` - Rust's keys. */
const MENU_LABELS = [
    "about",
    "services",
    "hide",
    "hideOthers",
    "showAll",
    "quit",
    "file",
    "open",
    "closeImage",
    "closeAll",
    "settings",
    "edit",
    "undo",
    "redo",
    "cut",
    "copy",
    "paste",
    "selectAll",
    "view",
    "fullscreen",
    "window",
    "minimize",
    "zoom",
    "bringAllToFront",
    "help",
] as const;

/**
 * The application's own actions - Open, Settings, Close Image, Close All Images and Quit - mounted once
 * by the shell.
 *
 * **One dispatcher, two inputs.** On macOS the menu bar holds the keys, and Rust forwards each of its
 * items here as `app-menu`; on Windows and Linux, which have no menu bar, one `keydown` listener on
 * `window` matches the same fixed keys. Never both: on macOS AppKit matches a menu's key before the
 * page could, and a listener here as well would act twice for one press.
 *
 * Each action does what its control in the window does and nothing else: Open is Browse images, Close
 * Image and Close All Images are the file options menu's (and do nothing with no image open), Settings
 * is the navbar's button, and Quit - only Linux's Ctrl+Q reaches here - is the setup dialog's. None of
 * them closes the window.
 *
 * While a dialog or a menu is open over the window, none of them takes effect - so Command-W never
 * closes an image under an open export dialog. A key press is offered only when `isWindowShortcut`
 * says it is the window's, which also leaves a text field its keys. Quit is the exception to both:
 * it ends the application whatever is open, as macOS's Command-Q does.
 */
export const useAppActions = () => {
    const { t } = useTranslation();
    const openImages = useOpenImages("menu");
    const hasFiles = useHasFiles();

    // The menu bar in the interface language, with Close Image and Close All Images available only
    // while an image is open: at mount, and again whenever either changes. `t` is a new function on
    // each change of language, which is what re-runs this.
    useEffect(() => {
        if (!isMacOs()) return;

        const labels = Object.fromEntries(MENU_LABELS.map((key) => [key, t(`menu.bar.${key}`, { name: APP_NAME })]));
        configureMenu(labels, hasFiles).catch((error: unknown) => report("Failed to relabel the menu", error));
    }, [t, hasFiles]);

    // Through a ref, so the listeners are added once rather than again each time the picker's wording
    // follows a change of language - a menu event in the gap between two `listen`s would be lost.
    const openRef = useRef(openImages);
    useEffect(() => {
        openRef.current = openImages;
    }, [openImages]);

    useEffect(() => {
        const platform = currentPlatform();

        const perform = (action: AppAction) => {
            const files = useFileStore.getState();

            switch (action) {
                case "open":
                    openRef.current();
                    break;
                case "settings":
                    useSettingsDialogStore.getState().openSettings();
                    break;
                case "closeImage": {
                    const current = files.files.at(files.currentIndex);
                    if (current) files.closeFile(current.path);
                    break;
                }
                case "closeAll":
                    if (files.files.length > 0) files.closeAll();
                    break;
                case "quit":
                    quit().catch((error: unknown) => report("Failed to quit", error));
                    break;
            }
        };

        if (platform === "macos") {
            // Quit is AppKit's own item there and never arrives here; the rest wait for no overlay.
            const stop = onMenuItem((id) => {
                if (isAppAction(id) && !isOverlayOpen()) perform(id);
            });

            return () => {
                stop.then((unlisten) => unlisten()).catch((error: unknown) =>
                    report("Failed to stop listening to the menu", error),
                );
            };
        }

        const onKeyDown = (event: KeyboardEvent) => {
            if (event.defaultPrevented) return;

            const action = appActionFor(event, platform);
            if (!action || (action !== "quit" && !isWindowShortcut(event))) return;

            event.preventDefault();
            // Holding Ctrl+W down would close one image per repeat.
            if (!event.repeat) perform(action);
        };

        window.addEventListener("keydown", onKeyDown);

        return () => window.removeEventListener("keydown", onKeyDown);
    }, []);
};
