import { Brush, Crosshair } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { PopoverClose } from "@/components/ui/popover";
import { Separator } from "@/components/ui/separator";
import { useLensInfo } from "@/hooks/useLensInfo";
import type { FamilyEntry } from "@/ipc/catalogue";
import type { Operation } from "@/ipc/enhance";
import { ASSUMED_LENS, canBlurMore, fNumberForAmount, formatFNumber, shownAmount } from "@/lib/blurAmount";
import { type Enhancement, F_NUMBER, parameterOf, withParameter } from "@/lib/enhancements";
import { useImageCrop } from "@/stores/crop";
import { useFocusPickStore } from "@/stores/focusPick";
import { useHasMask, useMaskStore } from "@/stores/mask";
import { BlurAmountSlider } from "./BlurAmountSlider";
import { OperationModelTray } from "./ModelTray";

type LensBlurOptionsProps = {
    /** The enhancement this panel is open over, which every control here reports and writes. */
    enhancement: Enhancement;
    /** The lens blur itself, as the stack carries it. */
    operation: Operation;
    /** What the catalogue publishes for lens blur: its model, its precision and its bounds. */
    entry: FamilyEntry | undefined;
    /**
     * The photograph this panel is open over, which is what its lens is asked of and what Set focus arms the preview
     * for. Absent for a file whose bytes could not be read, which has no pixels for either.
     */
    identity?: string;
    /** The photograph's path, which the Refine brush's strokes are kept by. */
    path: string;
    /** Writes the enhancement back to the stack, which is what re-runs the preview. */
    onChange: (operation: Operation) => void;
};

/**
 * What a lens blur can be set to: which model measures the depth, how much blur there is, and where the focus falls.
 *
 * **The slider is a blur amount, 0 to 100, against the photograph.** 0 is the photograph as taken - its own equivalent
 * f-number, from its EXIF, which is where a lens blur adds nothing - and 100 is f/1.4; the f-number the amount stands
 * for is what the operation carries (`lib/blurAmount.ts`). Until the lens has been asked for the slider is drawn and
 * unavailable, and so it is for a photograph taken at f/1.4 or wider, which has no more blur to add, with a note saying
 * so. A photograph with no lens information is taken at the f/8 the library assumes, and the panel says so.
 *
 * **Set focus closes the panel and arms the preview**, which then takes the next click on the photograph as the point.
 * The panel is where it is chosen and the photograph is where the point is, so the two cannot be one surface - see
 * `features/preview/FocusPick.tsx`.
 *
 * **Refine closes the panel and opens the brush** over the photograph, in the sidebar's mask section, as the area painted
 * for Remove is: Focus over what should stay sharp (stray hairs the depth missed), Blur over what should not (a patch
 * of background at the subject's distance), and Erase. The strokes are kept beside the stack, with their own undo, and
 * sent with the Lens Blur; the button says Edit refinement once something is painted.
 *
 * **Nothing is written on open.** The slider writes when released and the button writes nothing at all, so opening the
 * panel to see what a lens blur is set to never cancels the run in flight.
 */
export const LensBlurOptions = ({ enhancement, operation, entry, identity, path, onChange }: LensBlurOptionsProps) => {
    const { t } = useTranslation();
    const refined = useHasMask(path, "refine");

    // As the photograph is framed: a crop is a smaller sensor, and its own f-number is the frame's times the framing.
    const crop = useImageCrop(identity);
    const asked = useLensInfo(identity, crop);

    // A failed question is the assumed lens, which is what a photograph the library knows nothing about is.
    const lens = asked === null ? ASSUMED_LENS : asked;
    const own = (lens ?? ASSUMED_LENS).ownFNumber;
    const blurrable = canBlurMore(own);

    // The amount is not stored: it is read back from the f-number the operation carries, against the photograph's own.
    const fNumber = parameterOf(operation, entry, F_NUMBER) ?? own;
    const amount = shownAmount(fNumber, own);

    return (
        <>
            <OperationModelTray enhancement={enhancement} operation={operation} entry={entry} onChange={onChange} />

            <Separator />

            <BlurAmountSlider
                value={amount}
                own={own}
                cropFactor={lens?.cropFactor ?? null}
                disabled={lens === undefined || !blurrable}
                onChange={(next) => onChange(withParameter(operation, F_NUMBER, fNumberForAmount(next, own)))}
            />

            {lens !== undefined && !blurrable && (
                <p className="text-[12px] text-muted-foreground leading-normal" data-slot="lens-cannot-blur-more">
                    {t("enhancements.lensBlur.cannotBlurMore", { f: formatFNumber(own) })}
                </p>
            )}

            {lens?.origin === "assumed" && (
                <p className="text-[12px] text-muted-foreground leading-normal" data-slot="lens-assumed">
                    {t("enhancements.lensBlur.assumed", { f: formatFNumber(lens.ownFNumber) })}
                </p>
            )}

            <Separator />

            {/*
             * Closes the panel through the popover's own close, and arms the preview for this photograph as it does.
             * A button for a pointer's job: choosing the point needs a click on the photograph, which a keyboard has
             * no equivalent of here.
             */}
            <PopoverClose asChild>
                <Button
                    variant="secondary"
                    className="h-9 border border-input font-normal text-[13px]"
                    disabled={identity === undefined}
                    onClick={() => identity !== undefined && useFocusPickStore.getState().arm(identity)}
                >
                    <Crosshair className="size-4" />
                    {t("enhancements.lensBlur.setFocus")}
                </Button>
            </PopoverClose>

            {/* Beside Set focus, and the same way out: the brush is painted on the photograph, not in the panel. */}
            <PopoverClose asChild>
                <Button
                    variant="secondary"
                    className="h-9 border border-input font-normal text-[13px]"
                    disabled={identity === undefined}
                    onClick={() => useMaskStore.getState().startEditing(path, "refine")}
                    data-slot="lens-refine"
                >
                    <Brush className="size-4" />
                    {refined ? t("enhancements.lensBlur.editRefine") : t("enhancements.lensBlur.refine")}
                </Button>
            </PopoverClose>
        </>
    );
};
