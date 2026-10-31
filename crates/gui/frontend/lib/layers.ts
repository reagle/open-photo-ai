import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import { flushSync } from "react-dom";

/*
 * **Two kinds of surface, and the keys each answers.** The rules every surface over the photograph follows, so a new
 * one is one or the other rather than a third thing:
 *
 * - A **draft dialog** - Settings, Export, Crop/Rotate, Select faces - holds changes that are not kept until its
 *   primary button is pressed. Escape is Cancel; Return is the primary button (`DIALOG_PRIMARY`), unless the focus is
 *   somewhere Return already means something (`returnAccepts`). It owns every key while it is open.
 * - A **live surface** - an enhancement's options panel, the Remove, Refine and limiting-mask brushes, Set focus -
 *   changes the photograph as it is used, so there is nothing to cancel: Escape and Return both mean Done, keeping
 *   what was done. The photograph stays operable under it: the view's keys (`VIEW_ACTIONS` in `lib/shortcuts.ts`), the
 *   wheel, pinch, drag and Space-drag all still reach the preview, and a press on the preview or the drawer's zoom
 *   (`KEEPS_LIVE_SURFACE`) leaves the panel open. A press anywhere else closes it.
 */

/**
 * What is open over the photograph and has an Escape - and keys - of its own: a dialog or alert dialog (a popover's
 * panel is one), a menu or a listbox, and not one that is closing.
 *
 * What the canvas's own Escape, Return and brush letters stand aside for: a key meant for a surface drawn over the
 * photograph is not one for the photograph - an options panel's included, whose Escape closes the panel first. The
 * view's keys and the Space pan ask {@link BLOCKING_LAYER} instead, which a live surface is not.
 */
export const OPEN_LAYER = ['[role="dialog"]', '[role="alertdialog"]', '[role="menu"]', '[role="listbox"]']
    .map((layer) => `${layer}:not([data-state="closed"])`)
    .join(",");

/**
 * The attribute that marks a **live surface** drawn over the window: an enhancement's options panel. See the rules
 * above; `lib/shortcuts.ts` reads it to let the view's keys through.
 */
export const LIVE_SURFACE = "data-live-surface";

/** {@link OPEN_LAYER} less the live surfaces: what is open over the photograph and takes even the view's keys. */
export const BLOCKING_LAYER = ['[role="dialog"]', '[role="alertdialog"]', '[role="menu"]', '[role="listbox"]']
    .map((layer) => `${layer}:not([data-state="closed"], [${LIVE_SURFACE}])`)
    .join(",");

/**
 * The attribute on the regions a press inside of leaves a live surface open: the preview's canvas and the drawer's
 * zoom. Pressing either is looking at the photograph, which is what the panel is open beside.
 */
export const KEEPS_LIVE_SURFACE = "data-keeps-live-surface";

/** Whether a press or a focus landing on `target` is one a live surface stays open for. */
export const keepsLiveSurface = (target: EventTarget | null): boolean =>
    target instanceof Element && target.closest(`[${KEEPS_LIVE_SURFACE}]`) !== null;

/** The attribute on a draft dialog's primary button - Save, Apply - which Return presses. */
export const DIALOG_PRIMARY = "data-dialog-primary";

// A button presses itself on Return, a link follows, a text area takes a new line, and a select, a combobox, a menu
// or a listbox chooses: Return there is the control's. A checkbox and a radio are buttons in Radix and are left alone
// too, which costs nothing - Return never ticked one.
/** Where Return operates the focused control itself, and is never a surface's accept. */
const RETURN_OPERATES = [
    "button",
    "a[href]",
    "textarea",
    "select",
    "input[type='button']",
    "input[type='submit']",
    "input[type='reset']",
    "input[type='file']",
    "[contenteditable]:not([contenteditable='false'])",
    "[role='button']",
    "[role='checkbox']",
    "[role='radio']",
    "[role='switch']",
    "[role='tab']",
    "[role='combobox']",
    "[role='listbox']",
    "[role='option']",
    "[role='menu']",
    "[role='menuitem']",
    "[role='menuitemcheckbox']",
    "[role='menuitemradio']",
].join(",");

/** The parts of a key event, native or React's, that {@link returnAccepts} reads. */
type ReturnLike = Pick<
    KeyboardEvent,
    "key" | "target" | "defaultPrevented" | "shiftKey" | "altKey" | "ctrlKey" | "metaKey"
> & {
    isComposing?: boolean;
    nativeEvent?: { isComposing?: boolean };
};

/**
 * Whether a key press is Return meaning "accept": a draft dialog's primary button, a live surface's Done.
 *
 * Plain Return only, with no modifier; not while an input method is composing (its Return commits the text); not once
 * a control has handled it (the Keyboard settings page's recorder, which also stops it); and not where Return operates
 * the focused control itself - a button, a link, a text area, a select or a menu. A single-line field is not one of
 * those: Return there commits the field and accepts, as a native dialog's default button does.
 */
export const returnAccepts = (event: ReturnLike): boolean => {
    if (event.key !== "Enter" || event.defaultPrevented) return false;
    if (event.shiftKey || event.altKey || event.ctrlKey || event.metaKey) return false;
    if (event.isComposing === true || event.nativeEvent?.isComposing === true) return false;

    return !(event.target instanceof Element && event.target.closest(RETURN_OPERATES));
};

/**
 * Accepts on Return, if the press is one ({@link returnAccepts}): prevents it, lets a focused field commit what was
 * typed - by blurring it, which is when every field here commits, with that render flushed - and then calls `accept`,
 * which so sees the committed value. Answers whether it did.
 */
export const acceptOnReturn = (event: ReturnLike & { preventDefault: () => void }, accept: () => void): boolean => {
    if (!returnAccepts(event)) return false;

    event.preventDefault();

    const field = event.target instanceof HTMLInputElement ? event.target : undefined;
    if (field) flushSync(() => field.blur());

    accept();

    return true;
};

/**
 * A draft dialog's Return, for its content's `onKeyDown`: presses the enabled button marked {@link DIALOG_PRIMARY}
 * inside it, if there is one. Through a click rather than a handler, so Return and the button can never come to do
 * different things.
 */
export const pressPrimaryOnReturn = (event: ReactKeyboardEvent<HTMLElement>) => {
    const dialog = event.currentTarget;
    const primary = () => dialog.querySelector<HTMLButtonElement>(`[${DIALOG_PRIMARY}]:not(:disabled)`);
    if (!primary()) return;

    // Asked again once the field has committed: the commit can change whether the button is enabled.
    acceptOnReturn(event, () => primary()?.click());
};
