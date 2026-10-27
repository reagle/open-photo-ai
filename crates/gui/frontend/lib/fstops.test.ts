import { describe, expect, it } from "vitest";
import { F_STOPS, formatFNumber, nearestStop, stopsUpTo } from "./fstops";

describe("the stops", () => {
    it("are the third-stop scale from f/1.0 to f/32", () => {
        expect(F_STOPS[0]).toBe(1);
        expect(F_STOPS.at(-1)).toBe(32);
        expect(F_STOPS).toHaveLength(31);
        expect([...F_STOPS].sort((a, b) => a - b)).toEqual([...F_STOPS]);
    });

    it("are each about one third of a stop from the last, to the nominal rounding", () => {
        // A third of a stop is a factor of 2^(1/6), 1.12, in the f-number; the marked values are that rounded as
        // lenses round them, which is 7 percent either way at the coarsest (f/11 to f/13 and f/13 to f/14).
        for (let index = 1; index < F_STOPS.length; index++) {
            const ratio = (F_STOPS[index] ?? 0) / (F_STOPS[index - 1] ?? 1);

            expect(ratio, `${F_STOPS[index]}`).toBeGreaterThan(1.07);
            expect(ratio, `${F_STOPS[index]}`).toBeLessThan(1.19);
        }
    });
});

describe("the stops up to a photograph's own f-number", () => {
    it("end at the first stop at or above it, so the right end adds no blur", () => {
        // The three test photographs' equivalents: f/6, f/5.91 and f/8.38.
        expect(stopsUpTo(6).at(-1)).toBe(6.3);
        expect(stopsUpTo(5.91).at(-1)).toBe(6.3);
        expect(stopsUpTo(8.38).at(-1)).toBe(9);
    });

    it("end at a stop that is the photograph's own f-number exactly", () => {
        expect(stopsUpTo(8).at(-1)).toBe(8);
        expect(stopsUpTo(2.8).at(-1)).toBe(2.8);
        // A hundredth short is the same aperture.
        expect(stopsUpTo(7.995).at(-1)).toBe(8);
    });

    it("start at f/1.0 whatever the photograph's own", () => {
        expect(stopsUpTo(6)[0]).toBe(1);
        expect(stopsUpTo(6)).toHaveLength(17);
    });

    it("hold only f/1.0 for a photograph at or wider than it, and stop at f/32 for one beyond it", () => {
        expect(stopsUpTo(0.9)).toEqual([1]);
        expect(stopsUpTo(1)).toEqual([1]);
        expect(stopsUpTo(45).at(-1)).toBe(32);
        expect(stopsUpTo(45)).toHaveLength(F_STOPS.length);
    });
});

describe("the stop nearest an f-number", () => {
    const stops = stopsUpTo(6);

    it("is the stop itself where the f-number is one", () => {
        expect(stops[nearestStop(stops, 1.4)]).toBe(1.4);
        expect(stops[nearestStop(stops, 2.8)]).toBe(2.8);
    });

    it("is the nearer neighbour between two, by ratio", () => {
        expect(stops[nearestStop(stops, 2.7)]).toBe(2.8);
        expect(stops[nearestStop(stops, 1.45)]).toBe(1.4);
    });

    it("is the last stop for an f-number beyond the photograph's own, and the first for one below f/1.0", () => {
        expect(nearestStop(stops, 20)).toBe(stops.length - 1);
        expect(nearestStop(stops, 0.5)).toBe(0);
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
