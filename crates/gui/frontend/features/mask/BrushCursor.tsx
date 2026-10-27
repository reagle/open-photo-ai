import { type RefObject, useEffect, useRef, useState } from "react";
import { Minus } from "lucide-react";

type BrushCursorProps = {
    /** The pane the pointer is over, which is what the ring follows the pointer across. */
    container: RefObject<HTMLElement | null>;
    /**
     * The element the ring is drawn inside, where that is not the pane itself - the canvas, for a ring that is not to
     * be clipped with the pane. Its position is then measured against this one rather than the pane.
     */
    frame?: HTMLElement | null | undefined;
    /** The brush's radius in the pane's pixels, at the zoom the photograph is at. */
    radius: number;
    /** The brush's softness, `0..1`: the hard core is `1 - softness` of the radius. */
    softness: number;
    /** Whether the tool erases. Option or Alt held makes a stroke erase as well, and the ring shows it. */
    erase: boolean;
};

/**
 * The ring under the pointer that shows what a stroke would cover: the brush's radius, and inside it the hard core, in
 * the pane's own pixels - so it is the same size on the photograph at every zoom - with a minus sign while erasing.
 *
 * **Moved by the DOM, not by React.** The pointer reports sixty times a second and the ring has nothing else to say: a
 * render for each would be the whole pane redrawn for a circle. Only whether Option or Alt is held is state, because it
 * changes what is drawn.
 *
 * It takes no pointer events, and the system cursor is hidden over the pane by the pane itself while painting.
 */
export const BrushCursor = ({ container, frame, radius, softness, erase }: BrushCursorProps) => {
    const ring = useRef<HTMLDivElement>(null);
    const [held, setHeld] = useState(false);

    useEffect(() => {
        const element = container.current;
        if (!element) return;

        const moved = (event: PointerEvent) => {
            const rect = (frame ?? element).getBoundingClientRect();

            if (ring.current) {
                const [x, y] = [event.clientX - rect.left, event.clientY - rect.top];

                ring.current.style.transform = `translate(${x}px, ${y}px)`;
                ring.current.style.opacity = "1";
            }

            setHeld(event.altKey);
        };

        const left = () => {
            if (ring.current) ring.current.style.opacity = "0";
        };

        const modifier = (event: KeyboardEvent) => {
            if (event.key === "Alt") setHeld(event.type === "keydown");
        };

        // A window that loses the focus with Option held never hears the key come up, and would go on erasing.
        const blurred = () => setHeld(false);

        element.addEventListener("pointermove", moved);
        element.addEventListener("pointerleave", left);
        window.addEventListener("keydown", modifier);
        window.addEventListener("keyup", modifier);
        window.addEventListener("blur", blurred);

        return () => {
            element.removeEventListener("pointermove", moved);
            element.removeEventListener("pointerleave", left);
            window.removeEventListener("keydown", modifier);
            window.removeEventListener("keyup", modifier);
            window.removeEventListener("blur", blurred);
        };
    }, [container, frame]);

    const core = radius * (1 - softness);
    const erasing = erase || held;

    return (
        <div
            ref={ring}
            aria-hidden
            className="pointer-events-none absolute top-0 left-0 z-[2]"
            style={{ opacity: 0 }}
            data-slot="brush-cursor"
            data-erasing={erasing}
        >
            <div
                className="absolute flex items-center justify-center rounded-full border border-white shadow-[0_0_0_1px_rgb(0_0_0/0.6),inset_0_0_0_1px_rgb(0_0_0/0.6)]"
                style={{ width: radius * 2, height: radius * 2, left: -radius, top: -radius }}
                data-slot="brush-radius"
            >
                {core >= 2 && (
                    <div
                        className="absolute rounded-full border border-white/80 shadow-[0_0_0_1px_rgb(0_0_0/0.4)]"
                        style={{ width: core * 2, height: core * 2 }}
                        data-slot="brush-core"
                    />
                )}

                {erasing && <Minus className="size-3.5 text-white drop-shadow-[0_0_1px_rgb(0_0_0)]" />}
            </div>
        </div>
    );
};
