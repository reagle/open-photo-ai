import { describe, expect, it } from "vitest";
import i18n from "@/i18n";
import type { CropInfo } from "@/ipc/crop";
import type { Operation } from "@/ipc/enhance";
import type { PhotoInfo } from "@/ipc/photoInfo";
import { cropFactorOf, fNumber, infoLines, shutter } from "./photoInfo";

// The English catalogue itself, so these read as the card does; every locale has the same keys (`parity.test`).
const english = i18n.getFixedT("en");

const SHOT: PhotoInfo = {
    make: "Apple",
    model: "iPhone 15 Pro",
    lensModel: "iPhone 15 Pro back camera 6.86mm f/1.78",
    focalMm: 6.86,
    focal35mm: 24,
    fNumber: 1.78,
    exposureSeconds: 1 / 120,
    iso: 50,
    taken: "2026-10-18 16:03:50",
    takenOffset: "+02:00",
    width: 4032,
    height: 3024,
    colorProfile: "Display P3",
    hdr: "gainMap",
    located: true,
    renderedFNumber: null,
    rendered: [],
};

/** The centre half of the 4032x3024 frame. */
const HALF: CropInfo = {
    left: 1008,
    top: 756,
    width: 2016,
    height: 1512,
    millidegrees: 0,
    flipHorizontal: false,
    flipVertical: false,
};

const operation = (family: Operation["family"], parameters: Record<string, number>): Operation => ({
    family,
    codename: "any",
    precision: "fp16",
    parameters,
});

describe("infoLines", () => {
    it("says what the photograph was taken with, as shot, where nothing is done to it", () => {
        expect(infoLines(english, SHOT, [], undefined)).toEqual({
            camera: "Apple iPhone 15 Pro",
            lens: "iPhone 15 Pro back camera 6.86mm f/1.78",
            exposure: "6.9 mm · 24 mm eq. · f/1.8 · 1/120 s · ISO 50",
            taken: "2026-10-18 16:03:50 +02:00",
            pixels: "4032×3024 · Display P3 · HDR gain map",
            edits: undefined,
            earlier: undefined,
            located: true,
        });
    });

    it("says a crop, an upscale and a lens blur as shot then as rendered", () => {
        const stack = [operation("upscale", { scale: 4 }), operation("lens_blur", { f_number: 1.4 })];
        const lines = infoLines(english, SHOT, stack, HALF);

        expect(lines.exposure).toBe("6.9 mm · 24 → 48 mm eq. (crop) · f/1.8 → f/1.4 simulated · 1/120 s · ISO 50");
        expect(lines.pixels).toBe("4032×3024 → 2016×1512 → 8064×6048 · Display P3 · HDR gain map");
        expect(lines.edits).toBe("Crop/Rotate, Upscale, Lens Blur");
    });

    it("names a camera whose model already says its maker once, and leaves out what the file does not say", () => {
        const lines = infoLines(
            english,
            { ...SHOT, make: "Canon", model: "Canon EOS R5", lensModel: null, focalMm: null, iso: null, hdr: null },
            [],
            undefined,
        );

        expect(lines.camera).toBe("Canon EOS R5");
        expect(lines.lens).toBeUndefined();
        expect(lines.exposure).toBe("24 mm eq. · f/1.8 · 1/120 s");
        expect(lines.pixels).toBe("4032×3024 · Display P3");
    });

    it("says a photograph was already blurred by an earlier export", () => {
        const lines = infoLines(english, { ...SHOT, renderedFNumber: 2, rendered: ["Lens Blur"] }, [], undefined);

        expect(lines.earlier).toBe("Lens Blur already applied (f/2 simulated)");
    });
});

describe("the numbers", () => {
    it("are written as a camera writes them", () => {
        expect(fNumber(8)).toBe("f/8");
        expect(fNumber(1.78)).toBe("f/1.8");
        expect(shutter(1 / 4000)).toBe("1/4000 s");
        expect(shutter(2.5)).toBe("2.5 s");
    });

    it("measure a crop against the file's own diagonal", () => {
        expect(cropFactorOf(SHOT, HALF)).toBeCloseTo(2);
        expect(cropFactorOf(SHOT, undefined)).toBe(1);
        expect(cropFactorOf({ width: null, height: null }, HALF)).toBe(1);
    });
});
