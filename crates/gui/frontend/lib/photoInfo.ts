import type { TFunction } from "i18next";
import type { CropInfo } from "@/ipc/crop";
import type { Operation } from "@/ipc/enhance";
import type { PhotoInfo } from "@/ipc/photoInfo";
import { ENHANCEMENTS, enlargement, F_NUMBER, runnable } from "@/lib/enhancements";

/**
 * What the information card says: one line each for the camera and lens, the exposure, when, the pixels and the edits,
 * each `undefined` where there is nothing to say.
 *
 * **As shot, then as rendered.** Where what the stack does changes a number the file states, the line says both,
 * `as shot → as rendered`: a lens blur's simulated f-number after the lens's own, a crop's 35 mm equivalent after the
 * lens's, and the size through the crop and the enlargement. The capture is never restated as if the camera had done
 * what the stack does: the export keeps it as shot too (see `opai::export::metadata::record`).
 */
export type InfoLines = {
    camera: string | undefined;
    lens: string | undefined;
    exposure: string | undefined;
    taken: string | undefined;
    pixels: string | undefined;
    edits: string | undefined;
    /** What an earlier Open Photo AI export recorded rendering into this file. */
    earlier: string | undefined;
    /** Whether the file records where it was taken; the place is never shown. */
    located: boolean;
};

/** `→`, with the spaces the card writes it with. */
const ARROW = " → ";

/** A number with at most `digits` decimals and no trailing zeros: `1.8`, `26`, `0.5`. */
const short = (value: number, digits = 1) => String(Number(value.toFixed(digits)));

/** An f-number as a lens is labelled: `f/1.8`, `f/8`. */
export const fNumber = (value: number) => `f/${short(value)}`;

/** An exposure time as a camera shows it: `1/120 s` under a second, `2 s` and `2.5 s` at or over. */
export const shutter = (seconds: number) => (seconds < 1 ? `1/${Math.round(1 / seconds)} s` : `${short(seconds)} s`);

/** The camera's name: its model, after its maker where the model does not already start with it. */
const cameraOf = ({ make, model }: PhotoInfo): string | undefined => {
    if (!make) return model ?? undefined;
    if (!model) return make;

    return model.toLowerCase().startsWith(make.toLowerCase()) ? model : `${make} ${model}`;
};

/**
 * How many times the framed photograph's diagonal goes into the file's: what a crop multiplies the 35 mm equivalent
 * by. 1 where nothing is cut, or the file's size is not known.
 */
export const cropFactorOf = (info: Pick<PhotoInfo, "width" | "height">, crop: CropInfo | undefined): number => {
    if (!crop || !info.width || !info.height) return 1;

    return Math.hypot(info.width, info.height) / Math.hypot(crop.width, crop.height);
};

/** The lens blur's f-number in `operations`, if one runs. */
const simulatedFNumber = (operations: readonly Operation[]): number | undefined =>
    operations.find((operation) => operation.family === "lens_blur")?.parameters[F_NUMBER];

/**
 * The card's lines for the photograph `info` describes, framed by `crop` and set to have `stack` done to it, in the
 * language `t` speaks.
 */
export const infoLines = (
    t: TFunction,
    info: PhotoInfo,
    stack: readonly Operation[],
    crop: CropInfo | undefined,
): InfoLines => {
    const operations = runnable(stack);
    const factor = cropFactorOf(info, crop);
    const cropped = factor > 1.001;

    // `26 mm · 52 mm eq.`, and `26 → 52 mm eq. (crop)` where a crop narrows the field of view.
    const equivalent = (() => {
        if (info.focal35mm === null) return;
        if (!cropped) return t("preview.info.equivalent", { value: short(info.focal35mm, 0) });

        return t("preview.info.croppedEquivalent", {
            shot: short(info.focal35mm, 0),
            rendered: short(info.focal35mm * factor, 0),
        });
    })();

    const simulated = simulatedFNumber(operations);
    const aperture = (() => {
        if (simulated === undefined) return info.fNumber === null ? undefined : fNumber(info.fNumber);

        const rendered = t("preview.info.simulated", { value: fNumber(simulated) });

        return info.fNumber === null ? rendered : `${fNumber(info.fNumber)}${ARROW}${rendered}`;
    })();

    const exposure = [
        info.focalMm === null ? undefined : `${short(info.focalMm)} mm`,
        equivalent,
        aperture,
        info.exposureSeconds === null ? undefined : shutter(info.exposureSeconds),
        info.iso === null ? undefined : t("preview.info.iso", { value: info.iso }),
    ].filter((part) => part !== undefined);

    // `4032×3024 → 2016×1512 → 8064×6048`: the file, the frame, then what the chain enlarges it to.
    const pixels = (() => {
        if (info.width === null || info.height === null) return;

        const sizes = [`${info.width}×${info.height}`];
        const framed = crop ? { width: crop.width, height: crop.height } : { width: info.width, height: info.height };
        if (crop) sizes.push(`${framed.width}×${framed.height}`);
        const scale = enlargement(operations);
        if (scale !== 1) sizes.push(`${Math.round(framed.width * scale)}×${Math.round(framed.height * scale)}`);

        const hdr = info.hdr === null ? undefined : t(`preview.info.hdr.${info.hdr}`);

        return [sizes.join(ARROW), info.colorProfile ?? undefined, hdr]
            .filter((part) => part !== undefined)
            .join(" · ");
    })();

    const names = [
        ...(crop ? [t("crop.title")] : []),
        ...operations.map((operation) => {
            const entry = ENHANCEMENTS.find((enhancement) => enhancement.family === operation.family);

            return entry ? t(entry.nameKey) : operation.family;
        }),
    ];

    const earlier = [
        ...(info.renderedFNumber === null
            ? []
            : [t("preview.info.blurredEarlier", { value: fNumber(info.renderedFNumber) })]),
        ...(info.rendered.length > 0 && info.renderedFNumber === null ? [t("preview.info.enhancedEarlier")] : []),
    ];

    return {
        camera: cameraOf(info),
        lens: info.lensModel ?? undefined,
        exposure: exposure.length > 0 ? exposure.join(" · ") : undefined,
        taken: info.taken === null ? undefined : [info.taken, info.takenOffset].filter(Boolean).join(" "),
        pixels,
        edits: names.length > 0 ? names.join(", ") : undefined,
        earlier: earlier.length > 0 ? earlier.join(" ") : undefined,
        located: info.located,
    };
};
