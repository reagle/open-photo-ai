import { type CSSProperties, type PointerEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ChevronsLeftRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { BrushCursor } from "@/features/mask/BrushCursor";
import { MaskOverlay } from "@/features/mask/MaskOverlay";
import { useEnhancementRun } from "@/hooks/useEnhancementRun";
import { useResizeObserver } from "@/hooks/useResizeObserver";
import { useStandIn } from "@/hooks/useStandIn";
import { framedDimensions } from "@/ipc/crop";
import { resultUrl } from "@/ipc/images";
import { DRAWER_HEIGHT, PAN_KEY_STEP, ZOOM_MIN, ZOOM_WHEEL_STEP } from "@/lib/constants";
import { FOCUS_X, FOCUS_Y, withParameter } from "@/lib/enhancements";
import { OPEN_LAYER } from "@/lib/layers";
import { onMac, radiusOf } from "@/lib/mask";
import { cn } from "@/lib/utils";
import { useImageCrop } from "@/stores/crop";
import { useDrawerStore } from "@/stores/drawer";
import { useEnhancementStore, useFileEnhancements } from "@/stores/enhancements";
import { useCurrentFile } from "@/stores/files";
import { useFocusPickStore, useIsArmed } from "@/stores/focusPick";
import { useIsEditing, useMaskStore } from "@/stores/mask";
import { usePeekStore } from "@/stores/peek";
import { usePreviewStore } from "@/stores/preview";
import {
    clampScale,
    type ImageTransform,
    type PanDirection,
    registerPanner,
    useImageTransform,
    useTransformStore,
} from "@/stores/transform";
import { CLICK_SLOP, FocusHint, FocusPick } from "./FocusPick";
import { constrain, constrainPosition, fitInside, imageFraction, type Size } from "./geometry";
import { ProgressBar, useShownProgress } from "./ProgressBar";

/**
 * What a pane does with a press while the mask is being painted: the brush, and what to tell the store as a stroke
 * begins, moves and ends. Present only while a primary press paints - not while Space is held, which pans.
 *
 * Positions are fractions across and down the framed photograph, clamped to it.
 */
type Paint = {
    /** The brush's diameter as a fraction of the longer side, which the ring under the pointer is drawn from. */
    size: number;
    /** The brush's softness, `0..1`. */
    softness: number;
    /** Whether the tool is Erase, as opposed to Option or Alt held for one stroke. */
    erase: boolean;
    /** A stroke begins at a point of the photograph; `alt` is whether Option or Alt was held as it went down. */
    onBegin: (x: number, y: number, alt: boolean) => void;
    onMove: (x: number, y: number) => void;
    onEnd: (x: number, y: number) => void;
    /** The press was taken away by the system: no stroke. */
    onCancel: () => void;
};

type PaneProps = {
    // Handed down rather than read from the store here, so that both panes draw the same photograph on
    // the same render and one stand-in serves the pair. See `hooks/useStandIn.ts`.
    //
    // The dimensions alone rather than a whole `ImageRecord`, because the enhanced pane's result is
    // not one: it has no path, no format and no size on disk, and a record built by swapping the
    // result's identity and dimensions into the source's would carry the source's answers for
    // everything else - so anything this component read off it would be reading the wrong image's
    // data. Taking what is actually used means there is nothing to fabricate.
    /**
     * The photograph's dimensions. Partial because an unparseable header publishes neither;
     * the decoded image's own size is the fallback.
     */
    published?: Partial<Size>;
    /** Where the pixels come from, or `undefined` if the file's bytes could not be read. */
    source?: string;
    /** The image's alt text. */
    alt: string;
    /** This pane's label, in the user's language. */
    label: string;
    /** How the pane is placed: a flex half, or the clipped overlay in split mode. */
    className?: string;
    /** Which corner the chip sits in, and how loud it reads. */
    chipClassName: string;
    /** How far the chip rides above the pane's bottom edge (the drawer pushes it up). */
    chipBottom: number;
    // A style rather than a class: the clip and the divider's own `left` are one number, and a Tailwind
    // arbitrary value cannot carry a number that changes under the pointer.
    /** The clip applied by the split comparison, and nothing else. */
    style?: CSSProperties;
    /** Whether this pane reports the canvas's viewport to the sidebar. Only the enhanced pane does. */
    publishViewport?: boolean;
    // The enhanced pane is handed the **source's** identity while drawing the result's pixels, which is
    // the one thing that keeps the comparison a comparison: two panes at different zooms showing
    // different parts of a photograph is exactly what the second pane is not for. Every operation this
    // application runs preserves the aspect ratio, so both panes fit into the same box and resolve one
    // transform identically.
    //
    // Given rather than derived: it is the *source's* identity for both panes, while the enhanced
    // pane's pixels are addressed by the result's. Nothing about a URL says which of the two it is,
    // which is why this is stated rather than inferred.
    /** Which photograph's transform this pane reads and writes. Absent, the pane draws the fitted one. */
    transformKey?: string;
    // The three of them are the focus pick's, and are the same for both panes of a comparison: whether the next click
    // sets a lens blur's focus point, where it is now, and what to do with the one that lands.
    /** Whether the preview is waiting for a click that sets a lens blur's focus point. */
    armed?: boolean;
    /** The lens blur's focus as fractions of the framed photograph, where the stack has one. */
    focus?: { x: number; y: number };
    /** Called with the click's place in the framed photograph, as fractions across and down. */
    onPick?: (x: number, y: number) => void;
    // The two of them are the mask's, and are the same for both panes of a comparison, except that only the enhanced
    // pane shows the tint: the original is what the mask is *not* applied to.
    /** What a press does while the mask is painted, or absent where a press pans. */
    paint?: Paint;
    /** The photograph whose mask is tinted over this pane, or absent where none is shown. */
    maskPath?: string;
    // In the split comparison the enhanced pane is clipped from the divider rightward, and the ring is drawn inside the
    // pane that has the pointer - so a stroke that began on one side and crossed the divider would lose it. Drawn in
    // the canvas instead, which nothing clips.
    /** The canvas to draw the brush's ring in, over both panes, or absent to draw it inside this pane. */
    ringIn?: HTMLElement | null;
};

/** The gap between a pane's chip and whatever is beneath it. */
const CHIP_GAP = 10;

/** A pane's size before it has been measured (which stays true in jsdom). */
const UNMEASURED: Size = { width: 0, height: 0 };

// The identity is carried with it because it is what makes a change of *image* different from a
// change of *scale* - see `resolve`.
//
// `identity: string | undefined` rather than the optional-property syntax the rest of this codebase
// prefers. Every read of it is an `===` against another `string | undefined`, where an absent key and
// an explicit `undefined` are indistinguishable - so the optional form buys nothing here and costs a
// conditional spread at the one place this is written.
//
// Requiring it is also what makes `resolve`'s return type honest: it answers a *position* and never
// an identity, so it returns `ImageTransform` and this is assembled from the two at the one call site
// that knows both.
/** What a pane has drawn: the resolved transform and the photograph it was resolved for. */
type Drawn = { identity: string | undefined; scale: number; x: number; y: number };

/** The photograph's on-screen size at a given magnification. */
const scaled = (fitted: Size, scale: number): Size => ({ width: fitted.width * scale, height: fitted.height * scale });

/**
 * Resolves the requested transform into the position to draw at.
 *
 * At an unchanged scale (or a changed image) the stored position is just held inside the pane. At a
 * changed scale, one point of the photograph stays pinned on screen: the point under the pointer for a
 * wheel zoom, the pane's centre for the slider and step buttons.
 */
const resolve = (
    transform: ImageTransform,
    previous: Drawn,
    identity: string | undefined,
    fitted: Size,
    container: Size,
): ImageTransform => {
    const after = scaled(fitted, transform.scale);

    // The two cases are the reference's own split. A change of image joins the unchanged one:
    // re-anchoring against the position the *previous* photograph was drawn at would show an arbitrary
    // part of the new one, and returning to an image has to return to the part of it that was being
    // examined. The reference resolves both panes off one library instance and does not distinguish
    // the two.
    if (previous.scale === transform.scale || previous.identity !== identity) {
        const position = constrainPosition({ x: transform.x, y: transform.y }, after, container);

        return { scale: transform.scale, ...position };
    }

    const before = scaled(fitted, previous.scale);
    const anchor = {
        x: transform.anchor?.x ?? imageFraction(container.width / 2 - previous.x, before.width),
        y: transform.anchor?.y ?? imageFraction(container.height / 2 - previous.y, before.height),
    };

    // Where that point of the photograph is on screen right now, and where the position has to move
    // to so that it is still there once the photograph has changed size around it.
    const screen = { x: previous.x + anchor.x * before.width, y: previous.y + anchor.y * before.height };

    return {
        scale: transform.scale,
        x: constrain(screen.x - anchor.x * after.width, after.width, container.width),
        y: constrain(screen.y - anchor.y * after.height, after.height, container.height),
    };
};

/** One side of the comparison: the photograph, fitted to the pane and moved by a transform. */
const Pane = ({
    source,
    alt,
    label,
    className,
    chipClassName,
    chipBottom,
    style,
    publishViewport,
    transformKey,
    published,
    armed = false,
    focus,
    onPick,
    paint,
    maskPath,
    ringIn,
}: PaneProps) => {
    const container = useRef<HTMLDivElement>(null);

    // What this pane's *view* is keyed on, which is not what its pixels are addressed by - see
    // `transformKey`. Everything below reads this: the stored transform, the "is this a different
    // photograph?" test in `resolve`, and the wheel handler's write.
    const identity = transformKey;
    const transform = useImageTransform(identity);
    const setTransform = useTransformStore((state) => state.setTransform);
    const setViewport = useTransformStore((state) => state.setViewport);
    const setFit = useTransformStore((state) => state.setFit);

    const [measured, setMeasured] = useState<Size>(UNMEASURED);
    const [natural, setNatural] = useState<Size>(UNMEASURED);
    const [drawn, setDrawn] = useState<Drawn>({ identity: undefined, scale: transform.scale, x: 0, y: 0 });

    // The photograph's own dimensions: the published ones where the header could be read, and the
    // decoded image's otherwise - a file whose header this application could not parse is still served
    // and still drawn, and without the fallback it would have no size to be fitted into and would draw
    // as nothing.
    const imageWidth = published?.width ?? natural.width;
    const imageHeight = published?.height ?? natural.height;

    // Memoised because the resolution below is keyed on it: a fresh object per render would re-run
    // that effect on its own output, and re-resolving an anchored zoom against the position the
    // *store* still holds - which is the pre-zoom one - would undo the anchoring a frame later.
    const fitted = useMemo(
        () => fitInside(measured, { width: imageWidth, height: imageHeight }),
        [measured, imageWidth, imageHeight],
    );

    // Read by the two pointer handlers, which are registered once and must not close over a value
    // that changes sixty times a second. This is what the reference reads off its library instance.
    const live = useRef({ drawn, fitted, measured, identity });
    live.current = { drawn, fitted, measured, identity };

    // Measured on every resize, not once per image. That is the reference's own correction: measuring
    // once leaves the wheel anchor, the bounds and the published viewport all working off stale values
    // after a window resize or a sidebar toggle - and all three are wrong together, which is what makes
    // it hard to see.
    useResizeObserver(
        useCallback(() => container.current, []),
        useCallback((element: Element) => {
            const rect = element.getBoundingClientRect();

            // Same numbers means the same object, so a resize that changes nothing does not
            // re-render - and does not republish the viewport it would have recomputed.
            setMeasured((current) =>
                current.width === rect.width && current.height === rect.height
                    ? current
                    : { width: rect.width, height: rect.height },
            );
        }, []),
    );

    const requested = useRef<ImageTransform>(undefined);

    /*
     * Resolves what was asked for into what is drawn, and reports it to the sidebar.
     *
     * `drawn` is read through the ref rather than taken as a dependency: it is this effect's own
     * output, and depending on it would run the effect again on every resolution.
     */
    useEffect(() => {
        const previous = live.current.drawn;

        // A pane that has not been measured has resolved nothing: every position against a zero box
        // is zero, and the first real measurement has to resolve the stored transform rather than
        // re-constrain that. This is the state every pane mounts in, and the one every pane in
        // jsdom stays in without a stubbed rectangle.
        const ready = fitted.width > 0 && fitted.height > 0;

        // Nothing new was asked for, so this is a re-measurement - a window resize, a sidebar
        // toggle - and the photograph stays exactly where it is while the edges it is held between
        // are re-derived. Resolving the stored transform again instead would throw it back to the
        // position that transform was written at, which after an anchored zoom is the position it
        // was at *before* the zoom.
        const next =
            ready && transform === requested.current
                ? {
                      scale: previous.scale,
                      ...constrainPosition(previous, scaled(fitted, previous.scale), measured),
                  }
                : resolve(transform, previous, identity, fitted, measured);

        requested.current = ready ? transform : undefined;

        setDrawn((current) =>
            current.scale === next.scale &&
            current.x === next.x &&
            current.y === next.y &&
            current.identity === identity
                ? current
                : { ...next, identity },
        );

        if (!publishViewport) return;

        // What one photograph pixel measures at 1x in this pane, which is what actual pixels is
        // computed from - in the photograph's own pixels, so a smaller rendition does not change it.
        if (identity && fitted.width > 0 && imageWidth > 0) setFit({ identity, ratio: fitted.width / imageWidth });

        // The portion of the photograph inside this pane, as fractions of the whole of it. The
        // container is this pane's own box, so it is already half-width in side by side.
        const size = scaled(fitted, next.scale);
        if (size.width <= 0 || size.height <= 0) {
            setViewport({ x: 0, y: 0, width: 1, height: 1 });
            return;
        }

        const left = Math.max(0, next.x);
        const top = Math.max(0, next.y);
        const right = Math.min(measured.width, next.x + size.width);
        const bottom = Math.min(measured.height, next.y + size.height);

        setViewport({
            x: (left - next.x) / size.width,
            y: (top - next.y) / size.height,
            width: Math.max(0, right - left) / size.width,
            height: Math.max(0, bottom - top) / size.height,
        });
    }, [transform, fitted, measured, identity, imageWidth, publishViewport, setViewport, setFit]);

    /*
     * The wheel, and the trackpad pinch the webview delivers as one.
     *
     * A native, non-passive listener because React's synthetic `onWheel` is registered passive and
     * `preventDefault()` on it is a silent no-op. Per pane rather than once on the canvas, because the
     * handler needs this pane's own rectangle to turn a client coordinate into a point of the
     * photograph, and in side by side the pointer is over one particular half.
     *
     * **Every wheel event zooms, by a fixed step whatever its magnitude.** That is parity with the
     * reference's `ZOOM_WHEEL_STEP`, and it surprises: a two-finger scroll on a trackpad magnifies.
     */
    useEffect(() => {
        const element = container.current;
        if (!element) return;

        const onWheel = (event: WheelEvent) => {
            event.preventDefault();

            const { drawn: current, fitted: fittedNow, measured: measuredNow, identity: id } = live.current;
            if (id === undefined) return;

            // Wheel up is zoom in. Clamped here as well as in the store, because the position
            // resolved below is only the right position for a scale the store will actually keep:
            // resolving against 8.05 and then storing 8 would move the photograph by the difference.
            // The store's own `clampScale`, so the two cannot come to mean different numbers.
            const direction = event.deltaY < 0 ? 1 : -1;
            const scale = clampScale(current.scale + direction * ZOOM_WHEEL_STEP);

            const rect = element.getBoundingClientRect();
            const size = scaled(fittedNow, current.scale);
            const anchor = {
                x: imageFraction(event.clientX - rect.left - current.x, size.width),
                y: imageFraction(event.clientY - rect.top - current.y, size.height),
            };

            /*
             * Stored where the anchoring *lands*, rather than where the photograph is standing now.
             *
             * The pane resolves the anchor as it draws, so drawing is right either way - but the
             * store is what a photograph is handed back from when it becomes current again, and one
             * holding the pre-zoom position would return the user to a crop up to one notch's
             * anchoring correction away from the one they left. `anchor * fitted * ZOOM_WHEEL_STEP`
             * is 40px of an 800px pane at the edge of a photograph: small, visible, and a divergence
             * from "the same part of it as when they left it".
             *
             * Through `resolve` rather than by repeating its arithmetic, and off the same three
             * values the effect resolves against, so the position stored and the position drawn are
             * the same number computed once.
             *
             * The anchor rides along regardless: it is what a sibling pane applies to its own
             * container, which is the whole reason it is a fraction rather than a pixel offset - see
             * `imageFraction`.
             */
            const next = resolve({ scale, x: current.x, y: current.y, anchor }, current, id, fittedNow, measuredNow);

            setTransform(id, { scale: next.scale, x: next.x, y: next.y, anchor });
        };

        element.addEventListener("wheel", onWheel, { passive: false });

        return () => element.removeEventListener("wheel", onWheel);
    }, [setTransform]);

    /*
     * The drag that pans, coalesced to one store write per frame.
     *
     * Without the coalescing every pointer frame re-renders both panes, re-runs the resolution above
     * and republishes the viewport - "a sustained ~60Hz cascade through the tree for the length of a
     * drag", which is the reference's own measurement.
     */
    const grab = useRef<{ pointer: { x: number; y: number }; from: { x: number; y: number } }>(undefined);
    const pending = useRef<ImageTransform>(undefined);
    const frame = useRef<number>(undefined);
    const [dragging, setDragging] = useState(false);

    /*
     * A press that may turn out to be the click that sets the focus: where it went down, and whether it has been
     * dragged since. Only while armed, and only the primary button. A drag still pans - the pick is decided when the
     * pointer comes up.
     */
    const press = useRef<{ x: number; y: number; dragged: boolean }>(undefined);

    /*
     * The stroke this pane has under the pointer, as the handlers it began with: kept so that a stroke goes on to its
     * end however `paint` has changed since - Space pressed half way through does not strand it.
     */
    const stroking = useRef<Paint>(undefined);
    // Where the stroke last was, for a press that ends without a pointer to say where.
    const lastAt = useRef<{ x: number; y: number }>(undefined);

    /*
     * Where the pointer is on the framed photograph, as fractions across and down it - the wheel's own anchor
     * arithmetic - clamped to its edges; and the same, or `undefined` for a point in the margin beside it.
     */
    const fractionsAt = (clientX: number, clientY: number) => {
        const { drawn: current, fitted: fittedNow } = live.current;
        const rect = container.current?.getBoundingClientRect();
        const size = scaled(fittedNow, current.scale);
        if (!rect || size.width <= 0 || size.height <= 0) return;

        const offset = { x: clientX - rect.left - current.x, y: clientY - rect.top - current.y };

        return {
            at: { x: imageFraction(offset.x, size.width), y: imageFraction(offset.y, size.height) },
            inside: offset.x >= 0 && offset.y >= 0 && offset.x <= size.width && offset.y <= size.height,
        };
    };

    const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
        if (identity === undefined) return;

        // Painting takes the primary press, and only where it lands on the photograph: one in the margin starts
        // nothing. Another button still pans, and so does Space, which is `paint` being absent.
        if (paint && event.button === 0) {
            // Ctrl and a click is a right click on a Mac, and the context menu is what it opens: not a stroke.
            if (event.ctrlKey && onMac()) return;

            const pressed = fractionsAt(event.clientX, event.clientY);
            if (!pressed?.inside) return;

            event.currentTarget.setPointerCapture(event.pointerId);
            stroking.current = paint;
            lastAt.current = pressed.at;
            paint.onBegin(pressed.at.x, pressed.at.y, event.altKey);

            return;
        }

        press.current =
            armed && event.button === 0 ? { x: event.clientX, y: event.clientY, dragged: false } : undefined;

        // Capture rather than document-level listeners, because a drag that leaves the pane - and one
        // that leaves the window - must keep delivering moves to the element that started it, and the
        // capture is also what makes the release land in the right place.
        event.currentTarget.setPointerCapture(event.pointerId);
        grab.current = { pointer: { x: event.clientX, y: event.clientY }, from: { x: drawn.x, y: drawn.y } };
        setDragging(true);
    };

    const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
        const pressed = press.current;
        if (pressed && Math.hypot(event.clientX - pressed.x, event.clientY - pressed.y) > CLICK_SLOP) {
            pressed.dragged = true;
        }

        if (stroking.current) {
            const moved = fractionsAt(event.clientX, event.clientY);
            if (moved) {
                lastAt.current = moved.at;
                stroking.current.onMove(moved.at.x, moved.at.y);
            }

            return;
        }

        const start = grab.current;
        const { drawn: current, fitted: fittedNow, measured: measuredNow, identity: id } = live.current;
        if (!start || id === undefined) return;

        const position = constrainPosition(
            {
                x: start.from.x + (event.clientX - start.pointer.x),
                y: start.from.y + (event.clientY - start.pointer.y),
            },
            scaled(fittedNow, current.scale),
            measuredNow,
        );

        pending.current = { scale: current.scale, ...position };
        if (frame.current !== undefined) return;

        frame.current = requestAnimationFrame(() => {
            frame.current = undefined;
            if (pending.current) setTransform(id, pending.current);
        });
    };

    /*
     * The photograph's place under a click, as fractions across and down it as it is framed - the wheel's own anchor
     * arithmetic - or `undefined` for a click in the margin beside it, which sets nothing.
     */
    const photographAt = (clientX: number, clientY: number) => {
        const { drawn: current, fitted: fittedNow } = live.current;
        const rect = container.current?.getBoundingClientRect();
        const size = scaled(fittedNow, current.scale);
        if (!rect || size.width <= 0 || size.height <= 0) return;

        const offset = { x: clientX - rect.left - current.x, y: clientY - rect.top - current.y };
        if (offset.x < 0 || offset.y < 0 || offset.x > size.width || offset.y > size.height) return;

        return { x: imageFraction(offset.x, size.width), y: imageFraction(offset.y, size.height) };
    };

    const release = (event: PointerEvent<HTMLDivElement>) => {
        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
            event.currentTarget.releasePointerCapture(event.pointerId);
        }

        grab.current = undefined;
        setDragging(false);
    };

    const onPointerUp = (event: PointerEvent<HTMLDivElement>) => {
        const stroke = stroking.current;
        stroking.current = undefined;

        if (stroke) {
            // Ends where the pointer came up, which is the last point of the stroke.
            const ended = fractionsAt(event.clientX, event.clientY);
            if (ended) stroke.onEnd(ended.at.x, ended.at.y);
            else stroke.onCancel();
        }

        release(event);

        // A click, not a drag: the pointer never strayed past the slop since it went down. The mode is left only by a
        // click that lands on the photograph; one in the margin, or a drag, leaves it armed.
        const pressed = press.current;
        press.current = undefined;
        if (!armed || !pressed || pressed.dragged || !onPick) return;

        const at = photographAt(event.clientX, event.clientY);
        if (at) onPick(at.x, at.y);
    };

    // A press the system took away is no stroke, and no click.
    const onPointerCancel = (event: PointerEvent<HTMLDivElement>) => {
        stroking.current?.onCancel();
        stroking.current = undefined;
        press.current = undefined;
        release(event);
    };

    // Capture lost with no release or cancel to say why - the element re-rendered away, or the webview took it: the
    // press is over, and the stroke ends where it last was rather than being left under a pointer that is no longer
    // there. After a release this finds nothing to end.
    const onLostPointerCapture = () => {
        const stroke = stroking.current;
        stroking.current = undefined;
        grab.current = undefined;
        setDragging(false);

        const at = lastAt.current;
        if (stroke && at) stroke.onEnd(at.x, at.y);
        else stroke?.onCancel();
    };

    /*
     * Flushed on unmount rather than cancelled, as the reference does: the coalesced write is the only
     * one that ever reaches the store, so dropping it loses up to a frame of panning. The identity
     * comes through the ref, so the flush lands on the image being dragged rather than on whichever
     * one this pane first mounted with.
     */
    useEffect(
        () => () => {
            if (frame.current === undefined) return;

            cancelAnimationFrame(frame.current);
            const id = live.current.identity;
            if (pending.current && id !== undefined) setTransform(id, pending.current);
        },
        [setTransform],
    );

    /*
     * The pan keys (`Mod` with an arrow by default), answered by the pane that publishes the viewport -
     * one pane, since in side by side both draw the same part of the photograph. Moved from where the
     * photograph is *drawn*, as a drag is, and held inside the pane the same way; a step is a share of
     * the pane, `PAN_KEY_STEP`. Nothing moves while the photograph is drawn whole.
     */
    const panned = useRef<ImageTransform>(undefined);

    useEffect(() => {
        if (!publishViewport) return;

        return registerPanner((direction: PanDirection) => {
            const { drawn: shown, fitted: fittedNow, measured: measuredNow, identity: id } = live.current;
            if (id === undefined || shown.scale <= ZOOM_MIN) return false;

            // A press that lands before the last one has been drawn moves on from where that one put the
            // photograph rather than undoing it - the store still holds exactly what it wrote. Anything
            // else in the store since (a zoom, a drag) is resolved by now, so what is drawn is the answer.
            const stored = useTransformStore.getState().transforms.get(id);
            const last = panned.current;
            const current =
                stored && last && stored.scale === last.scale && stored.x === last.x && stored.y === last.y
                    ? last
                    : shown;

            const step = { x: measuredNow.width * PAN_KEY_STEP, y: measuredNow.height * PAN_KEY_STEP };
            // The photograph moves against the arrow, so the view moves with it.
            const delta = {
                left: { x: step.x, y: 0 },
                right: { x: -step.x, y: 0 },
                up: { x: 0, y: step.y },
                down: { x: 0, y: -step.y },
            }[direction];

            const position = constrainPosition(
                { x: current.x + delta.x, y: current.y + delta.y },
                scaled(fittedNow, current.scale),
                measuredNow,
            );
            panned.current = { scale: current.scale, ...position };
            setTransform(id, panned.current);

            return true;
        });
    }, [publishViewport, setTransform]);

    const ring = paint && (
        <BrushCursor
            container={container}
            frame={ringIn}
            radius={radiusOf(paint.size) * Math.max(fitted.width, fitted.height) * drawn.scale}
            softness={paint.softness}
            erase={paint.erase}
        />
    );

    return (
        <div
            ref={container}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={onPointerCancel}
            onLostPointerCapture={onLostPointerCapture}
            style={style}
            className={cn(
                "relative min-w-0 flex-1 touch-none overflow-hidden",
                // No system cursor while painting: the ring under the pointer is the cursor.
                dragging ? "cursor-grabbing" : paint ? "cursor-none" : armed ? "cursor-crosshair" : "cursor-grab",
                className,
            )}
            data-slot="preview-pane"
        >
            {/*
             * Spread conditionally, the shape `components/ui/slider.tsx` uses and for the same reason:
             * under `exactOptionalPropertyTypes` an explicit `undefined` is not an absent prop, and
             * `src=""` would resolve against the document and fetch the page itself.
             *
             * **No `object-contain`.** It centres the photograph by a rule the pane cannot read back,
             * and every number here - the bounds a pan stops at, the point a wheel zoom is anchored
             * on, the rectangle the sidebar draws - is derived from knowing exactly where the
             * photograph's top-left corner is. The `<img>` is drawn at its measured fitted size
             * instead, positioned at the pane's top-left, and moved with `translate(...) scale(...)`
             * about a `top left` origin.
             *
             * Sized from the measurement rather than by CSS, and left unsized until there is one - a
             * pane mounted hidden, or measured before layout, would otherwise draw a photograph of
             * zero width. `draggable` is off because the browser's own image drag would otherwise
             * start on top of the pan.
             *
             * **`will-change: transform` puts the photograph on a compositing layer of its own.**
             * Without it WKWebView paints the transformed `<img>` into the pane's backing store, and
             * zooming back out repaints only part of the area the magnified image had covered: stale
             * tiles are left behind as thin horizontal lines over the canvas. On its own layer the
             * zoom and pan are a compositor transform and the pane is never repainted. An `<img>` is
             * composited directly from its decoded pixels, so magnification stays sharp.
             */}
            {/*
             * Keyed by the photograph, so a new one starts from an empty element rather than a webview that goes on
             * painting the previous photograph's bitmap into the new box until the new one arrives: the stand-in is
             * this photograph's own thumbnail, already decoded, so the empty frame is not seen.
             */}
            <img
                key={transformKey}
                {...(source !== undefined && { src: source })}
                alt={alt}
                draggable={false}
                onLoad={(event) =>
                    setNatural({ width: event.currentTarget.naturalWidth, height: event.currentTarget.naturalHeight })
                }
                style={{
                    ...(fitted.width > 0 && { width: fitted.width, height: fitted.height }),
                    transform: `translate(${drawn.x}px, ${drawn.y}px) scale(${drawn.scale})`,
                    transformOrigin: "top left",
                    willChange: "transform",
                }}
                className="block max-w-none select-none"
            />

            {/*
             * The ring over the focus point, in the box the photograph is drawn in: the pane's own position and size
             * of it, not the picture's transform, so the ring keeps one size at every zoom.
             */}
            <FocusPick
                armed={armed}
                {...(focus && { focus })}
                box={{ x: drawn.x, y: drawn.y, width: fitted.width * drawn.scale, height: fitted.height * drawn.scale }}
            />

            {/*
             * The mask's tint, in the box the photograph is drawn in: the pane's own position and size of it, not the
             * picture's transform, so it is the same shape at every zoom - as the faces' boxes are.
             */}
            {maskPath !== undefined && (
                <MaskOverlay
                    path={maskPath}
                    box={{
                        x: drawn.x,
                        y: drawn.y,
                        width: fitted.width * drawn.scale,
                        height: fitted.height * drawn.scale,
                    }}
                />
            )}

            {paint && ringIn === undefined && ring}
            {paint && ringIn && createPortal(ring, ringIn)}

            {/*
             * The offset is a style rather than a class pair, for the reason the drawer's own fold is:
             * the unfolded value is `DRAWER_HEIGHT` plus the design's 10px gap, and writing it as a
             * Tailwind step would be that sum restated in a second unit with nothing linking the two.
             */}
            <span
                data-slot="preview-chip"
                style={{ bottom: chipBottom }}
                className={cn(
                    "absolute rounded-full border border-border bg-background/80 px-2 py-0.5 text-[11px]",
                    chipClassName,
                )}
            >
                {label}
            </span>
        </div>
    );
};

/** Where the split divider starts, and how far an arrow key moves it, in percent of the canvas. */
const DIVIDER_START = 50;
const DIVIDER_STEP = 2;

/**
 * The current image, drawn in the chosen comparison: full (enhanced pane only), side by side (both
 * panes at half width), or split (enhanced pane overlaid on the original, revealed from the divider
 * rightward).
 *
 * - The enhanced pane draws the current enhancement result; the original pane always draws the source.
 *   With no result yet (none, pending, or stopped) both point at the same URL and both chips read
 *   *Original*.
 * - The right-hand chip turns to *Enhanced* exactly when the pane stops drawing the source. A previous
 *   result is held while a new run works, so the chip doesn't flicker back.
 * - While the show-original key is held (`stores/peek.ts`), the enhanced pane draws the source and its
 *   chip reads *Original*, as if there were no result.
 * - The URL carries no size bound; the canvas asks for the photograph at its own size.
 * - Both panes share one transform, so zooming or panning either moves both.
 * - It draws the current image at once, from its thumbnail upwards (`hooks/useStandIn.ts`).
 */
export const PreviewImage = () => {
    const { t } = useTranslation();
    // Everything a pane needs - the URL, the dimensions it is fitted from, the identity its transform
    // is stored under - comes off this one record, so a change of image reaches all of them on the
    // same render. The current one, on the click: every URL drawn for it is its own, from its thumbnail
    // up (`hooks/useStandIn.ts`), so its shape is never put on another photograph's pixels.
    const file = useCurrentFile();
    const previewMode = usePreviewStore((state) => state.previewMode);

    /*
     * How the photograph is framed, which both panes draw through: the original pane asks the protocol
     * for the framing, and the enhanced pane draws a result that was *already* made from it, so its own
     * URL carries no crop.
     */
    const crop = useImageCrop(file?.identity);
    /*
     * The run, owned here because this is the component that draws both panes and the bar, and is
     * mounted for as long as a file is open. Asked for on the click: Rust decodes its own pixels, so
     * the run need not wait for the canvas's, and the previous photograph's result is not drawn over
     * this one's stand-in - it belongs to another identity.
     */
    const { enhanced: result, report, running, fraction } = useEnhancementRun(file, crop);
    const progressShown = useShownProgress(running, report);

    /*
     * While the show-original key is held the result is set aside for this render, and everything
     * below - the URL, the dimensions, the chip - reads as it does before a result has landed: the
     * enhanced pane draws the source and calls it the original, in all three comparisons. Nothing
     * else is touched, so the transform, the divider and a running enhancement carry on underneath.
     */
    const showingOriginal = usePeekStore((state) => state.showingOriginal);
    const enhanced = showingOriginal ? undefined : result;
    /*
     * The chain this file runs, for the bar to name the model a slow session is being opened for - *Compiling Kazan
     * model* - since a report names its family and not its model.
     */
    const operations = useFileEnhancements(file?.path);

    /*
     * One URL for both panes and for all three modes, so switching modes re-points an `<img>` rather
     * than asking the protocol for anything new. Whether the *second pane* is free depends on the
     * platform: the response is served immutable, but as `ipc/images.ts` records, only WebView2 acts
     * on that - on macOS and Linux each `<img>` issues its own request and its own decode. Rust
     * answers the second from `Renditions` either way, so what repeats is the webview's decode, not
     * the encode. Bounding the panes the way the sidebar's miniature is bounded would change what the
     * canvas draws, so it is a decision for the canvas rather than a cleanup.
     *
     * The strip's thumbnail on the click, a screen-sized rendition once it has decoded, and the photograph at its own
     * size - `getImage(file, 0)` in the reference - only once it is magnified: that is what makes 8x a magnification of
     * real pixels rather than of a canvas-sized rendition, and it is a cost the fitted view does not need to pay.
     *
     * Absent for a file whose pixels nothing can serve: the `<img>` then has no source and draws as
     * broken, which is also what a file deleted between being described and being drawn does.
     * Inventing a state for that here would be designing an error screen for a case no user has
     * reached.
     */
    const magnified = useImageTransform(file?.identity).scale > ZOOM_MIN;
    const source = useStandIn(file, crop, magnified);

    /*
     * The pane labels ride above whichever of the drawer's two states is showing, as the design's own
     * `chipBottom` does. The drawer slides over the canvas rather than resizing it, so this is the one
     * thing on the canvas that has to know it moved.
     */
    const drawerOpen = useDrawerStore((state) => state.open);
    const chipBottom = (drawerOpen ? DRAWER_HEIGHT : 0) + CHIP_GAP;

    /*
     * Where the split's divider sits, in this component's own state.
     *
     * Not in a store, because it is not a property of a photograph - switching images leaves it
     * alone, and nothing remounts this component to make that happen - and not a persisted
     * preference either: the reference does not keep it, and `stores/preview.ts` writes into the
     * Wails application's own key, which held exactly one field. Adding a second would be this
     * application writing something the application it replaces does not read.
     */
    const [divider, setDivider] = useState(DIVIDER_START);
    const handle = useRef<HTMLDivElement>(null);
    // The canvas itself, as state so that a pane draws into it once it is there: see `PaneProps.ringIn`.
    const [canvas, setCanvas] = useState<HTMLDivElement | null>(null);

    /*
     * Painting the mask: whether a press on the photograph paints, and with what.
     *
     * It does while the preview is in paint mode for this photograph and Space is not held - Space turns the drag back
     * into a pan. The stack has to have something in it, for there is nothing for a mask to limit otherwise. The
     * strokes go to the store, which holds them until the pointer comes up; both panes paint the same mask.
     */
    const path = file?.path;
    const editing = useIsEditing(path);
    const panning = useMaskStore((state) => state.panning);
    const tool = useMaskStore((state) => state.tool);
    const stacked = useFileEnhancements(path).length > 0;
    const paintable = editing && stacked && path !== undefined;

    /*
     * The focus pick: whether the next click sets the lens blur's focus point, and the lens blur it would set it on.
     *
     * Armed is for one photograph - the one the Set focus button was pressed over - so another photograph, or none,
     * is never armed. The mode is left when the photograph changes, when the lens blur is taken off the stack, and by
     * Escape; a click that lands leaves it in `onPick`.
     */
    const lensBlur = operations.find((operation) => operation.family === "lens_blur");
    const hasLensBlur = lensBlur !== undefined;
    const armed = useIsArmed(file?.identity) && hasLensBlur;
    const replaceEnhancement = useEnhancementStore((state) => state.replaceEnhancement);
    const identity = file?.identity;

    useEffect(() => {
        const { armed: waiting, imageId, disarm } = useFocusPickStore.getState();

        // Another photograph than the one armed for, or one whose lens blur has gone: nothing to click for.
        if (waiting && (imageId !== identity || !hasLensBlur)) disarm();
    }, [identity, hasLensBlur]);

    // A click is either the brush's or the focus pick's: arming the pick leaves the mask's editing, and editing the
    // mask leaves the pick.
    useEffect(() => {
        if (editing) useFocusPickStore.getState().disarm();
    }, [editing]);

    useEffect(() => {
        if (!armed) return;
        useMaskStore.getState().stopEditing();

        // An Escape that something else has taken - a dialog or a menu closing on it - is not this mode's: the dialog's
        // own layer marks the event handled, and a surface still open over the photograph would be left by the same
        // press that closed it.
        const onKeyDown = (event: KeyboardEvent) => {
            if (event.key !== "Escape" || event.defaultPrevented || document.querySelector(OPEN_LAYER)) return;

            useFocusPickStore.getState().disarm();
        };

        window.addEventListener("keydown", onKeyDown);

        return () => window.removeEventListener("keydown", onKeyDown);
    }, [armed]);

    // The middle where the operation carries none, which is the catalogue's own default for both and what a new lens
    // blur is built with; the canvas does not wait on the catalogue to draw a ring.
    const focus = lensBlur && { x: lensBlur.parameters[FOCUS_X] ?? 0.5, y: lensBlur.parameters[FOCUS_Y] ?? 0.5 };

    // Written to the stack as the panel writes the f-number, which is what starts the new run. Rounded to the
    // thousandths the library keeps a focus point at, so the ring is drawn where the run will focus.
    const onPick = (x: number, y: number) => {
        if (!lensBlur || path === undefined) return;

        const thousandths = (fraction: number) => Math.round(fraction * 1000) / 1000;

        replaceEnhancement(
            path,
            withParameter(withParameter(lensBlur, FOCUS_X, thousandths(x)), FOCUS_Y, thousandths(y)),
        );
        useFocusPickStore.getState().picked();
    };

    const moveDivider = (clientX: number) => {
        const rect = handle.current?.parentElement?.getBoundingClientRect();
        if (!rect || rect.width <= 0) return;

        // The same clamped fraction the panes take their anchor from, as a percentage - the width
        // guard above is what `imageFraction`'s own zero case would otherwise answer with a centred
        // divider rather than leaving this one where the user put it.
        setDivider(imageFraction(clientX - rect.left, rect.width) * 100);
    };

    // Drawn by `Preview`, which asks the same question to decide between this and the invitation.
    // Stated anyway, because a component that reads the current file has to say what it does without
    // one - and a file whose pixels cannot be served is the case above, not this one.
    if (!file) return;

    /*
     * The shape the photograph is drawn at, which is the framing's rather than the file's wherever one
     * is set. Both panes lay their box out from this *before* the pixels arrive, so it has to come
     * from the crop's own rectangle - see `framedDimensions`.
     */
    const framed = framedDimensions(file, crop);

    // The photograph's shape as it is framed, which decimating a stroke's points is measured in. Zero where unknown.
    const aspect = framed.width && framed.height ? framed.width / framed.height : 0;

    const paint: Paint | undefined =
        paintable && !panning
            ? {
                  size: tool.size,
                  softness: tool.softness,
                  erase: tool.mode === "erase",
                  onBegin: (x, y, alt) => useMaskStore.getState().beginStroke(path, x, y, alt),
                  onMove: (x, y) => useMaskStore.getState().extendStroke(x, y, aspect),
                  onEnd: (x, y) => useMaskStore.getState().endStroke(x, y),
                  onCancel: () => useMaskStore.getState().cancelStroke(),
              }
            : undefined;

    // The tint is the enhanced pane's alone, and only while the mask is being edited and the user has it on.
    const maskPath = paintable && tool.overlay ? path : undefined;

    /*
     * What the enhanced pane draws: the result's own pixels and its own dimensions, or the source's
     * URL and the framing's shape while nothing has been produced - which is both panes pointing at
     * one URL, as the reference does for an empty stack. That costs no second encode - the identity
     * and the bound are the same, so Rust answers the second pane from `Renditions` - and on WebView2,
     * the one webview that acts on `immutable`, the webview's cache answers it outright. See the
     * comment above `source`.
     *
     * No framing on the result's own URL: the run was given the framed pixels, so what it produced is
     * already the enhancement of the framing and asking for it to be framed again would cut the
     * rectangle out of it a second time.
     *
     * Named by the run that produced it as well as by its identity - see `resultUrl`.
     */
    const enhancedSource = enhanced ? resultUrl(enhanced) : source;

    return (
        <div ref={setCanvas} className="relative flex size-full flex-row gap-0.5 p-0.5">
            {/*
             * Absent in full, which draws the enhanced image alone - and present in split, where it is
             * the pane the other one is revealed over rather than a second half of the canvas.
             */}
            {previewMode !== "full" && (
                <Pane
                    published={framed}
                    {...(file.identity !== undefined && { transformKey: file.identity })}
                    {...(source !== undefined && { source })}
                    alt={t("common.previewAlt")}
                    label={t("preview.pane.original")}
                    chipClassName="left-2.5 text-foreground"
                    chipBottom={chipBottom}
                    armed={armed}
                    {...(focus && { focus })}
                    onPick={onPick}
                    {...(paint && { paint })}
                    {...(previewMode === "split" && { ringIn: canvas })}
                />
            )}

            <Pane
                published={enhanced ?? framed}
                {...(enhancedSource !== undefined && { source: enhancedSource })}
                {...(file.identity !== undefined && { transformKey: file.identity })}
                alt={t("common.previewAlt")}
                /*
                 * *Enhanced* only once a result has landed. This pane draws the source until
                 * then - with no enhancements at all, and for as long as the first run is still
                 * working - and a chip that called the source enhanced would be labelling the
                 * photograph by what was asked for rather than by what is on screen.
                 */
                label={enhanced ? t("preview.pane.enhanced") : t("preview.pane.original")}
                publishViewport
                /*
                 * `inset-0.5` is the container's own padding, so the overlay covers exactly the box
                 * the panes are laid out in. As a flex sibling it would be a second, half-width copy
                 * of the photograph instead of the same one superimposed.
                 *
                 * The clip and the divider's `left` below are one number rather than the matched pair
                 * of literals they were while the divider could not move.
                 */
                className={cn(previewMode === "split" && "absolute inset-0.5")}
                {...(previewMode === "split" && { style: { clipPath: `inset(0 0 0 ${divider}%)` } })}
                chipClassName="right-2.5 text-foreground"
                chipBottom={chipBottom}
                armed={armed}
                {...(focus && { focus })}
                onPick={onPick}
                {...(paint && { paint })}
                {...(maskPath !== undefined && { maskPath })}
                {...(previewMode === "split" && { ringIn: canvas })}
            />

            {armed && <FocusHint />}

            {progressShown && <ProgressBar fraction={fraction} {...(report && { report })} operations={operations} />}

            {previewMode === "split" && (
                /*
                 * A real `role="slider"` with arrow keys, rather than a `<div>` with a pointer
                 * handler: the reference's handle is reachable by no route but the mouse, and this
                 * codebase has taken the other answer at every turn. A keydown handler moving the
                 * divider by 2% is what makes the third comparison operable.
                 *
                 * **The divider is the only draggable thing**, which is the reference's
                 * `onlyHandleDraggable` - and here it is not a preference: the panes themselves are
                 * the pan surface, so a drag started on a pane and a drag started on the divider have
                 * to mean different things.
                 */
                <div
                    ref={handle}
                    role="slider"
                    tabIndex={0}
                    aria-label={t("preview.divider")}
                    aria-valuemin={0}
                    aria-valuemax={100}
                    aria-valuenow={Math.round(divider)}
                    style={{ left: `${divider}%` }}
                    onPointerDown={(event) => {
                        event.currentTarget.setPointerCapture(event.pointerId);
                        moveDivider(event.clientX);
                    }}
                    onPointerMove={(event) => {
                        if (event.currentTarget.hasPointerCapture(event.pointerId)) moveDivider(event.clientX);
                    }}
                    onPointerUp={(event) => {
                        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
                            event.currentTarget.releasePointerCapture(event.pointerId);
                        }
                    }}
                    onKeyDown={(event) => {
                        if (event.key === "ArrowLeft") setDivider((at) => Math.max(at - DIVIDER_STEP, 0));
                        else if (event.key === "ArrowRight") setDivider((at) => Math.min(at + DIVIDER_STEP, 100));
                        else return;

                        event.preventDefault();
                    }}
                    className="absolute top-0 bottom-0 z-[3] w-0.5 cursor-ew-resize touch-none bg-foreground outline-hidden"
                    data-slot="preview-divider"
                >
                    <span className="absolute top-1/2 left-1/2 flex size-8 -translate-x-1/2 -translate-y-1/2 items-center justify-center rounded-full border-2 border-foreground bg-background/75">
                        <ChevronsLeftRight className="size-4" />
                    </span>
                </div>
            )}
        </div>
    );
};
