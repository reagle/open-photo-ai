/**
 * The f-numbers a lens blur's slider stops at: the standard third-stop scale from f/1.0 to f/32, in the nominal values
 * a lens barrel is marked with (f/1.4 is 1.4, though the stop is 2^(1/2)).
 *
 * **Nominal on the wire**: the parameter the library receives is the stop's own number, and the slider's value is the
 * stop's index, so one step of the control is always one third of a stop. The range is `FNumber`'s, 1 to 32.
 */
export const F_STOPS: readonly number[] = [
    1.0, 1.1, 1.2, 1.4, 1.6, 1.8, 2.0, 2.2, 2.5, 2.8, 3.2, 3.5, 4.0, 4.5, 5.0, 5.6, 6.3, 7.1, 8.0, 9.0, 10, 11, 13, 14,
    16, 18, 20, 22, 25, 29, 32,
];

/**
 * The photograph's own f-number where nothing is known of it - no EXIF, or a question that could not be answered - as
 * the library assumes it: f/8 equivalent. Written a second time in `crates/opai/src/models/lens_blur/lens.rs`; the lens
 * the panel assumes and the lens a run assumes have to be one.
 */
export const ASSUMED_F_NUMBER = 8;

/**
 * How far below a stop a photograph's own f-number may be and still be taken as at it: the library rounds the own
 * f-number to a hundredth, so a stop a hundredth short of it is the same aperture.
 */
const SAME = 0.005;

/**
 * The stops a photograph taken at the equivalent f-number `own` can be opened up from: f/1.0 up to the first stop at or
 * above `own`, which is the slider's right end - "As taken".
 *
 * **Rounded up, not to the nearest stop.** The library adds no blur at or above the photograph's own f-number and a
 * good deal below it (a third of a stop under it is already a third of the full disc), so a right end that rounded down
 * would start a new lens blur already blurring. Rounding up makes the right end add nothing, at the cost of reading up
 * to a third of a stop wider than the photograph's own f-number. A photograph above f/32 ends at f/32.
 */
export const stopsUpTo = (own: number): readonly number[] => {
    const end = F_STOPS.findIndex((stop) => stop >= own - SAME);

    return end === -1 ? F_STOPS : F_STOPS.slice(0, end + 1);
};

/**
 * The index in `stops` of the stop nearest `fNumber`, by ratio (a stop is a ratio, not a difference) - the last where
 * `fNumber` is beyond it, which is how an f-number above the photograph's own is shown: at "As taken", where it adds
 * the same nothing.
 */
export const nearestStop = (stops: readonly number[], fNumber: number): number => {
    let nearest = 0;

    stops.forEach((stop, index) => {
        const best = stops[nearest] ?? stop;
        if (Math.abs(Math.log(stop / fNumber)) < Math.abs(Math.log(best / fNumber))) nearest = index;
    });

    return nearest;
};

/**
 * How an f-number reads after the `f/`: `1.4`, `8`, with the float noise a typed or computed value can carry dropped.
 */
export const formatFNumber = (fNumber: number): string => String(Number.parseFloat(fNumber.toFixed(2)));
