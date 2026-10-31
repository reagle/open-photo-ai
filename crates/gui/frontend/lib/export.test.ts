import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import i18n from "@/i18n";
import type { Operation } from "@/ipc/enhance";
import type { ExportName, Loss } from "@/ipc/export";
import { EXPORT_FORMATS } from "@/test/support";
import {
    describeExportError,
    FORMAT_CHOICES,
    formatLoss,
    groupByChain,
    isExportFormat,
    qualityFor,
    queueQualityFor,
    typeLabel,
    warnsOf,
    writesUltraHdr,
} from "./export";
import { directoryOf } from "./paths";

/** A `t` that shows which key it was asked for and with what, so a test reads the composition rather than English. */
const t = ((key: string, values?: Record<string, unknown>) =>
    values ? `${key}(${JSON.stringify(values)})` : key) as unknown as TFunction;

describe("FORMAT_CHOICES", () => {
    it("offers Preserve, the eight formats, and Ultra HDR right after plain JPEG", () => {
        expect(FORMAT_CHOICES).toEqual([
            "preserve",
            "avif",
            "bmp",
            "gif",
            "heic",
            "jpeg",
            "uhdr",
            "png",
            "tiff",
            "webp",
        ]);
    });

    it("counts neither Preserve nor Ultra HDR as a format a quality is kept for", () => {
        expect(FORMAT_CHOICES.filter(isExportFormat)).toEqual(EXPORT_FORMATS.formats.map(({ format }) => format));
    });
});

describe("Ultra HDR on a row", () => {
    const english = i18n.getFixedT("en");
    const CHOSEN: Loss = { kind: "hdr", hdr: "gainMap", format: "JPEG", cause: "chosen" };
    const NOT_CARRIED: Loss = { kind: "hdr", hdr: "gainMap", format: "JPEG", cause: "notCarried" };
    const DEPTH: Loss = {
        kind: "depth",
        from: { bits: 16, float: false },
        to: { bits: 8, float: false },
        format: "JPEG",
    };
    const named = (format: ExportName["format"], gainMap: ExportName["gainMap"]): ExportName => ({
        path: "/p/hawk.jpg",
        name: "hawk.jpg",
        destination: "/p/hawk.jpg",
        format,
        extension: format === "jpeg" ? "jpg" : format,
        gainMap,
    });

    it("labels a type JPG, or JPG (Ultra HDR) in the locale's words", () => {
        expect(typeLabel(english, "jpg", false)).toBe("JPG");
        expect(typeLabel(english, "jpeg", true)).toBe("JPEG (Ultra HDR)");
        expect(typeLabel(i18n.getFixedT("ja"), "jpg", true)).toBe("JPG（Ultra HDR）");
    });

    it("is written only from a gain-map source, as JPEG keeping it, where no HDR is lost", () => {
        expect(writesUltraHdr(named("jpeg", "keep"), true, [])).toBe(true);
        // A depth loss is no HDR loss.
        expect(writesUltraHdr(named("jpeg", "keep"), true, [DEPTH])).toBe(true);
        expect(writesUltraHdr(named("jpeg", "keep"), false, [])).toBe(false);
        expect(writesUltraHdr(named("jpeg", "drop"), true, [CHOSEN])).toBe(false);
        expect(writesUltraHdr(named("jpeg", "keep"), true, [NOT_CARRIED])).toBe(false);
        expect(writesUltraHdr(named("png", "keep"), true, [])).toBe(false);
    });

    it("warns of a gain map that could not be carried, and not of one left out by choice", () => {
        expect(warnsOf([CHOSEN])).toBe(false);
        expect(warnsOf([CHOSEN, DEPTH])).toBe(true);
        expect(warnsOf([NOT_CARRIED])).toBe(true);
        expect(warnsOf([])).toBe(false);
    });
});

describe("directoryOf", () => {
    it.each([
        ["/photos/beach.jpg", "/photos"],
        ["C:\\Users\\someone\\beach.jpg", "C:\\Users\\someone"],
        ["/beach.jpg", "/"],
        ["beach.jpg", ""],
    ])("the directory of %s is %s", (path, directory) => {
        expect(directoryOf(path)).toBe(directory);
    });
});

describe("groupByChain", () => {
    const upscale: Operation = { family: "upscale", codename: "kyoto", precision: "fp32", parameters: { scale: 2 } };
    const bigger: Operation = { ...upscale, parameters: { scale: 4 } };
    const denoise = { family: "denoise", codename: "stockholm", precision: "fp32" } as Operation;

    it("puts stacks running the same families together, groups in the order of their first member", () => {
        const stacks = new Map<string, Operation[]>([
            ["A", [upscale]],
            ["B", [denoise, upscale]],
            ["C", [bigger]],
            ["D", [denoise, upscale]],
        ]);

        expect(groupByChain(["A", "B", "C", "D"], (path) => stacks.get(path))).toEqual(["A", "C", "B", "D"]);
    });

    it("groups paths with no stack or an empty one together, keeping drawer order within every group", () => {
        const stacks = new Map<string, Operation[]>([
            ["B", [upscale]],
            ["C", []],
            ["E", [upscale]],
        ]);

        expect(groupByChain(["A", "B", "C", "D", "E"], (path) => stacks.get(path))).toEqual(["A", "C", "D", "B", "E"]);
    });

    it("leaves a uniform queue in its order", () => {
        expect(groupByChain(["C", "A", "B"], () => [upscale])).toEqual(["C", "A", "B"]);
    });
});

describe("qualityFor", () => {
    it("is the written format's published range, for a format that takes one", () => {
        expect(qualityFor("jpeg", EXPORT_FORMATS)).toEqual({
            format: "jpeg",
            range: { min: 1, max: 100, default: 90 },
        });
        expect(qualityFor("png", EXPORT_FORMATS)).toBeUndefined();
    });

    it("reads which formats take one off what Rust publishes, naming none of its own", () => {
        const noneLossy = {
            ...EXPORT_FORMATS,
            formats: EXPORT_FORMATS.formats.map((format) => ({ ...format, quality: null })),
        };

        expect(qualityFor("jpeg", noneLossy)).toBeUndefined();
    });
});

describe("queueQualityFor", () => {
    it("is the one lossy format a uniform queue is written in", () => {
        expect(queueQualityFor(["webp", "webp"], EXPORT_FORMATS)?.format).toBe("webp");
        // Under Preserve, `.jpg` and `.jpeg` are both written as JPEG, and `.heic` and `.heif` both as HEIC.
        expect(queueQualityFor(["jpeg", "jpeg"], EXPORT_FORMATS)?.format).toBe("jpeg");
        expect(queueQualityFor(["heic", "heic"], EXPORT_FORMATS)?.format).toBe("heic");
    });

    it("is nothing for a mixed Preserve", () => {
        expect(queueQualityFor(["jpeg", "png"], EXPORT_FORMATS)).toBeUndefined();
        expect(queueQualityFor(["jpeg", "webp"], EXPORT_FORMATS)).toBeUndefined();
    });

    it("is nothing for a lossless format, and for an empty queue", () => {
        expect(queueQualityFor(["png"], EXPORT_FORMATS)).toBeUndefined();
        // A RAW source under Preserve, written as TIFF.
        expect(queueQualityFor(["tiff"], EXPORT_FORMATS)).toBeUndefined();
        expect(queueQualityFor([], EXPORT_FORMATS)).toBeUndefined();
    });
});

describe("describeExportError", () => {
    it("names the file and the folder of a write from the error's own path, then the operating system's reason untranslated", () => {
        const error = { kind: "write", path: "/exports/beach-opai.png", message: "Permission denied (os error 13)" };

        expect(describeExportError(t, { cause: "export", error })).toBe(
            'export.queue.writeFailed({"name":"beach-opai.png","folder":"/exports"}) Permission denied (os error 13)',
        );
    });

    it.each([
        [{ kind: "enhance", message: "the model could not be downloaded" }, "the model could not be downloaded"],
        [{ kind: "unreadableSource", identity: "0123", message: "no such file" }, "no such file"],
        [{ kind: "notReady" }, "notReady"],
        [{ kind: "unknownSource", identity: "0123" }, "unknownSource"],
        [{ kind: "unknownOperation", index: 0, reason: { kind: "model" } }, "unknownOperation"],
    ])("says an enhancement failed, with the library's sentence or the kind: %j", (error, detail) => {
        expect(describeExportError(t, { cause: "export", error })).toBe(`export.queue.enhanceFailed ${detail}`);
    });

    it("says a record with no identity could not be read", () => {
        expect(describeExportError(t, { cause: "unreadable" })).toBe("export.queue.unreadable");
    });

    it("says the analysis failed, with its message where the rejection carries one", () => {
        const error = { kind: "analyse", message: "the detector failed" };

        expect(describeExportError(t, { cause: "analysis", error })).toBe(
            "export.queue.analysisFailed the detector failed",
        );
        expect(describeExportError(t, { cause: "analysis" })).toBe("export.queue.analysisFailed");
    });
});

describe("formatLoss", () => {
    // The English catalogue itself, so these read as the window does; every locale has the same keys (`parity.test`).
    const english = i18n.getFixedT("en");
    const words = (loss: Loss) => formatLoss(english, loss);
    const eight = { bits: 8, float: false };

    it("words an HDR loss to the format by how the source declared it", () => {
        expect(words({ kind: "hdr", hdr: "gainMap", format: "PNG", cause: "format" })).toBe(
            "HDR gain map dropped: PNG output is SDR only for now",
        );
        expect(words({ kind: "hdr", hdr: "pq", format: "PNG", cause: "format" })).toBe(
            "HDR (PQ) dropped: PNG output is SDR only for now",
        );
    });

    it("words a gain map a JPEG could not carry", () => {
        expect(words({ kind: "hdr", hdr: "gainMap", format: "JPEG", cause: "notCarried" })).toBe(
            "HDR gain map dropped: it could not be carried",
        );
    });

    it("words a gain map plain JPEG left out as what was done, not as a failure", () => {
        expect(words({ kind: "hdr", hdr: "gainMap", format: "JPEG", cause: "chosen" })).toBe(
            "Written as SDR (gain map left out)",
        );
    });

    it("words a depth loss, floats included", () => {
        expect(words({ kind: "depth", from: { bits: 16, float: false }, to: eight, format: "JPEG" })).toBe(
            "16-bit → 8-bit in JPEG",
        );
        expect(
            words({ kind: "depth", from: { bits: 32, float: true }, to: { bits: 16, float: false }, format: "PNG" }),
        ).toBe("32-bit float → 16-bit in PNG");
    });

    it("words each profile loss, naming HEIF as the window does and an unnamed profile as ICC", () => {
        const profile = { kind: "profile", profile: "Display P3", colourSpace: "RGB", format: "AVIF" } as const;

        expect(words({ ...profile, cause: "format" })).toBe("Display P3 profile not embedded in AVIF");
        expect(words({ ...profile, format: "HEIF", cause: "format" })).toBe("Display P3 profile not embedded in HEIC");
        expect(words({ ...profile, profile: null, format: "GIF", cause: "format" })).toBe(
            "ICC profile not embedded in GIF",
        );
        expect(words({ ...profile, colourSpace: "GRAY", format: "WebP", cause: "colourModel" })).toBe(
            "Display P3 profile not embedded in WebP: it is for GRAY, not the file's colors",
        );
        expect(words({ ...profile, profile: "SWOP", colourSpace: "CMYK", cause: "discarded" })).toBe(
            "SWOP profile dropped: it is for CMYK, not the image's pixels",
        );
        expect(words({ ...profile, colourSpace: "", format: "PNG", cause: "coded" })).toBe(
            "Display P3 color description not written in PNG",
        );
        expect(words({ ...profile, profile: "sRGB IEC61966-2.1", format: "JPEG", cause: "notWritten" })).toBe(
            "sRGB IEC61966-2.1 profile not embedded in JPEG: the encoder could not write it",
        );
    });
});
