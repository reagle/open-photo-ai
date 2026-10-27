import { Crosshair } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { PopoverClose } from "@/components/ui/popover";
import { Separator } from "@/components/ui/separator";
import { useLensInfo } from "@/hooks/useLensInfo";
import type { FamilyEntry } from "@/ipc/catalogue";
import type { Operation } from "@/ipc/enhance";
import type { LensInfo } from "@/ipc/lens";
import { type Enhancement, F_NUMBER, parameterOf, withParameter } from "@/lib/enhancements";
import { ASSUMED_F_NUMBER, F_STOPS, formatFNumber, stopsUpTo } from "@/lib/fstops";
import { useImageCrop } from "@/stores/crop";
import { useFocusPickStore } from "@/stores/focusPick";
import { FStopSlider } from "./FStopSlider";
import { OperationModelTray } from "./ModelTray";

/**
 * What a photograph the lens of which could not be asked about is taken to be: the f/8 the library assumes for a
 * photograph with no lens information, so the panel and a run agree on it.
 */
const ASSUMED: LensInfo = { ownFNumber: ASSUMED_F_NUMBER, focal35mm: null, origin: "assumed" };

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
    /** Writes the enhancement back to the stack, which is what re-runs the preview. */
    onChange: (operation: Operation) => void;
};

/**
 * What a lens blur can be set to: which model measures the depth, how wide the aperture is, and where the focus falls.
 *
 * **The slider is bounded by the photograph.** Its right end is the photograph's own equivalent f-number, from its
 * EXIF, which is where a lens blur adds nothing; until that has been asked for the slider is drawn and unavailable. A
 * photograph with no lens information is taken at the f/8 the library assumes, and the panel says so.
 *
 * **Set focus closes the panel and arms the preview**, which then takes the next click on the photograph as the point.
 * The panel is where it is chosen and the photograph is where the point is, so the two cannot be one surface - see
 * `features/preview/FocusPick.tsx`.
 *
 * **Nothing is written on open.** The slider writes when released and the button writes nothing at all, so opening the
 * panel to see what a lens blur is set to never cancels the run in flight.
 */
export const LensBlurOptions = ({ enhancement, operation, entry, identity, onChange }: LensBlurOptionsProps) => {
    const { t } = useTranslation();

    // As the photograph is framed: a crop is a smaller sensor, and its own f-number is the frame's times the framing.
    const crop = useImageCrop(identity);
    const asked = useLensInfo(identity, crop);

    // A failed question is the assumed lens, which is what a photograph the library knows nothing about is.
    const lens = asked === null ? ASSUMED : asked;
    const stops = lens ? stopsUpTo(lens.ownFNumber) : F_STOPS;

    const fNumber = parameterOf(operation, entry, F_NUMBER) ?? lens?.ownFNumber ?? ASSUMED_F_NUMBER;

    return (
        <>
            <OperationModelTray enhancement={enhancement} operation={operation} entry={entry} onChange={onChange} />

            <Separator />

            <FStopSlider
                stops={stops}
                value={fNumber}
                disabled={lens === undefined}
                onChange={(stop) => onChange(withParameter(operation, F_NUMBER, stop))}
            />

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
        </>
    );
};
