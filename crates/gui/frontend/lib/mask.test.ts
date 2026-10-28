import { describe, expect, it } from "vitest";
import type { Operation } from "@/ipc/enhance";
import {
    isEmpty,
    isUsedFor,
    keepsPoint,
    PREVIEW_EDGE_MAX,
    pngDataUrl,
    previewSize,
    radiusOf,
    regionOf,
    SIZE_MAX,
    SIZE_MIN,
    sizeToSlider,
    sliderToSize,
    stepSize,
    stepSoftness,
    strokesAfter,
    toWire,
    withRemoval,
} from "./mask";

const STROKE = { erase: false, radius: 0.025, softness: 0.5, points: [0.1, 0.2] };

describe("the brush's size", () => {
    it("is the diameter, and Rust is given half of it", () => {
        expect(radiusOf(0.05)).toBe(0.025);
    });

    it("runs from 0.2 to 25 percent of the longer side on a logarithmic slider", () => {
        expect(sizeToSlider(SIZE_MIN)).toBeCloseTo(0);
        expect(sizeToSlider(SIZE_MAX)).toBeCloseTo(100);
        expect(sliderToSize(0)).toBeCloseTo(SIZE_MIN);
        expect(sliderToSize(100)).toBeCloseTo(SIZE_MAX);

        // The middle of a logarithmic range is the geometric mean, not the arithmetic one.
        expect(sliderToSize(50)).toBeCloseTo(Math.sqrt(SIZE_MIN * SIZE_MAX));
    });

    it("comes back from the slider as the size it went in as", () => {
        for (const size of [0.003, 0.02, 0.05, 0.2]) expect(sliderToSize(sizeToSlider(size))).toBeCloseTo(size, 10);
    });

    it("steps by a quarter either way and stops at the ends", () => {
        expect(stepSize(0.04, 1)).toBeCloseTo(0.05);
        expect(stepSize(0.05, -1)).toBeCloseTo(0.04);
        expect(stepSize(SIZE_MAX, 1)).toBe(SIZE_MAX);
        expect(stepSize(SIZE_MIN, -1)).toBe(SIZE_MIN);
    });
});

describe("the brush's softness", () => {
    it("steps by a tenth with no float noise, and stops at both ends", () => {
        expect(stepSoftness(0.5, 1)).toBe(0.6);
        expect(stepSoftness(0.7, -1)).toBe(0.6);
        expect(stepSoftness(0.95, 1)).toBe(1);
        expect(stepSoftness(0.05, -1)).toBe(0);
        expect(stepSoftness(0.3, 1) * 10).toBe(4);
    });
});

describe("keepsPoint", () => {
    // A square photograph and a radius of 0.05 of its side: a point is dropped inside 0.005 of the last one.
    it("keeps the first point of a stroke", () => {
        expect(keepsPoint([], 0.5, 0.5, 0.05, 1)).toBe(true);
    });

    it("drops a point closer than a tenth of the radius, and keeps one that is not", () => {
        expect(keepsPoint([0.5, 0.5], 0.503, 0.5, 0.05, 1)).toBe(false);
        expect(keepsPoint([0.5, 0.5], 0.506, 0.5, 0.05, 1)).toBe(true);
    });

    it("measures in the photograph's own proportions", () => {
        // 2:1. A step of 0.004 along the width is 0.004 of the longer side; the same step down the height is half that.
        expect(keepsPoint([0.5, 0.5], 0.504, 0.5, 0.05, 2)).toBe(false);
        expect(keepsPoint([0.5, 0.5], 0.5, 0.508, 0.05, 2)).toBe(false);
        expect(keepsPoint([0.5, 0.5], 0.5, 0.52, 0.05, 2)).toBe(true);
    });

    it("keeps every point where the photograph's shape is not known", () => {
        expect(keepsPoint([0.5, 0.5], 0.5, 0.5, 0.05, 0)).toBe(true);
    });
});

describe("emptiness and the wire", () => {
    it("is empty with no strokes, whether or not it is turned over", () => {
        expect(isEmpty(undefined)).toBe(true);
        expect(isEmpty({ strokes: [], inverted: true })).toBe(true);
        expect(isEmpty({ strokes: [STROKE], inverted: false })).toBe(false);
    });

    it("sends nothing for an empty mask and the strokes and inversion for one that paints", () => {
        expect(toWire(undefined)).toBeUndefined();
        expect(toWire({ strokes: [], inverted: true })).toBeUndefined();
        expect(toWire({ strokes: [STROKE], inverted: true })).toEqual({ strokes: [STROKE], inverted: true });
    });

    it("sends a use only switched off, and a mask that paints nothing not at all whatever its uses", () => {
        expect(toWire({ strokes: [STROKE], inverted: false, limitsEnhancements: true, keepsSharp: true })).toEqual({
            strokes: [STROKE],
            inverted: false,
        });
        expect(toWire({ strokes: [STROKE], inverted: false, keepsSharp: false })).toEqual({
            strokes: [STROKE],
            inverted: false,
            keepsSharp: false,
        });
        expect(toWire({ strokes: [STROKE], inverted: false, limitsEnhancements: false })).toEqual({
            strokes: [STROKE],
            inverted: false,
            limitsEnhancements: false,
        });
        expect(toWire({ strokes: [], inverted: false, keepsSharp: false })).toBeUndefined();
        expect(isUsedFor(undefined, "keepsSharp")).toBe(true);
        expect(isUsedFor({ keepsSharp: false }, "keepsSharp")).toBe(false);
    });

    it("rounds the points to a hundred-thousandth and nothing else", () => {
        const wire = toWire({
            strokes: [{ ...STROKE, radius: 0.0250049, points: [0.123456789, 0.333333333333, 1, 0] }],
            inverted: false,
        });

        expect(wire?.strokes[0]?.points).toEqual([0.12346, 0.33333, 1, 0]);
        expect(wire?.strokes[0]?.radius).toBe(0.0250049);
    });

    it("does not touch the strokes it was given", () => {
        const stroke = { ...STROKE, points: [0.123456789, 0.5] };

        toWire({ strokes: [stroke], inverted: false });

        expect(stroke.points).toEqual([0.123456789, 0.5]);
    });
});

describe("a removal's region on the wire", () => {
    it("is sent even when nothing is painted, as a mask with no strokes", () => {
        // Rust leaves a removal of nothing out of the chain; sending none would be a second way of saying it.
        expect(regionOf(undefined)).toEqual({ strokes: [], inverted: false });
        expect(regionOf({ strokes: [], inverted: false })).toEqual({ strokes: [], inverted: false });
    });

    it("carries the strokes rounded as the limiting mask's are, and is never turned over", () => {
        const painted = { strokes: [{ ...STROKE, points: [0.1234567, 0.7654321] }], inverted: true };

        expect(regionOf(painted)).toEqual({
            strokes: [{ ...STROKE, points: [0.12346, 0.76543] }],
            inverted: false,
        });
    });

    it("is put on the Remove of a chain and on nothing else", () => {
        const remove: Operation = { family: "remove", codename: "lima", precision: "fp32", parameters: {} };
        const upscale: Operation = {
            family: "upscale",
            codename: "kyoto",
            precision: "fp32",
            parameters: { scale: 2 },
        };
        const region = regionOf({ strokes: [STROKE], inverted: false });

        expect(withRemoval([remove, upscale], region)).toEqual([{ ...remove, mask: region }, upscale]);
        expect(withRemoval([upscale], region)).toEqual([upscale]);
    });
});

describe("what a picture on show lacks", () => {
    const a = { ...STROKE, points: [0.1, 0.1] };
    const b = { ...STROKE, points: [0.2, 0.2] };
    const c = { ...STROKE, points: [0.3, 0.3] };

    it("is the strokes on the end of the same mask, in order", () => {
        expect(strokesAfter({ strokes: [a], inverted: false }, { strokes: [a, b, c], inverted: false })).toEqual([
            b,
            c,
        ]);
        expect(strokesAfter({ strokes: [a], inverted: false }, { strokes: [a], inverted: false })).toEqual([]);
        expect(strokesAfter(undefined, { strokes: [a], inverted: false })).toEqual([a]);
    });

    it("is nothing to draw where the mask was undone, turned over or changed underneath", () => {
        expect(strokesAfter({ strokes: [a, b], inverted: false }, { strokes: [a], inverted: false })).toBeUndefined();
        expect(strokesAfter({ strokes: [a], inverted: false }, { strokes: [a, b], inverted: true })).toBeUndefined();
        expect(
            strokesAfter({ strokes: [a, b], inverted: false }, { strokes: [a, c], inverted: false }),
        ).toBeUndefined();
        expect(strokesAfter(undefined, { strokes: [a], inverted: true })).toBeUndefined();
        expect(strokesAfter({ strokes: [a], inverted: false }, undefined)).toBeUndefined();
    });
});

describe("the overlay's picture", () => {
    it("asks for the drawn box in device pixels, rounded up to a step", () => {
        expect(previewSize(800, 600, 2)).toEqual({ width: 1792, height: 1344 });
        expect(previewSize(800, 600, 1)).toEqual({ width: 1024, height: 768 });
    });

    it("never asks for more than Rust draws", () => {
        const size = previewSize(7200, 4800, 2);

        expect(Math.max(size?.width ?? 0, size?.height ?? 0)).toBe(PREVIEW_EDGE_MAX);
    });

    it("asks for nothing of a box that has not been measured", () => {
        expect(previewSize(0, 0, 2)).toBeUndefined();
    });

    it("keeps a size across the notches of a wheel zoom", () => {
        expect(previewSize(800, 600, 2)).toEqual(previewSize(840, 630, 2));
    });
});

describe("pngDataUrl", () => {
    it("spells the bytes as a base64 PNG data URL", () => {
        expect(pngDataUrl(new Uint8Array([137, 80, 78, 71]).buffer)).toBe("data:image/png;base64,iVBORw==");
    });

    it("takes a picture larger than one chunk", () => {
        const url = pngDataUrl(new Uint8Array(100_000).buffer);

        expect(url.startsWith("data:image/png;base64,")).toBe(true);
        expect(atob(url.slice("data:image/png;base64,".length))).toHaveLength(100_000);
    });
});
