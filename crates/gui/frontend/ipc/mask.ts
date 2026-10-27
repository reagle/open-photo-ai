import { call } from "./invoke";

/**
 * Rust command name. Must match the `#[tauri::command]` function in `crates/gui/src/mask.rs`; renames aren't
 * type-checked across the IPC boundary.
 */
const MASK_PREVIEW_COMMAND = "mask_preview";

/**
 * One brush stroke, in the wire's spelling: `opai::Stroke`, camelCase.
 *
 * Everything is a fraction of the **framed** photograph - the picture the preview draws and a run receives - so a
 * stroke means the same place at any size, and the same place in the preview as in an export.
 */
export type Stroke = {
    /** Whether the stroke takes weight away from what is painted so far, rather than adding to it. */
    erase: boolean;
    /** The brush's radius as a fraction of the photograph's longer side. Rust brings it inside `0.0005..=0.5`. */
    radius: number;
    /** `0` is a hard edge (anti-aliased only); `1` feathers from the middle out. Rust brings it inside `0..=1`. */
    softness: number;
    /** `x0, y0, x1, y1, ...`, each a fraction of the photograph's width or height. */
    points: number[];
};

/**
 * A mask over the framed photograph, in the wire's spelling: `opai::Mask` without the base, which never crosses it.
 *
 * Strokes are painted and erased in order. `inverted` turns the finished weight over. White - weight one - is where a
 * run's enhancements apply, and black is where the photograph is left as it was.
 */
export type Mask = {
    strokes: Stroke[];
    inverted: boolean;
};

/**
 * Draws the weight `mask` gives a `width` x `height` picture, as a PNG, for the preview's tint.
 *
 * Rust rasterises it, so the tint is the weight a run will blend with rather than a second reading of the strokes. The
 * picture is greyscale with the weight also in its alpha channel. Rust brings a picture larger than 2048 pixels on its
 * longer side down to that, in the same shape, so a caller stretches it to the box it draws over.
 *
 * Rejects with a serialized error (typed `unknown`, as `invoke` rejections carry no type) for a picture with no pixels.
 */
export const maskPreview = (mask: Mask, width: number, height: number) =>
    call<ArrayBuffer>(MASK_PREVIEW_COMMAND, { mask, width, height });
