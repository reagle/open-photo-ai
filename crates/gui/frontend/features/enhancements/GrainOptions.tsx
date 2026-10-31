import { useTranslation } from "react-i18next";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import type { FamilyEntry } from "@/ipc/catalogue";
import type { Operation } from "@/ipc/enhance";
import { type Enhancement, parameterOf, publishedRange, withParameter } from "@/lib/enhancements";
import { toPercent } from "@/lib/utils";
import { IntensityControl } from "./IntensityControl";
import { OperationModelTray } from "./ModelTray";

/** How many percent one unit is: the wire speaks units, this panel speaks percent. */
const PERCENT = 100;

/**
 * Whether a grain's `match` switch, as the wire carries it, matches the source: the library reads every value from a
 * half up as matching, so this does too.
 */
export const matchesSource = (operation: Operation, entry: FamilyEntry | undefined): boolean =>
    (parameterOf(operation, entry, "match") ?? 1) >= 0.5;

type GrainOptionsProps = {
    /** The enhancement this panel is open over, which every control here reports and writes. */
    enhancement: Enhancement;
    /** The grain itself, as the stack carries it. */
    operation: Operation;
    /** What the catalogue publishes for grain: its one model and its bounds. */
    entry: FamilyEntry | undefined;
    /** Writes the enhancement back to the stack, which is what re-runs the preview. */
    onChange: (operation: Operation) => void;
};

/**
 * What a grain can be set to: an Amount, as Lightroom's Grain has, and whether that amount is a share of the
 * photograph's own noise the enhancements smoothed away ("Match source", on by default) or of a sensor's grain added
 * everywhere - with the one sentence that says which.
 *
 * **Nothing is written on open**, for `UpscaleOptions`' reason: writing the stack asks for a new run.
 *
 * The amount's range is the published one, 0 to 100%; the switch is published as a range from 0 (add) to 1 (match),
 * and drawn as the switch it is. The library measures and draws the grain; this panel only says how much and of what.
 */
export const GrainOptions = ({ enhancement, operation, entry, onChange }: GrainOptionsProps) => {
    const { t } = useTranslation();

    const amounts = publishedRange(entry, operation.codename, "amount");
    const matching = matchesSource(operation, entry);

    return (
        <>
            <OperationModelTray enhancement={enhancement} operation={operation} entry={entry} onChange={onChange} />

            <Separator />

            <IntensityControl
                label={t("enhancements.grain.amount")}
                // All of it where nothing knows an amount, which is what a new grain starts at.
                value={toPercent(parameterOf(operation, entry, "amount") ?? amounts?.default ?? 1)}
                {...(amounts && { min: amounts.min * PERCENT, max: amounts.max * PERCENT })}
                mark={100}
                onChange={(percent) => onChange(withParameter(operation, "amount", percent / PERCENT))}
            />

            <div className="flex flex-col gap-1" data-slot="grain-match">
                <label htmlFor="grain-match" className="flex items-center justify-between">
                    <span className="text-[13px]">{t("enhancements.grain.match")}</span>
                    <Switch
                        id="grain-match"
                        checked={matching}
                        onCheckedChange={(checked: boolean) =>
                            onChange(withParameter(operation, "match", checked ? 1 : 0))
                        }
                        // Both states, for the reason the sidebar's Autopilot switch gives.
                        className="data-[state=unchecked]:bg-input data-[state=checked]:bg-primary"
                    />
                </label>
                <p className="text-muted-foreground text-xs" data-slot="grain-hint">
                    {t(matching ? "enhancements.grain.matchHint" : "enhancements.grain.addHint")}
                </p>
            </div>
        </>
    );
};
