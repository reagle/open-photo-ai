import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CropInfo } from "@/ipc/crop";
import type { Operation } from "@/ipc/enhance";
import type { LensInfo } from "@/ipc/lens";
import { fNumberForAmount } from "@/lib/blurAmount";
import { useEnhancementStore } from "@/stores/enhancements";
import { HOLIDAY } from "@/test/support";
import { fNumberAfterCrop, mapFocus, reframeLensBlur } from "./reframe";

// What the photograph's own lens is at each framing, keyed by the framing's width (0 for the whole photograph), asked
// of the backend through the one `invoke` boundary.
const lenses = new Map<number, LensInfo>();

vi.mock("@/ipc/lens", () => ({
    lensInfo: vi.fn(async (_identity: string, crop?: CropInfo) => {
        const lens = lenses.get(crop?.width ?? 0);
        if (!lens) throw new Error("no lens");

        return lens;
    }),
}));

const whole = { width: 3000, height: 2000 };

const crop = (left: number, top: number, width: number, height: number, extra: Partial<CropInfo> = {}): CropInfo => ({
    left,
    top,
    width,
    height,
    millidegrees: 0,
    flipHorizontal: false,
    flipVertical: false,
    ...extra,
});

describe("mapFocus", () => {
    it("is the same point where the framing does not change", () => {
        expect(mapFocus({ x: 0.3, y: 0.7 }, undefined, undefined, whole)).toEqual({ x: 0.3, y: 0.7 });
        expect(mapFocus({ x: 0.3, y: 0.7 }, crop(10, 20, 500, 400), crop(10, 20, 500, 400), whole)).toEqual({
            x: 0.3,
            y: 0.7,
        });
    });

    it("keeps the focus on the same subject across a crop", () => {
        // The subject at (1800, 800) of the file: 0.6, 0.4 of the whole, and 0.5, 0.5 of a crop centred on it.
        const onto = crop(1300, 400, 1000, 800);

        expect(mapFocus({ x: 0.6, y: 0.4 }, undefined, onto, whole)).toEqual({ x: 0.5, y: 0.5 });
        // And back out again.
        expect(mapFocus({ x: 0.5, y: 0.5 }, onto, undefined, whole)).toEqual({ x: 0.6, y: 0.4 });
    });

    it("carries it from one crop to another through the file's own pixels", () => {
        const first = crop(0, 0, 1500, 1000);
        const second = crop(750, 500, 1500, 1000);

        // (1125, 750) of the file: 0.75 of the first, and 0.25 of the second.
        expect(mapFocus({ x: 0.75, y: 0.75 }, first, second, whole)).toEqual({ x: 0.25, y: 0.25 });
    });

    it("clamps a subject the new frame leaves out to its nearest edge", () => {
        // The subject at the far left of the whole, and a crop that starts well to the right of it.
        const onto = crop(1500, 500, 1000, 1000);

        expect(mapFocus({ x: 0.1, y: 0.5 }, undefined, onto, whole)).toEqual({ x: 0, y: 0.5 });
        expect(mapFocus({ x: 0.95, y: 0.99 }, undefined, onto, whole)).toEqual({ x: 1, y: 1 });
    });

    it("follows the mirrors", () => {
        // A flip across the whole photograph puts a subject at 0.2 at 0.8.
        const mirrored = crop(0, 0, 3000, 2000, { flipHorizontal: true });

        expect(mapFocus({ x: 0.2, y: 0.3 }, undefined, mirrored, whole)).toEqual({ x: 0.8, y: 0.3 });
        expect(mapFocus({ x: 0.8, y: 0.3 }, mirrored, undefined, whole)).toEqual({ x: 0.2, y: 0.3 });
    });

    it("follows a quarter turn, clockwise", () => {
        // A 3000x2000 photograph turned a quarter clockwise is 2000x3000, and what was at the top left is at the top
        // right: (0.1, 0.1) goes to (0.9, 0.1).
        const turned = crop(0, 0, 2000, 3000, { millidegrees: 90_000 });

        const moved = mapFocus({ x: 0.1, y: 0.1 }, undefined, turned, whole);

        expect(moved.x).toBeCloseTo(0.9, 3);
        expect(moved.y).toBeCloseTo(0.1, 3);
        const back = mapFocus(moved, turned, undefined, whole);
        expect(back.x).toBeCloseTo(0.1, 3);
        expect(back.y).toBeCloseTo(0.1, 3);
    });

    it("leaves the centre of a photograph at the centre of a levelled one", () => {
        const levelled = crop(0, 0, 3000, 2000, { millidegrees: 3_000 });

        const moved = mapFocus({ x: 0.5, y: 0.5 }, undefined, levelled, whole);

        // The canvas is a little larger than the photograph, but both turn about the same centre.
        expect(moved.x).toBeCloseTo(0.5, 1);
        expect(moved.y).toBeCloseTo(0.5, 1);
    });

    it("is the point as it was where the photograph's size is not known", () => {
        const onto = crop(1300, 400, 1000, 800);

        expect(mapFocus({ x: 0.6, y: 0.4 }, undefined, onto, undefined)).toEqual({ x: 0.6, y: 0.4 });
        expect(mapFocus({ x: 0.6, y: 0.4 }, undefined, onto, { width: 3000 })).toEqual({ x: 0.6, y: 0.4 });
    });

    it("rounds to the thousandths the library keeps", () => {
        const onto = crop(0, 0, 1000, 1000);

        expect(mapFocus({ x: 0.1111, y: 0.2222 }, undefined, onto, { width: 1000, height: 1000 })).toEqual({
            x: 0.111,
            y: 0.222,
        });
    });
});

describe("fNumberAfterCrop", () => {
    it("moves As taken to the new frame's As taken, whichever way the frame went", () => {
        expect(fNumberAfterCrop(6, 6, 12)).toBe(12);
        expect(fNumberAfterCrop(12, 12, 6)).toBe(6);
    });

    it("moves a value above the old own f-number, which is shown at As taken, to the new one too", () => {
        expect(fNumberAfterCrop(8, 6, 12)).toBe(12);
    });

    it("keeps the blur amount the user chose, against the new own f-number", () => {
        // Amount 50 is f/2.27 of the whole photograph (f/6) and f/2.507 of the crop (f/12), and amount 100 is f/1.4.
        expect(fNumberAfterCrop(2.27, 6, 12)).toBe(2.507);
        expect(fNumberAfterCrop(2.507, 12, 6)).toBe(2.27);
        expect(fNumberAfterCrop(1.4, 6, 12)).toBe(1.4);
    });

    it("does not walk about by the thousandths an f-number is quantised to, across a crop and back", () => {
        let fNumber = fNumberForAmount(37, 6);

        for (let round = 0; round < 5; round++) {
            fNumber = fNumberAfterCrop(fNumberAfterCrop(fNumber, 6, 12), 12, 6);
        }

        expect(fNumber).toBe(fNumberForAmount(37, 6));
    });

    it("leaves the value alone where the framing leaves the own f-number, a flip or a turn, whatever the value is", () => {
        expect(fNumberAfterCrop(2.8, 6, 6)).toBe(2.8);
        expect(fNumberAfterCrop(2.8, 6, 6.004)).toBe(2.8);
    });

    it("is the new own f-number for a photograph that could not be blurred more, which adds nothing", () => {
        expect(fNumberAfterCrop(1.2, 1.2, 2.4)).toBe(2.4);
    });
});

describe("reframeLensBlur", () => {
    const lensBlur = (parameters: Record<string, number>): Operation => ({
        family: "lens_blur",
        codename: "lisbon",
        precision: "fp16",
        parameters,
    });

    const stored = () => useEnhancementStore.getState().enhancements.get(HOLIDAY.path)?.[0];
    const put = (operation: Operation) =>
        useEnhancementStore.setState({ enhancements: new Map([[HOLIDAY.path, [operation]]]) });

    const onto = crop(1300, 400, 1000, 800);

    beforeEach(() => {
        useEnhancementStore.setState({ enhancements: new Map() });
        lenses.clear();
        lenses.set(0, { ownFNumber: 6, focal35mm: 36, cropFactor: 1.5, origin: "exif" });
        lenses.set(1000, { ownFNumber: 12, focal35mm: 72, cropFactor: 3, origin: "exif" });
    });

    it("keeps a lens blur at As taken after a crop, and its focus on the same subject", async () => {
        put(lensBlur({ f_number: 6, focus_x: 0.6, focus_y: 0.4 }));

        await reframeLensBlur(HOLIDAY, undefined, onto);

        expect(stored()?.parameters).toEqual({ f_number: 12, focus_x: 0.5, focus_y: 0.5 });
    });

    it("keeps the blur amount the user chose, and the same amount when the crop is undone", async () => {
        put(lensBlur({ f_number: 2.27, focus_x: 0.5, focus_y: 0.5 }));

        await reframeLensBlur(HOLIDAY, undefined, onto);
        // Amount 50 of the whole photograph (f/6) is f/2.27; of the crop (f/12) it is f/2.507.
        expect(stored()?.parameters.f_number).toBe(2.507);

        await reframeLensBlur(HOLIDAY, onto, undefined);
        expect(stored()?.parameters.f_number).toBe(2.27);
        expect(stored()?.parameters.focus_x).toBeCloseTo(0.5, 3);
    });

    it("keeps the widest blur the widest", async () => {
        put(lensBlur({ f_number: 1.4, focus_x: 0.5, focus_y: 0.5 }));

        await reframeLensBlur(HOLIDAY, undefined, onto);

        expect(stored()?.parameters.f_number).toBe(1.4);
    });

    it("writes nothing where nothing changes, though the lens blur carries no focus", async () => {
        put(lensBlur({ f_number: 6 }));
        const before = useEnhancementStore.getState().enhancements;

        await reframeLensBlur(HOLIDAY, undefined, undefined);

        expect(useEnhancementStore.getState().enhancements).toBe(before);
    });

    it("answers a lens it could not ask about as the assumed f/8, both ends alike", async () => {
        vi.spyOn(console, "error").mockImplementation(() => {});
        lenses.clear();
        put(lensBlur({ f_number: 8, focus_x: 0.5, focus_y: 0.5 }));

        await reframeLensBlur(HOLIDAY, undefined, onto);

        expect(stored()?.parameters.f_number).toBe(8);
    });

    it("does nothing for a photograph with no lens blur, or no identity", async () => {
        const before = useEnhancementStore.getState().enhancements;

        await reframeLensBlur(HOLIDAY, undefined, onto);
        await reframeLensBlur({ path: HOLIDAY.path, extension: "png" }, undefined, onto);
        await reframeLensBlur(undefined, undefined, onto);

        expect(useEnhancementStore.getState().enhancements).toBe(before);
    });

    it("reads the stack when the lens arrives, so an edit made meanwhile is kept", async () => {
        put(lensBlur({ f_number: 6, focus_x: 0.6, focus_y: 0.4 }));

        const pending = reframeLensBlur(HOLIDAY, undefined, onto);
        put(lensBlur({ f_number: 6, focus_x: 0.6, focus_y: 0.4, extra: 1 }));
        await pending;

        expect(stored()?.parameters.extra).toBe(1);
        expect(stored()?.parameters.f_number).toBe(12);
    });
});
