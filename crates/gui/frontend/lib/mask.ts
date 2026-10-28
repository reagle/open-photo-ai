import type { Operation } from "@/ipc/enhance";
import type { Mask, Stroke } from "@/ipc/mask";
import { isPainted } from "@/lib/enhancements";
import { clampTo } from "@/lib/utils";

/**
 * What a mask is, on this side of the boundary: the strokes painted so far and whether the weight is turned over.
 *
 * Nothing in it is a pixel. The strokes are fractions of the framed photograph, so the same value is the right mask at
 * any size - the preview's, the export's, and the picture Rust draws for the tint.
 */
export type Snapshot = { strokes: Stroke[]; inverted: boolean };

/** How many steps of undo a photograph's mask keeps. The oldest is dropped past this. */
export const HISTORY_LIMIT = 100;

/**
 * The brush's size is its **diameter**, as a fraction of the photograph's longer side, and Rust takes the radius: half
 * of it. These are the slider's ends and where the brush starts.
 */
export const SIZE_MIN = 0.002;
export const SIZE_MAX = 0.25;
export const SIZE_DEFAULT = 0.05;

/** How soft the brush's edge starts: half its radius is the hard core. */
export const SOFTNESS_DEFAULT = 0.5;

/** How far one press of a size key scales the brush, and how far one press of a softness key moves the edge. */
const SIZE_STEP = 1.25;
const SOFTNESS_STEP = 0.1;

/**
 * The slider's range. The size is on a logarithmic scale, so that a hair's width and a quarter of the frame are both
 * within reach.
 */
export const SLIDER_MAX = 100;

/**
 * How close, in brush radii, a point may come to the last one kept before it is dropped: `0.1 r`.
 *
 * A pointer reports a position far more often than a stroke needs one. At a tenth of the radius the polyline is smooth
 * to the eye and a long stroke is a few hundred points rather than a few thousand.
 */
export const POINT_SPACING = 0.1;

/** The radius Rust takes for a brush of this size. */
export const radiusOf = (size: number) => size / 2;

/** Where on the slider a brush of `size` sits, from `0` to {@link SLIDER_MAX}, on a logarithmic scale. */
export const sizeToSlider = (size: number) =>
    (Math.log(clampTo(size, SIZE_MIN, SIZE_MAX) / SIZE_MIN) / Math.log(SIZE_MAX / SIZE_MIN)) * SLIDER_MAX;

/** The size at `position` on the slider: the inverse of {@link sizeToSlider}. */
export const sliderToSize = (position: number) =>
    SIZE_MIN * (SIZE_MAX / SIZE_MIN) ** (clampTo(position, 0, SLIDER_MAX) / SLIDER_MAX);

/** `size` one step larger (`1`) or smaller (`-1`), inside the brush's range. */
export const stepSize = (size: number, direction: 1 | -1) => clampTo(size * SIZE_STEP ** direction, SIZE_MIN, SIZE_MAX);

/** `softness` one step softer (`1`) or harder (`-1`), inside `0..1`, with no float noise left on it. */
export const stepSoftness = (softness: number, direction: 1 | -1) =>
    clampTo(Math.round((softness + direction * SOFTNESS_STEP) * 100) / 100, 0, 1);

/**
 * Whether a pointer at (`x`, `y`) - fractions of the framed photograph - is far enough from the last point of
 * `points` to be kept, for a brush of `radius` (a fraction of the longer side).
 *
 * The distance is measured in the photograph's own proportions, not in fractions: a tenth of the way across a wide
 * photograph is farther than a tenth of the way down it. `aspect` is its width over its height, or `0` where that is
 * not known, which keeps every point.
 */
export const keepsPoint = (
    points: readonly number[],
    x: number,
    y: number,
    radius: number,
    aspect: number,
): boolean => {
    const lastX = points[points.length - 2];
    const lastY = points[points.length - 1];
    if (lastX === undefined || lastY === undefined || aspect <= 0) return true;

    // Against the longer side, which is what a radius is a fraction of: a wide photograph's width is 1 and its height
    // `1 / aspect`; a tall one's height is 1 and its width `aspect`.
    const [across, down] = aspect >= 1 ? [1, 1 / aspect] : [aspect, 1];

    return Math.hypot((x - lastX) * across, (y - lastY) * down) >= POINT_SPACING * radius;
};

/** Whether `snapshot` paints nothing: no strokes, whether or not it is turned over. */
export const isEmpty = (snapshot: Snapshot | undefined): boolean => !snapshot || snapshot.strokes.length === 0;

/**
 * What a point's coordinate is rounded to on the wire: a hundred-thousandth, far finer than any pixel and short enough
 * to keep a payload small.
 */
const COORDINATE_STEP = 1e5;

/**
 * `snapshot` as the mask a run is sent, or `undefined` for one that paints nothing.
 *
 * An empty mask is no mask, and Rust reads it so as well: a photograph the user has not painted - or has undone back to
 * nothing - runs and is identified exactly as it did before there was a brush.
 *
 * **The points are rounded to a hundred-thousandth.** A pointer reports positions to the float's last digit, and a long
 * stroke is a few hundred of them: sent as they are, each is eighteen characters of JSON for a place no screen can tell
 * from its neighbour.
 */
export const toWire = (snapshot: Snapshot | undefined): Mask | undefined =>
    snapshot && !isEmpty(snapshot)
        ? {
              strokes: snapshot.strokes.map((stroke) => ({
                  ...stroke,
                  points: stroke.points.map((point) => Math.round(point * COORDINATE_STEP) / COORDINATE_STEP),
              })),
              inverted: snapshot.inverted,
          }
        : undefined;

/**
 * What a removal's region is on the wire: always a mask, with nothing in it for a region nothing was painted in.
 *
 * Unlike {@link toWire}'s, sent even when empty - Rust leaves a removal of nothing out of the chain, so no model is
 * opened for it - and never inverted: a removal turned over would take out all but what was painted. The coordinates
 * are rounded as {@link toWire} rounds them.
 */
export const regionOf = (snapshot: Snapshot | undefined): Mask => ({
    strokes: toWire(snapshot && { strokes: snapshot.strokes, inverted: false })?.strokes ?? [],
    inverted: false,
});

/**
 * `operations` with `region` as the region of each one painted with the brush - Remove - and every other one as it
 * was: the removal's strokes put in on the way to `enhance` or `export`, as `withChoice` puts in the choice among
 * faces. Which operations take a region is `ENHANCEMENTS`' business (`isPainted`); Rust hands it to the parameter the
 * model publishes as a mask.
 */
export const withRemoval = (operations: readonly Operation[], region: Mask): Operation[] =>
    operations.map((operation) => (isPainted(operation.family) ? { ...operation, mask: region } : operation));

/**
 * The strokes `now` has beyond `base`, where `base` is `now` as it was a few strokes ago: the same strokes, the same
 * way up, and fewer. `undefined` where it is not - an undo, a clear or an invert has nothing to add to what `base`
 * shows.
 *
 * Strokes are compared by identity, which is what the store keeps: a stroke is never edited once committed.
 */
export const strokesAfter = (base: Snapshot | undefined, now: Snapshot | undefined): Stroke[] | undefined => {
    const before = base?.strokes ?? [];
    if (!now || now.inverted !== (base?.inverted ?? false) || now.strokes.length < before.length) return;
    if (!before.every((stroke, index) => stroke === now.strokes[index])) return;

    return now.strokes.slice(before.length);
};

/**
 * `bytes` as a `data:` URL for a PNG: what a CSS mask reads, in a window whose policy admits `data:` images and not
 * `blob:`.
 */
export const pngDataUrl = (bytes: ArrayBuffer): string => {
    const view = new Uint8Array(bytes);
    let binary = "";

    // In chunks: `String.fromCharCode` takes its arguments on the stack, and a few hundred kilobytes is too many for
    // it.
    for (let at = 0; at < view.length; at += 0x8000) {
        binary += String.fromCharCode(...view.subarray(at, at + 0x8000));
    }

    return `data:image/png;base64,${btoa(binary)}`;
};

/** The longest edge, in pixels, of the picture the overlay asks Rust for. Rust caps it there as well. */
export const PREVIEW_EDGE_MAX = 2048;

/**
 * The sizes the overlay's picture is asked for in steps of, so that a wheel zoom does not ask for a new one at every
 * notch.
 */
const PREVIEW_EDGE_STEP = 256;

/**
 * The size of the picture to ask Rust for, for a photograph drawn `width` x `height` CSS pixels on a display of
 * `density` device pixels to each: the drawn box in device pixels, rounded up to a step and capped.
 *
 * `undefined` for a box that has not been measured.
 */
export const previewSize = (width: number, height: number, density: number) => {
    const longer = Math.max(width, height);
    if (!(width > 0 && height > 0)) return;

    const edge = Math.min(PREVIEW_EDGE_MAX, Math.ceil((longer * density) / PREVIEW_EDGE_STEP) * PREVIEW_EDGE_STEP);
    const scale = edge / longer;

    return { width: Math.max(1, Math.round(width * scale)), height: Math.max(1, Math.round(height * scale)) };
};

/** Whether this is a Mac, where the shortcut modifier is Command and the erase modifier is called Option. */
export const onMac = () => typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

/** The key a shortcut is spelled with, untranslated: `⌘Z` on a Mac and `Ctrl+Z` elsewhere. */
export const withModifier = (key: string, shift = false) =>
    onMac() ? `${shift ? "⇧" : ""}⌘${key}` : `Ctrl+${shift ? "Shift+" : ""}${key}`;
