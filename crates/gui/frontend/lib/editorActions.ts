import { toast } from "sonner";
import i18n from "@/i18n";
import type { Operation } from "@/ipc/enhance";
import { revealImage } from "@/ipc/images";
import { ENHANCEMENTS } from "@/lib/enhancements";
import { track } from "@/lib/faro";
import { report } from "@/lib/report";
import { useAddEnhancementMenuStore } from "@/stores/addEnhancementMenu";
import { useAutopilotStore } from "@/stores/autopilot";
import { useCropDialogStore } from "@/stores/cropDialog";
import { useDrawerStore } from "@/stores/drawer";
import { useEnhancementStore } from "@/stores/enhancements";
import { useFileStore } from "@/stores/files";
import { useOptionsPanelStore } from "@/stores/optionsPanel";
import { nextPreviewMode, usePreviewStore } from "@/stores/preview";

/**
 * What the editor's keyboard shortcuts do: each the same as the control it stands for, and unavailable
 * whenever that control is - Add enhancement and Crop/Rotate with no image open, the comparison and the
 * drawer's fold with nothing open at all.
 *
 * Outside the components, reading the stores when called, for the reason `stepZoom` is: a key listener
 * is not a component. The controls share the two that have more to them than a store write - removing
 * an enhancement and showing a file in the file manager - so the two routes are one.
 *
 * Each answers `false` when it did nothing, so the listener leaves that key press alone (a `Backspace`
 * away from the enhancement list is not taken from anything).
 */

type Family = Operation["family"];

/** Where an enhancement's row says which enhancement it is. */
const ROW = "[data-slot='enhancement-row']";

/** The row's own control, the one that opens its options and holds the focus between presses. */
const ROW_TRIGGER = "[data-slot='enhancement-trigger']";

/** The Add enhancement button, where the focus goes when the last row is removed. */
const ADD_TRIGGER = "[data-slot='add-enhancement']";

/** The current image, if one is open. */
const currentFile = () => {
    const { files, currentIndex } = useFileStore.getState();

    return files.at(currentIndex);
};

/** Takes one enhancement off one image's stack, as its row's remove button does. */
export const removeEnhancementOf = (path: string, family: Family) => {
    useEnhancementStore.getState().removeEnhancement(path, family);
    track("enhancement_removed", { family });
};

/**
 * Shows a file in the platform's file manager, as the file options menu's item does. A file moved or
 * deleted since it was opened is reported to the console and in a toast.
 */
export const revealInFileManager = (path: string) => {
    revealImage(path).catch((error: unknown) => {
        report("showing the image in the file manager failed", error);
        toast.error(i18n.t("errors.revealFileFailed"));
    });
};

/** Autopilot on or off, as its switch. */
export const toggleAutopilot = () => useEnhancementStore.getState().toggle();

/** Opens the Add enhancement menu, as its button does. Nothing with no image open. */
export const openAddEnhancement = (): boolean => {
    if (!currentFile()) return false;

    useAddEnhancementMenuStore.getState().setOpen(true);
    return true;
};

/**
 * Removes the enhancement whose row has the focus, as that row's remove button does, and moves the focus
 * to the row that takes its place - the next, else the one before, else the Add enhancement button - so
 * a second press removes the next one rather than nothing. `false` with the focus anywhere else.
 */
export const removeFocusedEnhancement = (document: Document = globalThis.document): boolean => {
    const row = document.activeElement?.closest<HTMLElement>(ROW);
    const family = row?.dataset.family as Family | undefined;
    const file = currentFile();
    if (!row || !family || !file) return false;

    const next = row.nextElementSibling?.matches(ROW) ? row.nextElementSibling : row.previousElementSibling;
    const successor =
        (next?.matches(ROW) ? next.querySelector<HTMLElement>(ROW_TRIGGER) : null) ??
        document.querySelector<HTMLElement>(ADD_TRIGGER);

    // Before the row goes, so the focus never falls back to the page in between.
    successor?.focus();
    removeEnhancementOf(file.path, family);

    return true;
};

/**
 * Opens the options of the enhancement in this place in the list, counting from 1, as a press on its row
 * does. Nothing past the end of the list, with no image open, or while Autopilot is analysing the image
 * and the list is not drawn.
 */
export const openOptionsAt = (place: number): boolean => {
    const file = currentFile();
    if (!file || useAutopilotStore.getState().analysing.has(file.path)) return false;

    // The rows the list draws: an operation whose family `ENHANCEMENTS` does not name draws none.
    const families = (useEnhancementStore.getState().enhancements.get(file.path) ?? [])
        .map((operation) => operation.family)
        .filter((family) => ENHANCEMENTS.some((entry) => entry.family === family));
    const family = families[place - 1];
    if (!family) return false;

    useOptionsPanelStore.getState().openPanel(family);
    return true;
};

/** Opens Crop/Rotate, as the miniature's button does - which is drawn only for an image with pixels to frame. */
export const openCropRotate = (): boolean => {
    if (!currentFile()?.identity) return false;

    useCropDialogStore.getState().setOpen(true);
    return true;
};

/** The next comparison - full, side by side, split, and round again - as the drawer's toggles choose one. */
export const cyclePreview = (): boolean => {
    if (useFileStore.getState().files.length === 0) return false;

    const { previewMode, setPreviewMode } = usePreviewStore.getState();
    const mode = nextPreviewMode(previewMode);
    setPreviewMode(mode);
    track("preview_mode_changed", { mode });

    return true;
};

/** Shows or hides the strip of open images, as the drawer's fold button does. */
export const toggleDrawer = (): boolean => {
    if (useFileStore.getState().files.length === 0) return false;

    useDrawerStore.getState().toggle();
    return true;
};

/** Shows the current image in the file manager. Nothing with no image open. */
export const revealCurrent = (): boolean => {
    const file = currentFile();
    if (!file) return false;

    revealInFileManager(file.path);
    return true;
};
