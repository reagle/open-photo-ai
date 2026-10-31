import { describe, expect, it } from "vitest";
import i18n from "@/i18n";
import type { FamilyEntry } from "@/ipc/catalogue";
import { isUnavailableModel, type UnavailableModel, unavailableNotice } from "./unavailable";

// Fork-only, as what it tests. Kazan stands for any model this fork exports: published at HD and SD, placed at SD.
const restore: FamilyEntry = {
    family: "upscale",
    variants: [
        { codename: "kazan", label: "Kazan", precisions: ["fp32", "fp16"], unavailable: ["fp32"], parameters: [] },
        { codename: "lima", label: "Lima", precisions: ["fp32"], unavailable: ["fp32"], parameters: [] },
    ],
};

const refused = (codename: string, precision: UnavailableModel["precision"]): UnavailableModel => ({
    kind: "unavailableModel",
    message: "the library's sentence",
    family: "upscale",
    codename,
    precision,
});

describe("the notice for a model this install cannot run", () => {
    it("names the model as the tray does and the tier to use instead", () => {
        expect(unavailableNotice(i18n.t, refused("kazan", "fp32"), restore)).toBe(
            "Kazan HD (FP32) isn't available on this install; use SD.",
        );
    });

    it("gives no advice where the model can be run at nothing else", () => {
        expect(unavailableNotice(i18n.t, refused("lima", "fp32"), restore)).toBe(
            "Lima (FP32) isn't available on this install.",
        );
    });

    it("names a model the catalogue does not by its codename rather than not at all", () => {
        expect(unavailableNotice(i18n.t, refused("retired", "fp16"), restore)).toBe(
            "retired (FP16) isn't available on this install.",
        );
    });

    it("is told apart from every other rejection by its kind", () => {
        expect(isUnavailableModel(refused("kazan", "fp32"))).toBe(true);
        expect(isUnavailableModel({ kind: "enhance", message: "a" })).toBe(false);
        expect(isUnavailableModel("a string")).toBe(false);
        expect(isUnavailableModel(null)).toBe(false);
    });
});
