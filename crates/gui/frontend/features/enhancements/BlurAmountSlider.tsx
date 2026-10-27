import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Slider } from "@/components/ui/slider";
import { fNumberForAmount, readoutFNumber, realFNumber } from "@/lib/blurAmount";

type BlurAmountSliderProps = {
    /** The amount the enhancement carries, 0 to 100, which the slider shows. */
    value: number;
    /** The photograph's own equivalent f-number, which every amount is an aperture relative to. */
    own: number;
    /** The photograph's crop factor as framed, where known, which turns the equivalent f-number into the real one. */
    cropFactor: number | null;
    /** Whether the slider is drawn and unavailable: the lens is not known yet, or there is no blur to add. */
    disabled?: boolean;
    /** Called with the amount the slider was released on, or moved to by a key. */
    onChange: (amount: number) => void;
};

/**
 * The blur as a slider from 0, the photograph as taken, to 100, as much as the lens goes to, with the amount beside the
 * label and the lens it corresponds to beneath it: `≈ f/2.8 (f/4.2 full-frame)`.
 *
 * **The amount is a place on the slider and the f-number is what it writes**: see `lib/blurAmount.ts`, which lays the
 * amounts out evenly in blur between the photograph's own aperture and f/1.4. The readout says what the lens would be
 * set to - the real f-number first, as the camera would show it, then the full-frame equivalent the maths is done in -
 * and where the crop is not known it says the equivalent alone, since no real aperture can be claimed.
 *
 * **The slider writes when it is released**, as `IntensityControl`'s does and for the same reason: every write re-runs
 * the preview, and this one re-renders the whole photograph at its own size. The amount it is dragged through is held
 * here and read out beside the label, the readout following it.
 */
export const BlurAmountSlider = ({ value, own, cropFactor, disabled = false, onChange }: BlurAmountSliderProps) => {
    const { t } = useTranslation();

    // Where the thumb is while it is being dragged, and nothing once it has been released.
    const [dragged, setDragged] = useState<number>();

    const shown = dragged ?? value;

    // What the lens is opened to at an amount, in the two terms: the equivalent, and the real one where the crop says.
    const readout = (amount: number) => {
        const equivalent = fNumberForAmount(amount, own);
        const real = realFNumber(equivalent, cropFactor);

        return real === null
            ? t("enhancements.lensBlur.readoutEquivalent", { equivalent: readoutFNumber(equivalent) })
            : t("enhancements.lensBlur.readout", {
                  real: readoutFNumber(real),
                  equivalent: readoutFNumber(equivalent),
              });
    };

    // What a screen reader is told of the thumb: the amount and the lens, and for 0 what it is, as the label beneath
    // the track does for the eye, which that label's `aria-hidden` keeps from being read.
    const spoken = (amount: number) =>
        amount === 0 && !disabled ? `0, ${t("enhancements.lensBlur.asTaken")}` : `${amount}, ${readout(amount)}`;

    return (
        <div className="flex flex-col gap-3" data-slot="blur-amount-slider">
            <div className="flex items-start justify-between gap-2">
                <span className="flex flex-col gap-0.5">
                    <span className="text-[13px] text-foreground">{t("enhancements.lensBlur.blurAmount")}</span>
                    <span className="text-[11px] text-foreground-dim" data-slot="blur-amount-readout">
                        {readout(shown)}
                    </span>
                </span>

                <span className="font-mono text-[13px] text-foreground" data-slot="blur-amount-value">
                    {shown}
                </span>
            </div>

            <div className="relative mx-1 mb-[23px]">
                <Slider
                    min={0}
                    max={100}
                    step={1}
                    value={[shown]}
                    disabled={disabled}
                    thumbLabel={t("enhancements.lensBlur.blurAmount")}
                    thumbValueText={spoken}
                    onValueChange={([next]) => next !== undefined && setDragged(next)}
                    onValueCommit={([next]) => {
                        setDragged(undefined);

                        // A write asks for a new run, so a release on the amount it is already at writes nothing.
                        if (next !== undefined && next !== value) onChange(next);
                    }}
                />

                {!disabled && (
                    <span
                        aria-hidden
                        className="pointer-events-none absolute top-[calc(100%+8px)] left-0 text-[10px] text-foreground-faint"
                    >
                        {t("enhancements.lensBlur.asTaken")}
                    </span>
                )}
            </div>
        </div>
    );
};
