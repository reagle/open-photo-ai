import { useEffect, useRef, useState } from "react";
import { type Mask, maskPreview, type Stroke } from "@/ipc/mask";
import { isEmpty, pngDataUrl, previewSize, refineMasks, type Snapshot, strokesAfter, toWire } from "@/lib/mask";
import { report } from "@/lib/report";
import { cn } from "@/lib/utils";
import { type MaskEntry, type Target, useEditingTarget, useFileMask, useMaskStore } from "@/stores/mask";

/** Where the photograph is drawn in a pane, in the pane's own pixels: what the overlay is laid over. */
export type PhotographBox = { x: number; y: number; width: number; height: number };

/**
 * The picture of `entry`'s weight, as a data URL, asked of Rust at `width` x `height`.
 *
 * **At most one request is in flight, and the newest wins.** A request made while one is out waits for it, so a quick
 * run of strokes costs one picture at the end of it and one in the middle, never a queue. A result that is no longer
 * the newest is still shown when it is the same mask with fewer strokes on the end - the strokes it lacks are drawn
 * over it as they were drawn, so the tint does not blink out while the next picture is made - and dropped on arrival
 * when it is not, an undo or a clear having made it a picture of something else. An entry that paints nothing asks for
 * nothing and has no picture.
 *
 * Answers the picture with the entry it is of, so a caller can tell what it shows from what has been painted since.
 *
 * `wire` is what of the entry is drawn: the entry itself, or for a refinement one of its two kinds of stroke.
 */
const usePreview = (
    entry: MaskEntry | undefined,
    width: number,
    height: number,
    wire: (entry: MaskEntry) => Mask | undefined = toWire,
) => {
    const [shown, setShown] = useState<{ url: string; of: MaskEntry }>();

    // What is wanted now, and what the request last started was for. Refs, because the request that settles has to
    // read what is wanted *then* and not what was when it began.
    const wanted = useRef<{ entry: MaskEntry; mask: Mask; width: number; height: number }>(undefined);
    const started = useRef<typeof wanted.current>(undefined);
    const busy = useRef(false);
    const mounted = useRef(true);

    useEffect(() => {
        mounted.current = true;

        return () => {
            mounted.current = false;
        };
    }, []);

    useEffect(() => {
        const mask = entry && wire(entry);

        if (!entry || !mask || width <= 0 || height <= 0) {
            wanted.current = undefined;
            setShown(undefined);

            return;
        }

        wanted.current = { entry, mask, width, height };

        const pump = () => {
            const job = wanted.current;
            if (busy.current || !job || job === started.current) return;

            busy.current = true;
            started.current = job;

            maskPreview(job.mask, job.width, job.height)
                .then(
                    (bytes) => {
                        // Superseded while it was out: kept for as long as it is a stale picture of the same mask, and
                        // not when what arrives is of a mask that is no longer the one painted.
                        const current = wanted.current;
                        if (mounted.current && current && strokesAfter(job.entry, current.entry) !== undefined)
                            setShown({ url: pngDataUrl(bytes), of: job.entry });
                    },
                    (error: unknown) => report("drawing the mask failed", error),
                )
                .finally(() => {
                    busy.current = false;
                    if (mounted.current) pump();
                });
        };

        pump();
    }, [entry, width, height, wire]);

    return shown;
};

/**
 * The device's pixel density, which the picture asked for is sized by. Not tracked: a window moved between displays
 * asks again at its next change.
 */
const density = () => (typeof window === "undefined" ? 1 : window.devicePixelRatio || 1);

/** `stroke` as the SVG path its pointer drew, over a photograph `width` x `height` pixels across. */
const pathOf = (stroke: Stroke, width: number, height: number) =>
    stroke.points.reduce((path, value, at) => {
        if (at % 2 === 1) return path;

        const [x, y] = [value * width, (stroke.points[at + 1] ?? 0) * height];

        return `${path}${at === 0 ? "M" : "L"}${x.toFixed(1)} ${y.toFixed(1)}`;
    }, "");

/**
 * Strokes the picture on show does not have yet, drawn as the pointer drew them.
 *
 * A stroke is drawn as what it does to the *tint*, which is the opposite of what it does to the strokes under a mask
 * that is turned over: painting adds weight to what is then inverted, so it takes the tint away.
 */
const Strokes = ({
    strokes,
    box,
    radius,
    inverted,
    target,
}: {
    strokes: Stroke[];
    box: PhotographBox;
    radius: number;
    inverted: boolean;
    target: Target;
}) => (
    <svg
        aria-hidden
        className="pointer-events-none absolute inset-0 size-full overflow-visible"
        viewBox={`0 0 ${box.width} ${box.height}`}
        data-slot="mask-strokes"
    >
        {strokes.map((stroke, index) => {
            const [x, y] = stroke.points;
            const takesAway = stroke.erase !== inverted;

            return (
                <g
                    // A stroke has no identity but its place in the list, which is where it was drawn.
                    // biome-ignore lint/suspicious/noArrayIndexKey: the index IS this list's identity
                    key={index}
                    // Taking the tint away is drawn as a pale trace, not as a removal: the picture under it has not
                    // changed yet.
                    className={takesAway ? "text-foreground" : strokeTint(target, stroke)}
                    opacity={takesAway ? 0.3 : 0.4}
                >
                    {stroke.points.length <= 2 ? (
                        <circle
                            cx={(x ?? 0) * box.width}
                            cy={(y ?? 0) * box.height}
                            r={stroke.radius * radius}
                            fill="currentColor"
                        />
                    ) : (
                        <path
                            d={pathOf(stroke, box.width, box.height)}
                            fill="none"
                            stroke="currentColor"
                            strokeWidth={2 * stroke.radius * radius}
                            strokeLinecap="round"
                            strokeLinejoin="round"
                        />
                    )}
                </g>
            );
        })}
    </svg>
);

/**
 * Each region's colour: the theme's primary for the limiting mask, `--removal`'s magenta for what Remove takes out, so
 * the two painted with one brush are never mistaken for each other. The removal's tint is lighter, since what is under
 * it is the fill the user is judging.
 */
const TINT = {
    limit: { fill: "bg-primary/40", stroke: "text-primary" },
    removal: { fill: "bg-removal/35", stroke: "text-removal" },
    // The refinement's focus; its blur is `BLUR_TINT`. Light, as the removal's is: what is under it is being judged.
    refine: { fill: "bg-refine-focus/35", stroke: "text-refine-focus" },
} as const satisfies Record<Target, { fill: string; stroke: string }>;

/** The tint of a refinement's blur strokes, beside {@link TINT}'s focus. */
const BLUR_TINT = { fill: "bg-refine-blur/35", stroke: "text-refine-blur" } as const;

/** The colour a stroke of `target` is drawn in: a refinement's by which of the two it paints. */
const strokeTint = (target: Target, stroke: Stroke) =>
    target === "refine" && stroke.blur ? BLUR_TINT.stroke : TINT[target].stroke;

/** What of a refinement each of its two tints is drawn from: Rust's own two masks of it, or nothing where it has none. */
const focusWire = (entry: Snapshot) =>
    entry.strokes.some((stroke) => !stroke.erase && !stroke.blur) ? toWire(refineMasks(entry).focus) : undefined;
const blurWire = (entry: Snapshot) =>
    entry.strokes.some((stroke) => !stroke.erase && stroke.blur) ? toWire(refineMasks(entry).blur) : undefined;

/** A tint shaped by the picture at `url`, filling the box it is laid in. */
const Tint = ({ url, fill }: { url: string; fill: string }) => (
    <div
        aria-hidden
        className={cn("absolute inset-0", fill)}
        style={{
            maskImage: `url(${url})`,
            WebkitMaskImage: `url(${url})`,
            maskSize: "100% 100%",
            WebkitMaskSize: "100% 100%",
            maskRepeat: "no-repeat",
            WebkitMaskRepeat: "no-repeat",
        }}
        data-slot="mask-tint"
    />
);

type MaskOverlayProps = {
    /** The photograph whose mask is shown, by the path the store keys it by. */
    path: string;
    /** Where the pane draws the photograph, which is the box the tint is laid over. */
    box: PhotographBox;
};

/**
 * The tint over the part of the photograph the mask covers, for the pane that draws the enhanced picture.
 *
 * **The tint is the weight Rust will use**, not the strokes redrawn: the picture it is shaped by is `mask_preview`'s,
 * asked for the box as it is drawn, in device pixels. Until a new one arrives, what was just painted shows as the
 * pointer drew it - so a stroke does not vanish for the length of a round trip - and the stroke under the pointer is
 * drawn the same way.
 *
 * **Laid over the drawn box, not the photograph's transform**, as the focus pick's ring and `FaceBoxes` are: the box is
 * the pane's own position and size of the photograph, so the tint is the same shape at every zoom. It takes no pointer
 * events: the pane under it paints, pans and zooms.
 */
export const MaskOverlay = ({ path, box }: MaskOverlayProps) => {
    // The region being painted: the overlay is drawn only while one is, and shows that one alone.
    const target = useEditingTarget(path) ?? "limit";
    const entry = useFileMask(path, target);
    const live = useMaskStore((state) =>
        state.live?.path === path && state.live.target === target ? state.live.stroke : undefined,
    );

    const wanted = previewSize(box.width, box.height, density());
    const refine = target === "refine";
    // A refinement is drawn as two tints, focus and blur, each from its own picture; anything else as one.
    const shown = usePreview(entry, wanted?.width ?? 0, wanted?.height ?? 0, refine ? focusWire : toWire);
    const blurred = usePreview(refine ? entry : undefined, wanted?.width ?? 0, wanted?.height ?? 0, blurWire);

    // What is painted that the picture on show is not of: the strokes added since, where it is the same mask with more
    // on the end. An undo, a clear or an invert has no such stroke to show, and waits for its picture. Of a
    // refinement's two pictures, the older says.
    const behind = refine && blurred && shown && blurred.of.version < shown.of.version ? blurred : shown;
    const pending = strokesAfter(refine ? (behind ?? blurred)?.of : shown?.of, entry) ?? [];

    if (box.width <= 0 || box.height <= 0) return;

    // The brush's radius in the box's own pixels is a fraction of its longer side.
    const longer = Math.max(box.width, box.height);
    const strokes = live ? [...pending, live] : pending;
    const picture = isEmpty(entry) ? undefined : shown?.url;
    const blurPicture = isEmpty(entry) ? undefined : blurred?.url;

    return (
        <div
            // Placed by a transform on a layer of its own, as the photograph is - see `PreviewImage`: WKWebView leaves
            // stale tiles behind a repainted `left` and `top`, and a pan moves this every frame.
            className="pointer-events-none absolute top-0 left-0"
            style={{
                width: box.width,
                height: box.height,
                transform: `translate(${box.x}px, ${box.y}px)`,
                willChange: "transform",
            }}
            data-slot="mask-overlay"
            data-target={target}
        >
            {picture && <Tint url={picture} fill={TINT[target].fill} />}
            {refine && blurPicture && <Tint url={blurPicture} fill={BLUR_TINT.fill} />}

            {strokes.length > 0 && (
                <Strokes
                    strokes={strokes}
                    box={box}
                    radius={longer}
                    inverted={entry?.inverted ?? false}
                    target={target}
                />
            )}
        </div>
    );
};
