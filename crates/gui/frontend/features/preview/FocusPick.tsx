import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useFocusPickStore } from "@/stores/focusPick";

/** How long the ring stays after a click has set the focus, in milliseconds. */
export const RING_MS = 2000;

/**
 * How far a pointer may move between going down and coming up for the press to be a click rather than a drag, in
 * pixels.
 */
export const CLICK_SLOP = 4;

/** Where the photograph is drawn in a pane, in the pane's own pixels: what the ring's percentages are of. */
export type PhotographBox = { x: number; y: number; width: number; height: number };

type FocusPickProps = {
    /** Whether the preview is waiting for a click, which is when the ring shows the current focus. */
    armed: boolean;
    /** The lens blur's focus, as fractions across and down the framed photograph, or absent with no lens blur. */
    focus?: { x: number; y: number };
    /** Where the pane draws the photograph, which is the space `focus` is a share of. */
    box: PhotographBox;
};

/**
 * The ring that marks where a lens blur is focused: while the preview waits for a click, and for a couple of seconds
 * after one has landed.
 *
 * **Positioned in percentages of the framed photograph's drawn box**, as `FaceBoxes` is, so nothing is measured and the
 * ring lands on the same point of the photograph at every zoom. The box is the pane's - where it draws the photograph
 * at the zoom it is at - rather than a transform of the picture, so the ring keeps one size however far the photograph
 * is magnified.
 *
 * It takes no pointer events: the pane under it pans, zooms and takes the click.
 */
export const FocusPick = ({ armed, focus, box }: FocusPickProps) => {
    const picks = useFocusPickStore((state) => state.picks);

    // Whether a click landed in the last couple of seconds. The count is compared with the one this mounted at, so
    // a pane drawn after a pick - another photograph - does not flash for it.
    const seen = useRef(picks);
    const [recent, setRecent] = useState(false);

    useEffect(() => {
        if (picks === seen.current) return;

        seen.current = picks;
        setRecent(true);

        const timer = setTimeout(() => setRecent(false), RING_MS);

        return () => clearTimeout(timer);
    }, [picks]);

    if (!focus || box.width <= 0 || box.height <= 0 || !(armed || recent)) return;

    return (
        <div
            className="pointer-events-none absolute"
            style={{ left: box.x, top: box.y, width: box.width, height: box.height }}
            data-slot="focus-pick"
        >
            {/*
             * Hidden from assistive technology: `FocusHint`, a status line, says what the mode is and how to leave it,
             * and a ring an assistive technology could name would say nothing it can act on. There is no keyboard way
             * to place the focus - choosing a point is a pointer's job - so Escape is the keyboard's one part in it.
             */}
            <div
                aria-hidden
                className="absolute size-9 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-warning shadow-[0_0_0_1.5px_rgb(0_0_0/0.6)]"
                style={{ left: `${focus.x * 100}%`, top: `${focus.y * 100}%` }}
                data-slot="focus-ring"
            >
                <span className="absolute top-1/2 left-1/2 size-1 -translate-x-1/2 -translate-y-1/2 rounded-full bg-warning" />
            </div>
        </div>
    );
};

/**
 * The line that says what the next click does, over the top of the canvas while the preview is armed.
 *
 * Once for the canvas rather than once per pane, so it is not clipped away by the split comparison's divider and is
 * not announced twice by side by side. `role="status"` so a user who cannot see it is told. It takes no pointer events.
 */
export const FocusHint = () => {
    const { t } = useTranslation();

    return (
        <span
            role="status"
            className="pointer-events-none absolute top-3 left-1/2 z-[4] max-w-[90%] -translate-x-1/2 rounded-full border border-border bg-background/85 px-3 py-1 text-center text-xs"
            data-slot="focus-hint"
        >
            {t("enhancements.lensBlur.focusHint")}
        </span>
    );
};
