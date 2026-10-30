import type { TFunction } from "i18next";
import type { FamilyEntry } from "@/ipc/catalogue";
import type { EnhanceError } from "@/ipc/enhance";
import { modelChoice, modelLabel, qualityLabel } from "@/lib/enhancements";

// Fork-only, not for upstream: every model upstream names can be run at every precision it is published at, so its
// runs never reject this way.

/** A run's rejection naming a model this install cannot run at the precision it was asked for. */
export type UnavailableModel = Extract<EnhanceError, { kind: "unavailableModel" }>;

/** Whether `error`, a run's rejection as `invoke` hands it over, is an {@link UnavailableModel}. */
export const isUnavailableModel = (error: unknown): error is UnavailableModel =>
    typeof error === "object" && error !== null && (error as { kind?: unknown }).kind === "unavailableModel";

/**
 * The notice for `error`: the model as the choosers name it, the precision it was asked at, and what to use instead -
 * "Kazan HD (FP32) isn't available on this install; use SD." - where the catalogue `entry` names a precision of the
 * same model this install can run, and the sentence without the advice where it names none.
 *
 * Composed from the catalogue rather than from the artifact the library refused, so the model is named the way the
 * tray names it, and the advice is the tier the tray offers.
 */
export const unavailableNotice = (t: TFunction, error: UnavailableModel, entry: FamilyEntry | undefined): string => {
    const own = modelChoice(entry, undefined).options.filter((option) => option.codename === error.codename);
    const asked = own.find((option) => option.precision === error.precision);
    const instead = own.find((option) => option.available && option.precision !== error.precision);

    const model = asked ? modelLabel(t, asked) : error.codename;
    const precision = error.precision.toUpperCase();

    if (!instead) return t("errors.modelUnavailableAlone", { model, precision });

    const alternative = qualityLabel(t, instead.quality) ?? instead.precision.toUpperCase();

    return t("errors.modelUnavailable", { model, precision, alternative });
};
