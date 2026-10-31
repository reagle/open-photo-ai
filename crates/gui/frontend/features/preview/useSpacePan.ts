import { useEffect } from "react";
import { isMacOs } from "@/ipc/os";
import { BLOCKING_LAYER } from "@/lib/layers";
import { actionFor, resolveShortcuts } from "@/lib/shortcuts";
import { useMaskStore } from "@/stores/mask";
import { usePanKeyStore } from "@/stores/panKey";
import { useSettingsStore } from "@/stores/settings";

/** Whether `target` takes typed text, which a key meant for it is never one for the photograph. */
export const isTyping = (target: EventTarget | null) =>
    target instanceof HTMLElement && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName));

/** Whether `target` is a control that Space presses, which a keyboard user has focused on purpose. */
const takesSpace = (target: EventTarget | null) =>
    target instanceof HTMLElement && target.closest("button, [role='switch'], [role='radio'], a[href]") !== null;

const isSpace = (event: KeyboardEvent) => event.key === " " || event.code === "Space";

/** Whether the user has bound a bare Space to one of the window's actions, which then has it. */
const spaceIsBound = () => {
    const mac = isMacOs();
    const space = { key: " ", ctrlKey: false, metaKey: false, altKey: false, shiftKey: false };

    return actionFor(space, resolveShortcuts(useSettingsStore.getState().shortcuts, mac), mac) !== undefined;
};

/**
 * **Space held is the preview's pan modifier**, as in Photoshop and Lightroom: while it is down a press on the
 * photograph drags it - instead of a brush's stroke, and instead of Set focus's click - and the cursor is a hand.
 * Mounted by the preview for as long as a photograph is drawn; the state is `stores/panKey.ts`.
 *
 * **A held modifier, not a shortcut.** Space is not a default *binding* because a Radix menu hands the focus back to
 * its trigger button as it closes, and the next Space would press it again (`DEFAULT_SHORTCUTS`). That reason holds
 * here too, so Space is left to whatever operates it: **a focused button, switch, radio or link keeps its Space**, and
 * so does a text field. Not while a dialog, a menu or a listbox is open over the photograph; an options panel is not
 * one of those (`BLOCKING_LAYER`), so the photograph can be panned while it is set. A user who has bound Space to an
 * action keeps it for that action - except while a brush is up, which has always panned with Space.
 *
 * Listened for on the window in the capture phase, as the brush's keys are, and consumed while it pans, so the page
 * does not scroll. A window that loses the focus never hears the key come up, so that lets go of it as well.
 */
export const useSpacePan = () => {
    useEffect(() => {
        const { setHeld } = usePanKeyStore.getState();

        const onKeyDown = (event: KeyboardEvent) => {
            if (!isSpace(event) || event.defaultPrevented) return;
            if (isTyping(event.target) || takesSpace(event.target) || document.querySelector(BLOCKING_LAYER)) return;
            if (useMaskStore.getState().editing === undefined && spaceIsBound()) return;
            if (event.metaKey || event.ctrlKey || event.altKey) return;

            event.preventDefault();
            setHeld(true);
        };

        const onKeyUp = (event: KeyboardEvent) => {
            // Only the Space this took: one pressed on a control it left alone is that control's to release.
            if (!isSpace(event) || !usePanKeyStore.getState().held) return;

            event.preventDefault();
            setHeld(false);
        };

        const onBlur = () => setHeld(false);

        window.addEventListener("keydown", onKeyDown, true);
        window.addEventListener("keyup", onKeyUp, true);
        window.addEventListener("blur", onBlur);

        return () => {
            window.removeEventListener("keydown", onKeyDown, true);
            window.removeEventListener("keyup", onKeyUp, true);
            window.removeEventListener("blur", onBlur);
            setHeld(false);
        };
    }, []);
};
