import { useEffect } from "react";
import { isTyping } from "@/features/preview/useSpacePan";
import { acceptOnReturn, OPEN_LAYER } from "@/lib/layers";
import { type Target, useMaskStore } from "@/stores/mask";

/**
 * The keys of editing a mask, for as long as this is mounted - which the panel is exactly while the preview is
 * painting.
 *
 * ```
 *   B  paint        E  erase        X  swap          O  overlay
 *   [  ]  size      Shift+[  Shift+]  softness
 *   Cmd/Ctrl+Z  undo      Shift+Cmd/Ctrl+Z  redo      (Ctrl+Y too)
 *   Escape or Return  done          Space (held)  pan instead of paint
 * ```
 *
 * On a lens blur's refinement the brush's letters are its three tools: `F` focus, `B` blur, `E` erase, and `X` swaps
 * focus and blur.
 *
 * **Registered here, not through the fork's keyboard-shortcut settings**, which this branch does not have. When both
 * are merged the shortcut series can adopt these; what it must keep is that Undo and Redo are taken here and go no
 * further.
 *
 * **A live surface** (`lib/layers.ts`): what is painted is kept as it is painted, so Escape and Return both mean Done.
 * Return is left to a focused button, as everywhere.
 *
 * **Ignored while a text field, a dialog or a menu has the keys**, the rule the canvas applies to Escape. The handler
 * is on the window in the capture phase and consumes only the keys it acts on: Undo and Redo are stopped before any
 * other handler sees them, so Cmd+Z while editing a mask undoes a stroke and nothing else.
 *
 * **Space, the pan modifier, is not here**: it pans everywhere on the preview, not only while painting - see
 * `features/preview/useSpacePan.ts`, which keeps the rule this had that Space does not press a focused button. A
 * button the mouse has just clicked keeps focus, so the panel gives the focus back after a click.
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

            if (acceptOnReturn(event, () => store().stopEditing())) return;

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
            else if (target === "refine" && event.code === "KeyF") handled(() => store().setPaints("focus"));
            else if (target === "refine" && event.code === "KeyB") handled(() => store().setPaints("blur"));
            else if (target === "refine" && event.code === "KeyX") handled(() => store().swapPaints());
            else if (event.code === "KeyB") handled(() => store().setMode("paint"));
            else if (event.code === "KeyE") handled(() => store().setMode("erase"));
            else if (event.code === "KeyX") handled(() => store().swapMode());
            else if (event.code === "KeyO") handled(() => store().setOverlay(!store().tool.overlay));
        };

        window.addEventListener("keydown", onKeyDown, true);

        return () => window.removeEventListener("keydown", onKeyDown, true);
    }, [path, target]);
};
