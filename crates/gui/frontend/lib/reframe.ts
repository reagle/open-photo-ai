import type { CropInfo } from "@/ipc/crop";
import type { ImageRecord } from "@/ipc/images";
import { amountOfFNumber, fNumberForAmount } from "@/lib/blurAmount";
import { F_NUMBER, FOCUS_X, FOCUS_Y, ownLens, withParameter } from "@/lib/enhancements";
import { clampTo } from "@/lib/utils";
import { useEnhancementStore } from "@/stores/enhancements";

/** A point as fractions across and down a picture, `0` to `1`. */
export type FocusPoint = { x: number; y: number };

type Size = { width: number; height: number };

/** The thousandths the library keeps a focus point at. */
const thousandths = (fraction: number) => Math.round(fraction * 1000) / 1000;

/**
 * The size of the canvas a turn of `radians` lays a picture of `size` on: the bounding box of the turned picture, which
 * is what the Crop/Rotate widget measures its rectangle in and `rust_sak::image::rotate` produces, rounded outward.
 */
const turnedSize = ({ width, height }: Size, radians: number): Size => {
    const cos = Math.abs(Math.cos(radians));
    const sin = Math.abs(Math.sin(radians));

    // The noise a float leaves (800 x cos(90 degrees) is not quite 0) is not a pixel.
    const outward = (value: number) => Math.ceil(value - 1e-6);

    return { width: outward(width * cos + height * sin), height: outward(width * sin + height * cos) };
};

/**
 * Where the point `frame` of a photograph as `crop` frames it (the whole of the `file` where absent) is in the file's
 * own pixels.
 *
 * The framing is flip, flip, turn and cut, in the turned picture's coordinates, so this undoes them in the opposite
 * order: out of the cut, back through the turn about the centre of the canvas it lands on, and back through the flips.
 */
const inFile = (frame: FocusPoint, crop: CropInfo | undefined, file: Size): FocusPoint => {
    if (!crop) return { x: frame.x * file.width, y: frame.y * file.height };

    const radians = (crop.millidegrees / 1000) * (Math.PI / 180);
    const canvas = turnedSize(file, radians);
    const dx = crop.left + frame.x * crop.width - canvas.width / 2;
    const dy = crop.top + frame.y * crop.height - canvas.height / 2;
    const cos = Math.cos(radians);
    const sin = Math.sin(radians);
    const x = cos * dx + sin * dy + file.width / 2;
    const y = -sin * dx + cos * dy + file.height / 2;

    return { x: crop.flipHorizontal ? file.width - x : x, y: crop.flipVertical ? file.height - y : y };
};

/** The inverse of {@link inFile}: where the file's pixel `point` falls in the photograph as `crop` frames it. */
const inFrame = (point: FocusPoint, crop: CropInfo | undefined, file: Size): FocusPoint => {
    if (!crop) return { x: point.x / file.width, y: point.y / file.height };

    const radians = (crop.millidegrees / 1000) * (Math.PI / 180);
    const canvas = turnedSize(file, radians);
    const dx = (crop.flipHorizontal ? file.width - point.x : point.x) - file.width / 2;
    const dy = (crop.flipVertical ? file.height - point.y : point.y) - file.height / 2;
    const cos = Math.cos(radians);
    const sin = Math.sin(radians);

    return {
        x: (cos * dx - sin * dy + canvas.width / 2 - crop.left) / crop.width,
        y: (sin * dx + cos * dy + canvas.height / 2 - crop.top) / crop.height,
    };
};

/**
 * Where the focus point `focus` of a photograph framed as `from` is once it is framed as `to`: the same place on the
 * subject, as fractions of the new frame.
 *
 * **A focus point is a fraction of the frame, so a frame that changes moves what it points at**; this carries it
 * through the file's own pixels, undoing the old flips, turn and cut and applying the new ones. **A point the new frame
 * leaves out is clamped to its nearest edge**, not sent to the middle: the subject was off to that side, and the edge
 * nearest it is the closest the new frame can stand to it. Rounded to the thousandths the library keeps.
 *
 * A frame the file's size is needed for and not known - a flip or a turn of a file whose header could not be read - is
 * not mapped, and the point is returned as it was.
 */
export const mapFocus = (
    focus: FocusPoint,
    from: CropInfo | undefined,
    to: CropInfo | undefined,
    file: { width?: number; height?: number } | undefined,
): FocusPoint => {
    const { width, height } = file ?? {};
    if (width === undefined || height === undefined || width <= 0 || height <= 0) return focus;

    const size = { width, height };
    const moved = inFrame(inFile(focus, from, size), to, size);

    return { x: thousandths(clampTo(moved.x, 0, 1)), y: thousandths(clampTo(moved.y, 0, 1)) };
};

/** How near two own f-numbers are the same: the library rounds one to a hundredth. */
const SAME_OWN = 0.005;

/**
 * The f-number a lens blur set to `stored` is moved to when the photograph's framing changes from one whose own
 * equivalent f-number is `oldOwn` to one whose is `newOwn`: the same blur amount, against the new own aperture.
 *
 * **The f-number is an absolute equivalent value, not a place on the slider**, and a crop is a smaller sensor behind the
 * same lens, so its own f-number is wider-numbered; a value left alone would come to mean something else (a blur set
 * to f/2.8 of a whole frame is far too strong over a crop whose own is f/12). What the user chose is the amount, so the
 * amount is kept and the f-number is worked out again: a lens blur at "As taken" stays at "As taken" and a blur at 70
 * stays at 70, held inside the new range, so a crop undone does not leave a value past the end.
 *
 * The amount is the whole number the slider shows, which is all it ever writes, so a framing changed back and forth
 * does not walk the f-number about by the thousandths it is quantised to.
 */
export const fNumberAfterCrop = (stored: number, oldOwn: number, newOwn: number): number =>
    // A framing that leaves the own f-number where it was (a flip, a turn) leaves the value alone, whatever it is.
    Math.abs(newOwn - oldOwn) < SAME_OWN
        ? stored
        : fNumberForAmount(Math.round(amountOfFNumber(stored, oldOwn)), newOwn);

/**
 * Carries a lens blur across a change of framing, for the photograph `file` whose framing was `from` and is now `to`.
 *
 * Both of the lens blur's values are the photograph's rather than the user's alone - its f-number is an absolute
 * equivalent value, kept as the same amount ({@link fNumberAfterCrop}), and its focus a fraction of a frame
 * ({@link mapFocus}) - so cropping a photograph that has one would otherwise silently blur it, or move its focus onto
 * another subject. A file with no lens
 * blur, or no identity to ask the lens of, is left alone.
 *
 * Asked after the framing is written, so a run starts over the new framing with the old values for the moment the lens
 * is on its way; the run the corrected lens blur starts replaces it. Read from the store when the answer arrives, not
 * before, so an edit made meanwhile is not overwritten with an older stack's.
 */
export const reframeLensBlur = async (
    file: ImageRecord | undefined,
    from: CropInfo | undefined,
    to: CropInfo | undefined,
): Promise<void> => {
    if (!file || file.identity === undefined) return;

    const { path } = file;
    const present = () =>
        useEnhancementStore
            .getState()
            .enhancements.get(path)
            ?.find((o) => o.family === "lens_blur");
    if (!present()) return;

    const [oldLens, newLens] = await Promise.all([ownLens(file, from), ownLens(file, to)]);

    const lensBlur = present();
    if (!lensBlur) return;

    let next = lensBlur;
    const stored = lensBlur.parameters[F_NUMBER];

    if (stored !== undefined) {
        const fNumber = fNumberAfterCrop(stored, oldLens.ownFNumber, newLens.ownFNumber);
        if (fNumber !== stored) next = withParameter(next, F_NUMBER, fNumber);
    }

    // The middle where the operation carries none, which is what the preview draws and a run reads.
    const focus = { x: lensBlur.parameters[FOCUS_X] ?? 0.5, y: lensBlur.parameters[FOCUS_Y] ?? 0.5 };
    const moved = mapFocus(focus, from, to, file);

    if (moved.x !== focus.x || moved.y !== focus.y) {
        next = withParameter(withParameter(next, FOCUS_X, moved.x), FOCUS_Y, moved.y);
    }

    if (next !== lensBlur) useEnhancementStore.getState().replaceEnhancement(path, next);
};
