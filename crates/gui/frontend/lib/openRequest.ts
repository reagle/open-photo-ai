import { familyEntry } from "@/hooks/useCatalogue";
import { catalogue, type FamilyEntry } from "@/ipc/catalogue";
import type { Operation } from "@/ipc/enhance";
import type { ImageRecord, OpenRequest } from "@/ipc/images";
import { startBatch } from "@/lib/batch";
import { DEFAULT_AMOUNT, fNumberForAmount } from "@/lib/blurAmount";
import {
    applyOrder,
    FOCUS_X,
    FOCUS_Y,
    modelValue,
    newOperation,
    optionFor,
    ownLens,
    withParameter,
} from "@/lib/enhancements";
import { useEnhancementStore } from "@/stores/enhancements";
import { useSettingsStore } from "@/stores/settings";

/** What the library calls a sharpen's amount: `Strength::NAME`, the name the catalogue publishes the range under. */
const STRENGTH = "strength";

/**
 * Switches Autopilot off, as `opai --gui --sharpen` or `--lens-blur` asks: the command line names the enhancements
 * itself, as it does for a run, where Autopilot cannot be combined with one.
 *
 * Before the photographs are opened, so the one made current is not analysed first.
 */
export const prepareForRequest = () => useEnhancementStore.getState().setAutopilot(false);

/**
 * The position among its model's precisions of the sharpen model saved in the settings - 0 for HD, 1 for SD - which is
 * the tier a run without `-q` takes; 0 where none is saved or the catalogue no longer publishes it.
 */
const savedTier = (entry: FamilyEntry | undefined): number => {
    const saved = optionFor(entry, useSettingsStore.getState().models.sharpen ?? "");
    const precisions = entry?.variants.find((published) => published.codename === saved?.codename)?.precisions;

    return saved && precisions ? precisions.indexOf(saved.precision) : 0;
};

/**
 * The Sharpen `request` asks for: the model it names, at its strength or the published default, and at the precision a
 * run would use - the {@link savedTier} of that model, or its first where it publishes no such tier. `undefined` for a
 * model the catalogue does not publish.
 */
const sharpenOperation = (entry: FamilyEntry | undefined, sharpen: NonNullable<OpenRequest["sharpen"]>) => {
    const variant = entry?.variants.find((published) => published.codename === sharpen.model);
    const precision = variant && (variant.precisions[savedTier(entry)] ?? variant.precisions[0]);
    if (!variant || precision === undefined) return;

    const operation = newOperation("sharpen", entry, modelValue({ codename: variant.codename, precision }));

    return operation && sharpen.strength !== undefined
        ? withParameter(operation, STRENGTH, sharpen.strength)
        : operation;
};

/**
 * Gives each photograph of `admitted` that `request` names the Sharpen and the Lens Blur it asks for, each kept in the
 * catalogue's apply order. The Sharpen is {@link sharpenOperation}'s; the Lens Blur is the one the add menu would add,
 * at the request's f-number - or its blur amount read against the photograph's own lens, as the slider reads it - and
 * its focus point. One of either already on the photograph is replaced.
 *
 * A photograph opened whole: nothing has been cropped yet, so the lens is its own.
 *
 * **Then the batch pages through them** (`lib/batch.ts`): a batch named on the command line is a batch somebody wants
 * done, not one photograph at a time as they are paged to. A batch already under way takes them on.
 */
export const applyRequest = async (request: OpenRequest, admitted: readonly ImageRecord[]) => {
    const asked = new Set(request.paths);
    const files = admitted.filter((file) => asked.has(file.path));
    if (files.length === 0) return;

    const families = await catalogue();
    const order = applyOrder(families);
    const sharpen = request.sharpen && sharpenOperation(familyEntry(families, "sharpen"), request.sharpen);
    const blurEntry = familyEntry(families, "lens_blur");
    const storedBlur = useSettingsStore.getState().models.lens_blur;

    const place = (path: string, operation: Operation) => {
        const store = useEnhancementStore.getState();
        const present = store.enhancements.get(path)?.some((existing) => existing.family === operation.family);

        if (present) store.replaceEnhancement(path, operation);
        else store.addEnhancement(path, operation, order);
    };

    for (const file of files) {
        if (sharpen) place(file.path, sharpen);
        if (!request.lensBlur) continue;

        const { fNumber, amount, focusX, focusY } = request.lensBlur;
        const own = fNumber ?? fNumberForAmount(amount ?? DEFAULT_AMOUNT, (await ownLens(file, undefined)).ownFNumber);
        let blur = newOperation("lens_blur", blurEntry, storedBlur, 1, own);
        if (!blur) continue;

        if (focusX !== undefined && focusY !== undefined) {
            blur = withParameter(withParameter(blur, FOCUS_X, focusX), FOCUS_Y, focusY);
        }
        place(file.path, blur);
    }

    // After the stacks, so the first look at them finds every one in place.
    startBatch(files.map((file) => file.path));
};
