import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Slider } from "@/components/ui/slider";
import { formatFNumber, nearestStop } from "@/lib/fstops";

type FStopSliderProps = {
    /** The stops the slider has, widest aperture first, as `stopsUpTo` answers: the last is the photograph's own. */
    stops: readonly number[];
    /** The f-number the enhancement carries, which the slider shows at the stop nearest it. */
    value: number;
    /** Whether the stops are not known yet: the slider is drawn, and unavailable. */
    disabled?: boolean;
    /** Called with the nominal f-number of the stop the slider was released on, or moved to by a key. */
    onChange: (fNumber: number) => void;
};

/**
 * The f-number as a slider in third stops, widest aperture at the left and "As taken" - the photograph's own - at the
 * right, with the f-number it is at beside the label and what it is in terms of beneath it.
 *
 * **Its value is the stop's index and what it writes is the stop's own number**, so one step of the control is always
 * a third of a stop and f/1.4 is 1.4 on the wire, not 1.41. See `lib/fstops.ts`.
 *
 * **The slider writes when it is released**, as `IntensityControl`'s does and for the same reason: every write re-runs
 * the preview, and this one re-renders the whole photograph at its own size. The stop it is dragged through is held
 * here and read out beside the label.
 *
 * It starts at, and a value above it is drawn at, "As taken": an f-number at or above the photograph's own adds no
 * blur, so the stop of the right end and anything wider-numbered are the same picture.
 */
export const FStopSlider = ({ stops, value, disabled = false, onChange }: FStopSliderProps) => {
    const { t } = useTranslation();

    const index = nearestStop(stops, value);

    // Where the thumb is while it is being dragged, and nothing once it has been released.
    const [dragged, setDragged] = useState<number>();

    const unavailable = disabled || stops.length < 2;
    const shown = dragged ?? index;
    const f = (at: number) => `f/${formatFNumber(stops[at] ?? value)}`;
    // What a screen reader is told of the thumb: the right end says what it is, as the label beneath the track does for
    // the eye, which that label's `aria-hidden` keeps from being read.
    const spoken = (at: number) =>
        !unavailable && at === stops.length - 1 ? `${f(at)}, ${t("enhancements.lensBlur.asTaken")}` : f(at);

    return (
        <div className="flex flex-col gap-3" data-slot="f-stop-slider">
            <div className="flex items-start justify-between gap-2">
                <span className="flex flex-col gap-0.5">
                    <span className="text-[13px] text-foreground">{t("enhancements.lensBlur.fNumber")}</span>
                    <span className="text-[11px] text-foreground-dim">{t("enhancements.lensBlur.equivalent")}</span>
                </span>

                <span className="font-mono text-[13px] text-foreground" data-slot="f-stop-value">
                    {f(shown)}
                </span>
            </div>

            <div className="relative mx-1 mb-[23px]">
                <Slider
                    min={0}
                    max={Math.max(stops.length - 1, 1)}
                    step={1}
                    value={[shown]}
                    disabled={unavailable}
                    thumbLabel={t("enhancements.lensBlur.fNumber")}
                    thumbValueText={spoken}
                    onValueChange={([next]) => next !== undefined && setDragged(next)}
                    onValueCommit={([next]) => {
                        setDragged(undefined);

                        const stop = next === undefined ? undefined : stops[next];
                        // A write asks for a new run, so a release on the stop it is already at writes nothing.
                        if (stop !== undefined && next !== index) onChange(stop);
                    }}
                />

                {!unavailable && (
                    <span
                        aria-hidden
                        className="pointer-events-none absolute top-[calc(100%+8px)] right-0 text-[10px] text-foreground-faint"
                    >
                        {t("enhancements.lensBlur.asTaken")}
                    </span>
                )}
            </div>
        </div>
    );
};
