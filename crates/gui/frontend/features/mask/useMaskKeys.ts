import { useEffect } from "react";
import { OPEN_LAYER } from "@/lib/layers";
import { type Target, useMaskStore } from "@/stores/mask";

/** Whether `target` takes typed text, which a key meant for it is never one for the brush. */
const isTyping = (target: EventTarget | null) =>
    target instanceof HTMLElement && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName));

/** Whether `target` is a control that Space presses, which a keyboard user has focused on purpose. */
const takesSpace = (target: EventTarget | null) =>
    target instanceof HTMLElement && target.closest("button, [role='switch'], [role='radio'], a[href]") !== null;

/**
 * The keys of editing a mask, for as long as this is mounted - which the panel is exactly while the preview is
 * painting.
 *
 * ```
 *   B  paint        E  erase        X  swap          O  overlay
 *   [  ]  size      Shift+[  Shift+]  softness
 *   Cmd/Ctrl+Z  undo      Shift+Cmd/Ctrl+Z  redo      (Ctrl+Y too)
 *   Escape  done          Space (held)  pan instead of paint
 * ```
 *
 * **Registered here, not through the fork's keyboard-shortcut settings**, which this branch does not have. When both
 * are merged the shortcut series can adopt these; what it must keep is that Undo and Redo are taken here and go no
 * further.
 *
 * **Ignored while a text field, a dialog or a menu has the keys**, the rule the canvas applies to Escape. The handler
 * is on the window in the capture phase and consumes only the keys it acts on: Undo and Redo are stopped before any
 * other handler sees them, so Cmd+Z while editing a mask undoes a stroke and nothing else.
 *
 * **Space is the pan modifier, and does not press a button.** A button the mouse has just clicked keeps focus, and a
 * Space pressed to pan would press it again; the panel gives the focus back after a click, and a Space meant for a
 * control a keyboard user has focused on purpose is left to it.
 *
 * Undo and Redo are read by what they spell, as the command is the letter on the key; the brush's keys - the letters
 * and the brackets - by where they are, so a layout that spells them differently, or a Cyrillic one, still has them
 * where the hand expects. `Shift+[` types `{` on some keyboards and the key is the same key.
 */
export const useMaskKeys = (path: string, target: Target = "limit") => {
    useEffect(() => {
        const store = () => useMaskStore.getState();

        const onKeyDown = (event: KeyboardEvent) => {
            if (event.defaultPrevented || isTyping(event.target) || document.querySelector(OPEN_LAYER)) return;

            const key = event.key.toLowerCase();
            const command = event.metaKey || event.ctrlKey;

            // Taken before anything else sees it, and consumed: `Cmd+Z` belongs to the mask while it is being edited.
            if (command && !event.altKey && (key === "z" || key === "y")) {
                event.preventDefault();
                event.stopPropagation();

                // The region being edited's own history: the limiting mask's and a removal's are never one undo.
                if (key === "y" || event.shiftKey) store().redo(path, target);
                else store().undo(path, target);

                return;
            }

            if (event.key === "Escape") {
                event.preventDefault();
                store().stopEditing();

                return;
            }

            if (key === " " || event.code === "Space") {
                if (takesSpace(event.target)) return;

                event.preventDefault();
                store().setPanning(true);

                return;
            }

            // A chord is some other command's: Cmd+B is not Paint.
            if (command || event.altKey) return;

            const handled = (act: () => void) => {
                event.preventDefault();
                act();
            };

            if (event.code === "BracketLeft")
                handled(() => (event.shiftKey ? store().stepSoftness(-1) : store().stepSize(-1)));
            else if (event.code === "BracketRight")
                handled(() => (event.shiftKey ? store().stepSoftness(1) : store().stepSize(1)));
            else if (event.shiftKey) return;
            else if (event.code === "KeyB") handled(() => store().setMode("paint"));
            else if (event.code === "KeyE") handled(() => store().setMode("erase"));
            else if (event.code === "KeyX") handled(() => store().swapMode());
            else if (event.code === "KeyO") handled(() => store().setOverlay(!store().tool.overlay));
        };

        const onKeyUp = (event: KeyboardEvent) => {
            // Only the Space this took: one pressed on a control it left alone is that control's to release.
            if ((event.key !== " " && event.code !== "Space") || !store().panning) return;

            event.preventDefault();
            store().setPanning(false);
        };

        // A window that loses the focus never hears the key come up.
        const onBlur = () => store().setPanning(false);

        window.addEventListener("keydown", onKeyDown, true);
        window.addEventListener("keyup", onKeyUp, true);
        window.addEventListener("blur", onBlur);

        return () => {
            window.removeEventListener("keydown", onKeyDown, true);
            window.removeEventListener("keyup", onKeyUp, true);
            window.removeEventListener("blur", onBlur);
            store().setPanning(false);
        };
    }, [path, target]);
};
