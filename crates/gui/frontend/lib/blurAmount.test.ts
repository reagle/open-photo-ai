import { describe, expect, it } from "vitest";
import {
    amountOfFNumber,
    canBlurMore,
    fNumberForAmount,
    formatFNumber,
    readoutFNumber,
    realFNumber,
    shownAmount,
} from "./blurAmount";

describe("the f-number a blur amount stands for", () => {
    it("is the photograph's own at 0, which adds nothing, and f/0.95 at 100", () => {
        for (const own of [2.8, 5.91, 6, 8, 8.38, 12]) {
            expect(fNumberForAmount(0, own), `${own}`).toBe(own);
            expect(fNumberForAmount(100, own), `${own}`).toBe(0.95);
        }
    });

    it("is the same numbers the command line's --lb-amount reads, from the library's own test", () => {
        // `a_blur_amount_runs_from_the_photograph_as_taken_to_f1_4_evenly_in_blur` in lens.rs pins the first row.
        expect(fNumberForAmount(50, 6)).toBe(1.64);
        expect(fNumberForAmount(25, 6)).toBe(2.576);
        expect(fNumberForAmount(50, 5.91)).toBe(1.637);
        expect(fNumberForAmount(50, 8)).toBe(1.698);
        expect(fNumberForAmount(50, 12)).toBe(1.761);
        expect(fNumberForAmount(1, 2.8)).toBe(2.747);
    });

    it("grows the blur evenly, one over the f-number being the blur", () => {
        for (const own of [2.5, 6, 12, 32]) {
            const disc = (amount: number) => 1 / fNumberForAmount(amount, own);
            const tenth = disc(10) - disc(0);

            for (const amount of [20, 50, 80, 100]) {
                const walked = disc(amount) - disc(0);

                expect(walked / (tenth * (amount / 10)), `${own} at ${amount}`).toBeCloseTo(1, 2);
            }
        }
    });

    it("never goes the wrong way: a larger amount is a wider aperture", () => {
        let last = Number.POSITIVE_INFINITY;

        for (let amount = 0; amount <= 100; amount++) {
            const fNumber = fNumberForAmount(amount, 8);

            expect(fNumber).toBeLessThanOrEqual(last);
            last = fNumber;
        }
    });

    it("holds an amount outside 0 to 100, or not a number, to the ends", () => {
        expect(fNumberForAmount(140, 6)).toBe(0.95);
        expect(fNumberForAmount(-5, 6)).toBe(6);
        expect(fNumberForAmount(Number.NaN, 6)).toBe(6);
    });

    it("is the photograph's own where it is already as wide as the widest, whatever the amount", () => {
        expect(canBlurMore(0.95)).toBe(false);
        expect(fNumberForAmount(100, 0.95)).toBe(0.95);
        expect(fNumberForAmount(50, 0.95)).toBe(0.95);
        // A 4×5 camera's f/0.7 equivalent is wider than any amount goes.
        expect(canBlurMore(0.7)).toBe(false);

        expect(canBlurMore(0.96)).toBe(true);
        // Held in hundredths, which the library rounds an own f-number to: noise above f/0.95 is f/0.95.
        expect(canBlurMore(0.9500001)).toBe(false);
        // A photograph at f/1.4, as wide as a fast prime goes, still has the Noctilux's half stop to give.
        expect(canBlurMore(1.4)).toBe(true);
        expect(fNumberForAmount(100, 1.4)).toBe(0.95);
    });

    it("holds a heavy crop's own f-number past f/32 to the range, so its 0 can be written", () => {
        expect(fNumberForAmount(0, 48)).toBe(32);
        expect(fNumberForAmount(50, 48)).toBe(1.845);
        expect(amountOfFNumber(32, 48)).toBe(0);
    });
});

describe("the blur amount an f-number is", () => {
    it("reads back what fNumberForAmount wrote, to a whole amount", () => {
        for (const own of [2.8, 5.91, 6, 8, 8.38, 12]) {
            for (let amount = 0; amount <= 100; amount++) {
                expect(shownAmount(fNumberForAmount(amount, own), own), `${own} at ${amount}`).toBe(amount);
            }
        }
    });

    it("is 0 at or above the photograph's own, and 100 at or below f/0.95", () => {
        expect(amountOfFNumber(6, 6)).toBe(0);
        expect(amountOfFNumber(20, 6)).toBe(0);
        expect(amountOfFNumber(0.95, 6)).toBe(100);
        expect(amountOfFNumber(0.9, 6)).toBe(100);
    });

    it("is 0 for a photograph that cannot be blurred more, and for nonsense", () => {
        expect(amountOfFNumber(0.9, 0.9)).toBe(0);
        expect(amountOfFNumber(0, 6)).toBe(0);
        expect(amountOfFNumber(Number.NaN, 6)).toBe(0);
    });
});

describe("the real f-number", () => {
    it("is the equivalent over the crop factor", () => {
        expect(realFNumber(4.2, 1.5)).toBeCloseTo(2.8, 10);
        expect(realFNumber(12, 3)).toBe(4);
    });

    it("is not claimed where the crop is not known", () => {
        expect(realFNumber(4.2, null)).toBeNull();
        expect(realFNumber(4.2, 0)).toBeNull();
    });
});

describe("an f-number as the readout writes it", () => {
    it("has one decimal below f/10 and none from it, and no trailing zero", () => {
        expect(readoutFNumber(2.8)).toBe("2.8");
        expect(readoutFNumber(2.27)).toBe("2.3");
        expect(readoutFNumber(4)).toBe("4");
        expect(readoutFNumber(4.2)).toBe("4.2");
        expect(readoutFNumber(9.96)).toBe("10");
        expect(readoutFNumber(11.4)).toBe("11");
        expect(readoutFNumber(1.4)).toBe("1.4");
    });

    it("has two figures below f/1, so the Noctilux's f/0.95 is not an f/0.9", () => {
        expect(readoutFNumber(0.95)).toBe("0.95");
        expect(readoutFNumber(0.587)).toBe("0.59");
        expect(readoutFNumber(0.5)).toBe("0.5");
    });
});

describe("an f-number as it is written after f/", () => {
    it("has no trailing zero, and no float noise", () => {
        expect(formatFNumber(1.4)).toBe("1.4");
        expect(formatFNumber(8)).toBe("8");
        expect(formatFNumber(1)).toBe("1");
        expect(formatFNumber(5.999999999999999)).toBe("6");
        expect(formatFNumber(5.91)).toBe("5.91");
    });
});
