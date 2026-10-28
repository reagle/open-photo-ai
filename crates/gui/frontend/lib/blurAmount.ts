import type { LensInfo } from "@/ipc/lens";

/**
 * The widest aperture a blur amount of 100 stands for: f/1.4 in full-frame-equivalent terms. Written a second time in
 * `crates/opai/src/models/lens_blur/lens.rs` (`AMOUNT_WIDEST_F_NUMBER`), where the command line's `--lb-amount` reads
 * it; the two mappings are pinned to the same numbers by a test on each side.
 */
export const WIDEST_F_NUMBER = 1.4;

/** The blur amount a freshly added lens blur starts at: half way between the photograph as taken and the widest. */
export const DEFAULT_AMOUNT = 50;

/** The range the library bounds an f-number to: `FNumber::MIN` and `FNumber::MAX`. */
const F_NUMBER_RANGE = { min: 1, max: 32 };

/**
 * The photograph's own f-number where nothing is known of it - no EXIF, or a question that could not be answered - as
 * the library assumes it: f/8 equivalent. Written a second time in `crates/opai/src/models/lens_blur/lens.rs`; the lens
 * the panel assumes and the lens a run assumes have to be one.
 */
export const ASSUMED_F_NUMBER = 8;

/**
 * What a photograph the lens of which could not be asked about is taken to be: the f/8 the library assumes for a
 * photograph with no lens information, so the panel and a run agree on it. No crop factor, so no real f-number.
 */
export const ASSUMED_LENS: LensInfo = {
    ownFNumber: ASSUMED_F_NUMBER,
    focal35mm: null,
    cropFactor: null,
    origin: "assumed",
};

/** `value` to `places` decimal places, as the number rather than the text. */
const rounded = (value: number, places: number) => {
    const scale = 10 ** places;

    return Math.round(value * scale) / scale;
};

/**
 * Whether a lens blur can add anything to a photograph taken at the equivalent f-number `own`: only where that is
 * wider than the widest a blur amount reaches. Compared in hundredths, which the library rounds an own f-number to, so
 * f/1.4 itself is not wider than f/1.4.
 */
export const canBlurMore = (own: number): boolean => rounded(own, 2) > WIDEST_F_NUMBER;

/**
 * The own f-number the amount is laid out from: the photograph's, held to the range the library takes an f-number in,
 * so a heavy crop of a small sensor (past f/32) has an "As taken" that can be written.
 */
const startOf = (own: number): number => Math.min(own, F_NUMBER_RANGE.max);

/**
 * The equivalent f-number a blur `amount` from 0 to 100 stands for on a photograph taken at the equivalent f-number
 * `own`: 0 is the photograph as taken - its own f-number, which adds nothing - and 100 is f/1.4.
 *
 * **Even in blur, not in stops.** A lens blurs by a disc whose diameter goes as `1 / N`, so the amount is laid out in
 * `1 / N` between the photograph's own and the widest: `1/N = 1/N0 + (a/100) (1/1.4 - 1/N0)`. Halfway is then half the
 * blur the widest aperture adds, which is what a slider is felt to do; an even walk through stops would spend most of
 * its length on apertures that blur little.
 *
 * Where the photograph is already as wide as the widest there is nothing to add, and this is its own f-number, which
 * adds nothing. Rounded to the thousandths the library keeps. The same arithmetic as `OwnLens::f_number_for_amount`,
 * which the command line uses.
 */
export const fNumberForAmount = (amount: number, own: number): number => {
    const start = startOf(own);
    const held = Number.isNaN(amount) ? 0 : Math.min(Math.max(amount, 0), 100);

    const fNumber = canBlurMore(own) ? 1 / (1 / start + (held / 100) * (1 / WIDEST_F_NUMBER - 1 / start)) : start;

    return Math.min(Math.max(rounded(fNumber, 3), F_NUMBER_RANGE.min), F_NUMBER_RANGE.max);
};

/**
 * The blur amount, 0 to 100 and not rounded, an f-number `fNumber` is on a photograph taken at `own`: the inverse of
 * {@link fNumberForAmount}. An f-number at or above the photograph's own is 0 - it adds nothing - and one wider than
 * f/1.4 is 100. A photograph that cannot be blurred more has no amount to speak of, which is 0.
 */
export const amountOfFNumber = (fNumber: number, own: number): number => {
    if (!canBlurMore(own) || !(fNumber > 0)) return 0;

    const start = startOf(own);
    const amount = (100 * (1 / fNumber - 1 / start)) / (1 / WIDEST_F_NUMBER - 1 / start);

    return Math.min(Math.max(amount, 0), 100);
};

/**
 * The amount a slider shows for `fNumber`: {@link amountOfFNumber}, a whole number. The slider writes whole amounts, so
 * what it wrote reads back as it was set, f-number quantisation notwithstanding.
 */
export const shownAmount = (fNumber: number, own: number): number => Math.round(amountOfFNumber(fNumber, own));

/**
 * The f-number the lens really has when it is opened to the equivalent `equivalent`: the equivalent divided by the
 * photograph's crop factor. `null` where the crop is not known, and no real aperture is claimed.
 */
export const realFNumber = (equivalent: number, cropFactor: number | null): number | null =>
    cropFactor !== null && cropFactor > 0 ? equivalent / cropFactor : null;

/**
 * An f-number as the readout beside the slider writes it, after the `f/`: a decimal below f/10 (`2.8`), a whole number
 * from it (`11`), as a lens barrel is marked, with no trailing zero. A figure that is approximate by its nature, so
 * there is no second decimal to read into it.
 */
export const readoutFNumber = (fNumber: number): string =>
    fNumber < 10 ? String(Number.parseFloat(fNumber.toFixed(1))) : String(Math.round(fNumber));

/**
 * How an f-number reads after the `f/`: `1.4`, `8`, `5.91`, with the float noise a typed or computed value can carry
 * dropped.
 */
export const formatFNumber = (fNumber: number): string => String(Number.parseFloat(fNumber.toFixed(2)));
