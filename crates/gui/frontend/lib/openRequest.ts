import { familyEntry } from "@/hooks/useCatalogue";
import { catalogue } from "@/ipc/catalogue";
import type { ImageRecord, OpenRequest } from "@/ipc/images";
import { DEFAULT_AMOUNT, fNumberForAmount } from "@/lib/blurAmount";
import { applyOrder, FOCUS_X, FOCUS_Y, newOperation, ownLens, withParameter } from "@/lib/enhancements";
import { useEnhancementStore } from "@/stores/enhancements";
import { useSettingsStore } from "@/stores/settings";

/**
 * Switches Autopilot off, as `opai --gui --lens-blur` asks: the command line names the enhancement itself, as it does
 * for a run, where Autopilot cannot be combined with one.
 *
 * Before the photographs are opened, so the one made current is not analysed first.
 */
export const prepareForRequest = () => useEnhancementStore.getState().setAutopilot(false);

/**
 * Gives each photograph of `admitted` that `request` names the Lens Blur it asks for: the one the add menu would add,
 * at the request's f-number - or its blur amount read against the photograph's own lens, as the slider reads it - and
 * its focus point. One already on the photograph is replaced.
 *
 * A photograph opened whole: nothing has been cropped yet, so the lens is its own.
 */
export const applyRequest = async (request: OpenRequest, admitted: readonly ImageRecord[]) => {
    const asked = new Set(request.paths);
    const files = admitted.filter((file) => asked.has(file.path));
    if (files.length === 0) return;

    const families = await catalogue();
    const entry = familyEntry(families, "lens_blur");
    const stored = useSettingsStore.getState().models.lens_blur;
    const { fNumber, amount, focusX, focusY } = request.lensBlur;

    for (const file of files) {
        const own = fNumber ?? fNumberForAmount(amount ?? DEFAULT_AMOUNT, (await ownLens(file, undefined)).ownFNumber);
        let operation = newOperation("lens_blur", entry, stored, 1, own);
        if (!operation) return;

        if (focusX !== undefined && focusY !== undefined) {
            operation = withParameter(withParameter(operation, FOCUS_X, focusX), FOCUS_Y, focusY);
        }

        const store = useEnhancementStore.getState();
        const present = store.enhancements.get(file.path)?.some((operation) => operation.family === "lens_blur");

        if (present) store.replaceEnhancement(file.path, operation);
        else store.addEnhancement(file.path, operation, applyOrder(families));
    }
};
