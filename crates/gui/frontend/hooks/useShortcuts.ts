import { useEffect } from "react";
import { isMacOs } from "@/ipc/os";
import { actionFor, DEFAULT_SHORTCUTS, isWindowShortcut, type ShortcutAction } from "@/lib/shortcuts";
import { usePeekStore } from "@/stores/peek";
import { fitCurrent, stepZoom } from "@/stores/transform";

/** What each one-shot action does. `showOriginal` is not one: it lasts for as long as its key is held. */
const PRESS: Record<Exclude<ShortcutAction, "showOriginal">, () => void> = {
    zoomIn: () => stepZoom(1),
    zoomOut: () => stepZoom(-1),
    fit: fitCurrent,
};

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

            const action = actionFor(event, DEFAULT_SHORTCUTS, mac);
            if (!action) return;

            event.preventDefault();

            if (action !== "showOriginal") {
                PRESS[action]();
                return;
            }

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
