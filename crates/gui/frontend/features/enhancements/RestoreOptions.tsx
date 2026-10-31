import { Sparkles } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Separator } from "@/components/ui/separator";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { FamilyEntry } from "@/ipc/catalogue";
import type { Operation } from "@/ipc/enhance";
import { type Enhancement, parameterOf, publishedRange, withParameter } from "@/lib/enhancements";
import { cn, toPercent } from "@/lib/utils";
import { IntensityControl } from "./IntensityControl";
import { OperationModelTray } from "./ModelTray";
import { SHORTCUT_ITEM } from "./ScaleControl";

/** How many percent one unit is: the wire speaks units, this panel speaks percent. */
const PERCENT = 100;

// The three sizes the command line's `--rs-scale` takes too. Not a typed factor, unlike an upscale's: a
// restore's cost grows with the square of its size and its result is redrawn rather than enlarged, so the
// choice is between a few sizes rather than along a range. Each is kept only where the selected model's
// published range reaches it, which is what keeps a size the library would bring down off the screen.
/** The sizes a restore is offered at, in the order the group draws them. */
const SIZES = [1, 2, 4] as const;

type RestoreOptionsProps = {
    /** The enhancement this panel is open over, which every control here reports and writes. */
    enhancement: Enhancement;
    /** The restore itself, as the stack carries it. */
    operation: Operation;
    /** What the catalogue publishes for restore: its models, their precisions and their bounds. */
    entry: FamilyEntry | undefined;
    /** Writes the enhancement back to the stack, which is what re-runs the preview. */
    onChange: (operation: Operation) => void;
};

/**
 * What a restore can be set to: which model redraws the photograph, the size it does it at, how much of the
 * redraw is kept and how much of the photograph's own grain is put back - with the one sentence a generative
 * enhancement owes its user about what it does.
 *
 * **Nothing is written on open**, for `UpscaleOptions`' reason: writing the stack asks for a new run.
 *
 * **Every amount is the selected model's own published range**: the size's for which sizes are offered, the
 * strength's for its slider, which stops at 100% - a redraw is blended back towards the photograph it was given,
 * never pushed past what the model drew - and the grain's for its own, from none to all of the noise the restoration
 * took away. The library measures and draws the grain; this panel only says how much.
 */
export const RestoreOptions = ({ enhancement, operation, entry, onChange }: RestoreOptionsProps) => {
    const { t } = useTranslation();

    const scales = publishedRange(entry, operation.codename, "scale");
    const strengths = publishedRange(entry, operation.codename, "strength");
    const grains = publishedRange(entry, operation.codename, "grain");
    // 1x where neither the operation nor the catalogue knows a size: the photograph as it is.
    const size = parameterOf(operation, entry, "scale") ?? 1;
    const offered = SIZES.filter((candidate) => !scales || (candidate >= scales.min && candidate <= scales.max));

    return (
        <>
            <OperationModelTray enhancement={enhancement} operation={operation} entry={entry} onChange={onChange} />

            <Separator />

            <div className="flex flex-col gap-2">
                <span className="text-[13px] text-foreground">{t("enhancements.restore.size")}</span>

                <ToggleGroup
                    type="single"
                    value={String(size)}
                    onValueChange={(next: string) => {
                        // Radix reports an empty string when the size already chosen is pressed again; a restore
                        // always runs at some size, so that deselection is ignored.
                        if (next) onChange(withParameter(operation, "scale", Number(next)));
                    }}
                    spacing={1}
                    aria-label={t("enhancements.restore.size")}
                    className="flex w-full rounded-lg bg-background p-1"
                >
                    {offered.map((candidate) => (
                        <ToggleGroupItem
                            key={candidate}
                            value={String(candidate)}
                            className={cn(SHORTCUT_ITEM, "flex-1")}
                        >
                            {t("enhancements.scale.shortcut", { scale: candidate })}
                        </ToggleGroupItem>
                    ))}
                </ToggleGroup>
            </div>

            <IntensityControl
                label={t("enhancements.strength")}
                // The model's own output where nothing knows an amount, which is what a new restore starts at.
                value={toPercent(parameterOf(operation, entry, "strength") ?? strengths?.default ?? 1)}
                {...(strengths && { min: strengths.min * PERCENT, max: strengths.max * PERCENT })}
                mark={100}
                onChange={(percent) => onChange(withParameter(operation, "strength", percent / PERCENT))}
            />

            {/* Shown only where the selected model publishes a grain, which both restore models do. */}
            {grains && (
                <div className="flex flex-col gap-1" data-slot="restore-grain">
                    <IntensityControl
                        label={t("enhancements.restore.grain")}
                        // Off where nothing knows an amount, which is what a new restore starts at.
                        value={toPercent(parameterOf(operation, entry, "grain") ?? grains.default)}
                        min={grains.min * PERCENT}
                        max={grains.max * PERCENT}
                        mark={0}
                        onChange={(percent) => onChange(withParameter(operation, "grain", percent / PERCENT))}
                    />
                    <p className="text-muted-foreground text-xs">{t("enhancements.restore.grainHint")}</p>
                </div>
            )}

            {/*
             * Always shown rather than behind the (i) marker: what this enhancement does differently from every other
             * one is not a detail of one model but the nature of the family, and it is the one thing a user should
             * not have to go looking for.
             */}
            <p className="flex gap-2 text-muted-foreground text-xs" data-slot="restore-generative">
                <Sparkles className="mt-px size-3.5 flex-none" aria-hidden />
                {t("enhancements.restore.generative")}
            </p>
        </>
    );
};
