import { act, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EnhancementRun } from "@/hooks/useEnhancementRun";
import { useShortcuts } from "@/hooks/useShortcuts";
import "@/i18n";
import type { Family } from "@/ipc/catalogue";
import { cropQuery } from "@/ipc/crop";
import type { RunProgress } from "@/ipc/enhance";
import type { Face } from "@/ipc/faces";
import { DRAWER_HEIGHT, ZOOM_MAX, ZOOM_MIN, ZOOM_WHEEL_STEP } from "@/lib/constants";
import { useDrawerStore } from "@/stores/drawer";
import { useEnhancementStore } from "@/stores/enhancements";
import { useFacesStore } from "@/stores/faces";
import { useFileStore } from "@/stores/files";
import { useFocusPickStore } from "@/stores/focusPick";
import { useMaskStore } from "@/stores/mask";
import { usePeekStore } from "@/stores/peek";
import { usePreviewStore } from "@/stores/preview";
import { type ImageTransform, panCurrent, useTransformStore } from "@/stores/transform";
import { FRAMING, frame, HOLIDAY, openFiles, render, resetCropStore, resetFileStore, SUNSET } from "@/test/support";
import { PreviewImage } from "./PreviewImage";

// `convertFileSrc` reads a global Tauri's init script installs, which jsdom has none of. Mocked to
// answer the identity it was asked for, so a test can tell which photograph a pane is drawing without
// restating the platform-dependent URL - which is the one thing `ipc/images.ts` exists to own.
// The real `platform()` reads a global the plugin's init script installs, which jsdom has none of - and the
// window's keys ask it, for which modifier `Mod` is.
vi.mock("@/ipc/os", () => ({ isMacOs: vi.fn(() => false), isWindows: vi.fn(() => false) }));

vi.mock("@tauri-apps/api/core", () => ({
    convertFileSrc: vi.fn((identity: string) => `opai://localhost/${identity}`),
}));

// The tint over a mask being painted asks Rust for its picture; what it does with the answer is
// `MaskOverlay.test.tsx`'s subject, and here only that it is drawn, or not, over the pane it should be.
vi.mock("@/ipc/mask", () => ({ maskPreview: vi.fn(() => new Promise(() => {})) }));

// The run itself is `hooks/useEnhancementRun.test.tsx`'s subject - what it asks for, what it stops
// and what it does with the answers. Mocked here so this file stays about what the canvas *draws*
// given a run, rather than about driving one through the seam.
vi.mock("@/hooks/useEnhancementRun", () => ({
    useEnhancementRun: vi.fn((): EnhancementRun => ({ running: false, fraction: 0, ...run })),
}));

/**
 * What the mocked hook answers, which a case sets before it draws.
 *
 * Partial, so that the position on the bar is stated only by the cases the position is about. It is
 * the hook's own figure and not the report's - a face recovery is two runs and one bar, and which
 * of the two the canvas draws is what the three cases below pin.
 */
let run: Partial<EnhancementRun> = { running: false };

/** The result of a run, as the hook reports one. A 2x enlargement of HOLIDAY's 3000x2000. */
const ENHANCED = { identity: "fedcba9876543210", width: 6000, height: 4000 };

/** One report about a run in flight. */
const progress = (extra: Partial<RunProgress> = {}): RunProgress => ({
    run: "run-1",
    operation: "Kyoto 2x (FP32)",
    family: "upscale",
    stage: "running",
    chainFraction: 0.62,
    ...extra,
});

const bar = () => document.querySelector("[data-slot='preview-progress']");

/**
 * How far the bar says the run has got.
 *
 * Read off the indicator's own transform, because that is how the shared `Progress` draws a value -
 * the slide of that transform is the animation this bar is built on. Its `progressbar` role carries
 * no `aria-valuenow`: the vendored component keeps `value` for the transform and does not forward it
 * to the Radix root, so every bar in this application reads as indeterminate to a screen reader.
 */
const filled = () => bar()?.querySelector("[data-slot='progress-indicator']");

/**
 * A pane of a known size, which jsdom otherwise answers 0 for.
 *
 * jsdom lays nothing out, so every `getBoundingClientRect` is a zero rect and every pane measures
 * itself as nothing - which is exactly the state `fitInside` returns zero for, and every assertion about
 * where a photograph is drawn would be an assertion about zero. One rect for every element is enough
 * here: the two panes are the same size in every mode, and the canvas the divider measures itself
 * against is the same box again.
 *
 * Restored by the `restoreMocks` in vite.config.ts, so nothing has to undo it.
 */
const PANE = { width: 1000, height: 600 };

const stubPaneGeometry = ({ width, height } = PANE) =>
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({
        x: 0,
        y: 0,
        top: 0,
        left: 0,
        right: width,
        bottom: height,
        width,
        height,
        toJSON: () => ({}),
    });

/** HOLIDAY is 3000x2000, so in a 1000x600 pane the height runs out first and it is drawn 900x600. */
const FITTED = { width: 900, height: 600 };

const panes = () => screen.getAllByRole("img", { name: "Preview" });

/** What a pane has actually drawn, read back off the one style that says so. */
const drawn = (index = 0) => {
    const style = panes()[index]?.style.transform ?? "";
    const [, x, y, scale] = /translate\((-?[\d.]+)px, (-?[\d.]+)px\) scale\(([\d.]+)\)/.exec(style) ?? [];

    return { x: Number(x), y: Number(y), scale: Number(scale) };
};

/** Magnifies the current photograph by whichever route a test is not itself about. */
const magnify = (transform: ImageTransform) =>
    act(() => useTransformStore.getState().setTransform(HOLIDAY.identity ?? "", transform));
const sources = () => panes().map((pane) => pane.getAttribute("src"));
const divider = () => document.querySelector("[data-slot='preview-divider']");

const draw = (mode: "full" | "side" | "split") => {
    usePreviewStore.setState({ previewMode: mode });

    return render(<PreviewImage />);
};

beforeEach(() => {
    run = { running: false };
    resetFileStore();
    resetCropStore();
    useDrawerStore.setState(useDrawerStore.getInitialState(), true);
    useTransformStore.setState(useTransformStore.getInitialState(), true);
    usePreviewStore.setState({ previewMode: "side" });
    usePeekStore.setState(usePeekStore.getInitialState(), true);
    openFiles(HOLIDAY);
});

describe("the canvas", () => {
    it("draws the two panes side by side, each labelled", () => {
        // With a result landed, which is the state in which the two chips differ - the labelling
        // rule itself is `the pane labels` below.
        run = { running: false, enhanced: ENHANCED };

        draw("side");

        expect(panes()).toHaveLength(2);
        expect(screen.getByText("Original")).toBeInTheDocument();
        expect(screen.getByText("Enhanced")).toBeInTheDocument();
        expect(divider()).toBeNull();
    });

    it("draws the enhanced image alone across the whole canvas", () => {
        run = { running: false, enhanced: ENHANCED };

        draw("full");

        // One pane, and it is the enhanced one: full is the mode that shows only the result.
        expect(panes()).toHaveLength(1);
        expect(screen.queryByText("Original")).not.toBeInTheDocument();
        expect(screen.getByText("Enhanced")).toBeInTheDocument();
    });

    it("draws the two superimposed, divided, when split is chosen", () => {
        run = { running: false, enhanced: ENHANCED };

        const { container } = draw("split");

        expect(panes()).toHaveLength(2);
        expect(divider()).toBeInTheDocument();

        // The enhanced pane is out of the flex flow and clipped from the divider rightward, which is
        // what makes it the same photograph superimposed rather than a second half-width copy. The
        // clip is a style rather than a class now that the divider moves: it is one number with the
        // divider's own `left`, where the two used to be a matched pair of literals.
        const enhanced = container.querySelector("[style*='clip-path']");
        expect(enhanced).toBeInTheDocument();
        expect(enhanced).toContainElement(screen.getByText("Enhanced"));
    });

    it("asks for one and the same photograph in every pane of every mode", () => {
        // The identity and the bound are the same, so the second pane and every mode change is a
        // cache hit on bytes the webview already has rather than a second decode.
        const wanted = `opai://localhost/${HOLIDAY.identity}`;

        for (const mode of ["full", "side", "split"] as const) {
            const { unmount } = draw(mode);

            expect(sources().every((source) => source === wanted)).toBe(true);
            unmount();
        }
    });

    it("asks for the photograph at its own size, with no bound", () => {
        draw("full");

        // `renditionUrl(identity)` rather than a canvas-sized rendition: the zoom a later slice adds
        // goes to 8x, and a bounded rendition would be an eight-times-magnified thumbnail.
        expect(sources()[0]).not.toContain("size=");
    });

    it("asks for the framing when the current image carries one", () => {
        frame(HOLIDAY);

        draw("side");

        // The same framing the sidebar's miniature asks for, at the canvas's own (unbounded) size.
        // Both regions read one store keyed by identity, so there is no second place a framing can be
        // spelled differently.
        const wanted = `opai://localhost/${HOLIDAY.identity}?crop=${cropQuery(FRAMING)}`;

        expect(sources().every((source) => source === wanted)).toBe(true);
    });

    it("asks for no framing at all while nothing is framed", () => {
        draw("side");

        // Which is every photograph in this application until the Crop/Rotate dialog ships: the URL
        // is byte-for-byte the one the canvas has always asked for.
        expect(sources().every((source) => source === `opai://localhost/${HOLIDAY.identity}`)).toBe(true);
    });

    it("does not frame a result that was already made from the framing", () => {
        frame(HOLIDAY);
        run = { running: false, enhanced: ENHANCED };

        draw("side");

        // The run was handed the framed pixels, so what it produced is already the enhancement of the
        // framing - asking for it to be framed again would cut the rectangle out of it a second time.
        const [original, enhanced] = sources();

        expect(original).toContain(`crop=${cropQuery(FRAMING)}`);
        expect(enhanced).toBe(`opai://localhost/${ENHANCED.identity}`);
    });

    it("draws nothing rather than the wrong pixels for a file it could not read", () => {
        resetFileStore();
        // A file whose bytes could not be read carries no identity, so nothing addresses it and the
        // protocol can never be asked for it. Built by leaving the key out rather than by setting it
        // to `undefined`, which under `exactOptionalPropertyTypes` is not the same record.
        const { identity, ...unreadable } = HOLIDAY;
        openFiles(unreadable);

        draw("side");

        expect(sources()).toEqual([null, null]);
    });
});

/**
 * Where the pane labels sit, which is the one thing on the canvas that has to know the drawer moved.
 *
 * The drawer slides over the canvas rather than resizing it, so a chip that stayed 10px above the
 * canvas's own bottom edge would be behind the strip for as long as it was unfolded - which is what
 * the design's `chipBottom` moves, and nothing else on this side does.
 */
describe("the pane labels", () => {
    const chip = (label: string) => screen.getByText(label);

    // A landed result throughout, so the two chips carry different words and `getByText` can name
    // either of them. What each one says is `what the pane labels say` below.
    beforeEach(() => {
        run = { running: false, enhanced: ENHANCED };
    });

    it("ride above the canvas's bottom edge while the drawer is folded", () => {
        draw("side");

        expect(chip("Original")).toHaveStyle({ bottom: "10px" });
        expect(chip("Enhanced")).toHaveStyle({ bottom: "10px" });
    });

    it("ride above the strip once the drawer is unfolded", () => {
        useDrawerStore.setState({ open: true });
        draw("side");

        // Derived from the drawer's own constant rather than written as 138, so the rule is what the
        // test states instead of today's arithmetic.
        expect(chip("Original")).toHaveStyle({ bottom: `${DRAWER_HEIGHT + 10}px` });
        expect(chip("Enhanced")).toHaveStyle({ bottom: `${DRAWER_HEIGHT + 10}px` });
    });

    it("lift in every comparison, not only the one with two panes", () => {
        useDrawerStore.setState({ open: true });
        draw("full");

        expect(chip("Enhanced")).toHaveStyle({ bottom: `${DRAWER_HEIGHT + 10}px` });
    });
});

/**
 * Where the photograph is drawn, which is the whole of what replaced `object-contain`.
 *
 * The browser's own centring was an offset nothing here could read back, and the bounds a pan stops
 * at, the point a wheel zoom is anchored on and the rectangle the sidebar draws are each derived
 * from knowing exactly where the photograph's top-left corner is.
 */
describe("the photograph's own box", () => {
    it("reports what one photograph pixel measures when fitted, for actual pixels", () => {
        stubPaneGeometry();
        draw("side");

        // 900px of pane for HOLIDAY's 3000px, from the pane that also reports the viewport.
        expect(useTransformStore.getState().fit).toEqual({ identity: HOLIDAY.identity, ratio: 0.3 });
    });

    it("is drawn at the size that fits, whole and centred, before anything magnifies it", () => {
        stubPaneGeometry();
        draw("full");

        // 900x600 in a 1000x600 pane, so the margin is 50px either side and none above or below.
        expect(panes()[0]).toHaveStyle({ width: "900px", height: "600px" });
        expect(drawn()).toEqual({ x: 50, y: 0, scale: 1 });
    });

    it("is drawn at the framing's shape rather than the file's", () => {
        stubPaneGeometry();
        // A portrait framing of a landscape photograph: 1200x1600 out of HOLIDAY's 3000x2000, so the
        // height runs out first and 600px of pane holds 450px of width.
        frame(HOLIDAY);

        draw("full");

        // Laid out from the crop's own rectangle, which is what makes the box right *before* the
        // pixels arrive - the whole point of `framedDimensions`. Read off the file it would be
        // 900x600, the wrong shape entirely, for the length of a decode and then permanently.
        expect(panes()[0]).toHaveStyle({ width: "450px", height: "600px" });
        expect(drawn()).toEqual({ x: 275, y: 0, scale: 1 });
    });

    it("is drawn at the file's own shape while nothing is framed", () => {
        stubPaneGeometry();
        draw("full");

        // Unchanged, which is this slice's acceptance criterion everywhere it appears.
        expect(panes()[0]).toHaveStyle({ width: `${FITTED.width}px`, height: `${FITTED.height}px` });
    });

    it("draws a magnified photograph at the position it was left at", () => {
        stubPaneGeometry();
        magnify({ scale: 2, x: -100, y: -50 });

        draw("full");

        // The element keeps its fitted size and the scale is a transform, so the compositor holds
        // one decoded bitmap and a matrix rather than a photograph resized to eight times a wall.
        expect(panes()[0]).toHaveStyle({ width: "900px", height: "600px" });
        expect(drawn()).toEqual({ x: -100, y: -50, scale: 2 });
    });

    it("holds a magnified photograph inside the pane it is in", () => {
        stubPaneGeometry();
        // 1800x1200 of photograph in a 1000x600 pane leaves 800px and 600px of travel; a position
        // past either is a photograph dragged away from its own edge, which never happens.
        magnify({ scale: 2, x: 400, y: -5000 });

        draw("full");

        expect(drawn()).toEqual({ x: 0, y: -600, scale: 2 });
    });

    it("draws nothing of a size it has not measured", () => {
        // A pane mounted hidden, a pane measured before layout - and every pane in jsdom without the
        // stub above. The `<img>` carries no width rather than a photograph of zero width.
        draw("full");

        expect(panes()[0]?.style.width).toBe("");
    });

    it("draws both panes from the one transform", () => {
        stubPaneGeometry();
        magnify({ scale: 2, x: -100, y: -50 });

        draw("side");

        // Two panes showing different parts of a photograph are not a comparison, which is the whole
        // of what the second one is for.
        expect(drawn(0)).toEqual(drawn(1));
        expect(drawn(0)).toEqual({ x: -100, y: -50, scale: 2 });
    });

    it("draws the same magnification in whichever comparison is chosen", () => {
        stubPaneGeometry();
        magnify({ scale: 2, x: -100, y: -50 });

        // The transform is a property of the photograph rather than of the layout drawing it, so
        // changing the comparison is a change of how many panes there are and of nothing else.
        for (const mode of ["full", "side", "split"] as const) {
            const { unmount } = draw(mode);

            expect(drawn()).toEqual({ x: -100, y: -50, scale: 2 });
            unmount();
        }
    });
});

/** Dragging a pane, which is the pan - and the one gesture that is coalesced rather than immediate. */
describe("panning", () => {
    let frames: FrameRequestCallback[] = [];

    beforeEach(() => {
        frames = [];
        // Held rather than run, so a test can say how many writes a drag made *before* the frame it
        // was coalesced into - which is the whole of what the coalescing is.
        vi.spyOn(globalThis, "requestAnimationFrame").mockImplementation((callback) => {
            frames.push(callback);

            return frames.length;
        });
        vi.spyOn(globalThis, "cancelAnimationFrame").mockImplementation(() => {});
    });

    afterEach(() => {
        frames = [];
    });

    const runFrame = () =>
        act(() => {
            for (const callback of frames) callback(0);
        });

    const pane = () => document.querySelector("[data-slot='preview-pane']") as HTMLElement;

    const dragBy = (element: HTMLElement, steps: { x: number; y: number }[]) => {
        fireEvent.pointerDown(element, { pointerId: 1, clientX: 500, clientY: 300 });
        for (const step of steps) {
            fireEvent.pointerMove(element, { pointerId: 1, clientX: 500 + step.x, clientY: 300 + step.y });
        }
    };

    it("writes once per frame however many pointer events a drag delivers", () => {
        stubPaneGeometry();
        magnify({ scale: 2, x: -100, y: -50 });
        draw("full");

        dragBy(pane(), [
            { x: -10, y: -10 },
            { x: -20, y: -20 },
            { x: -30, y: -30 },
        ]);

        // Three moves, one frame asked for, and nothing in the store until it runs: without this
        // every pointer frame re-rendered both panes and republished the viewport.
        expect(frames).toHaveLength(1);
        expect(useTransformStore.getState().transforms.get(HOLIDAY.identity ?? "")).toEqual({
            scale: 2,
            x: -100,
            y: -50,
        });

        runFrame();

        // The last of the three, not the first: the pending write is replaced, not queued.
        expect(drawn()).toEqual({ x: -130, y: -80, scale: 2 });
    });

    it("stops a magnified photograph at its own edge", () => {
        stubPaneGeometry();
        magnify({ scale: 2, x: -100, y: -50 });
        draw("full");

        dragBy(pane(), [{ x: 4000, y: 4000 }]);
        runFrame();

        expect(drawn()).toEqual({ x: 0, y: 0, scale: 2 });
    });

    it("leaves a fitted photograph whole and centred however it is dragged", () => {
        stubPaneGeometry();
        draw("full");

        dragBy(pane(), [{ x: -400, y: -200 }]);
        runFrame();

        // There is nothing to move at 1x, and "fitted" means centred - so every position on this
        // axis resolves to the same one.
        expect(drawn()).toEqual({ x: 50, y: 0, scale: 1 });
    });

    it("takes the pointer capture, so a drag that leaves the pane keeps following it", () => {
        stubPaneGeometry();
        magnify({ scale: 2, x: -100, y: -50 });
        draw("full");

        const element = pane();
        fireEvent.pointerDown(element, { pointerId: 1, clientX: 500, clientY: 300 });
        expect(element.hasPointerCapture(1)).toBe(true);

        fireEvent.pointerUp(element, { pointerId: 1 });
        expect(element.hasPointerCapture(1)).toBe(false);
    });

    it("keeps the last position of a drag that ends with the pane going away", () => {
        stubPaneGeometry();
        magnify({ scale: 2, x: -100, y: -50 });
        const { unmount } = draw("full");

        dragBy(pane(), [{ x: -30, y: -30 }]);
        unmount();

        // Flushed rather than cancelled: the coalesced write is the only one that ever reaches the
        // store, so dropping it loses up to a frame of panning.
        expect(useTransformStore.getState().transforms.get(HOLIDAY.identity ?? "")).toEqual({
            scale: 2,
            x: -130,
            y: -80,
        });
    });
});

/** The wheel, and the trackpad pinch the webview delivers as one. */
describe("the pan keys", () => {
    /** The canvas with the window's keys listening, as the shell mounts both. */
    const WithKeys = () => {
        useShortcuts();

        return <PreviewImage />;
    };

    const press = (key: string) => {
        const event = new KeyboardEvent("keydown", { key, ctrlKey: true, bubbles: true, cancelable: true });
        act(() => {
            document.body.dispatchEvent(event);
        });

        return event;
    };

    it("moves a magnified photograph a tenth of the pane the way the arrow points", () => {
        stubPaneGeometry();
        magnify({ scale: 2, x: -100, y: -50 });
        usePreviewStore.setState({ previewMode: "full" });
        render(<WithKeys />);

        // The view moves right, so the photograph moves left: a tenth of the 1000px pane.
        const right = press("ArrowRight");
        expect(right.defaultPrevented).toBe(true);
        expect(drawn()).toEqual({ x: -200, y: -50, scale: 2 });

        press("ArrowDown");
        expect(drawn()).toEqual({ x: -200, y: -110, scale: 2 });

        press("ArrowLeft");
        press("ArrowUp");
        expect(drawn()).toEqual({ x: -100, y: -50, scale: 2 });
    });

    it("stops at the photograph's own edge, as a drag does", () => {
        stubPaneGeometry();
        magnify({ scale: 2, x: -50, y: -30 });
        draw("full");

        act(() => {
            panCurrent("left");
            panCurrent("up");
        });

        expect(drawn()).toEqual({ x: 0, y: 0, scale: 2 });
    });

    it("moves from where the photograph is drawn, not from where the store last left it", () => {
        stubPaneGeometry();
        magnify({ scale: 2, x: -100, y: -50 });
        draw("full");
        // A step of the zoom keeps the middle of the pane still, so what is drawn is not what is stored.
        magnify({ scale: 3, x: -100, y: -50 });
        const before = drawn();

        act(() => {
            panCurrent("right");
        });

        expect(drawn()).toEqual({ x: before.x - 100, y: before.y, scale: 3 });
    });

    it("does nothing to a photograph drawn whole, and leaves the key alone", () => {
        stubPaneGeometry();
        usePreviewStore.setState({ previewMode: "full" });
        render(<WithKeys />);

        const event = press("ArrowRight");

        expect(event.defaultPrevented).toBe(false);
        expect(drawn()).toEqual({ x: 50, y: 0, scale: 1 });
        expect(useTransformStore.getState().transforms.get(HOLIDAY.identity ?? "")).toBeUndefined();
    });

    it("moves both panes of side by side together, by one step", () => {
        stubPaneGeometry();
        magnify({ scale: 2, x: -100, y: -50 });
        draw("side");

        act(() => {
            panCurrent("right");
        });

        expect(drawn(0)).toEqual({ x: -200, y: -50, scale: 2 });
        expect(drawn(1)).toEqual({ x: -200, y: -50, scale: 2 });
    });

    it("answers that nothing moved once the canvas is gone", () => {
        stubPaneGeometry();
        magnify({ scale: 2, x: -100, y: -50 });
        const { unmount } = draw("full");

        unmount();

        expect(panCurrent("right")).toBe(false);
    });
});

describe("the wheel", () => {
    const wheel = (element: Element, { deltaY = -100, clientX = 500, clientY = 300 } = {}) => {
        const event = new WheelEvent("wheel", { deltaY, clientX, clientY, cancelable: true, bubbles: true });
        act(() => {
            element.dispatchEvent(event);
        });

        return event;
    };

    const pane = () => document.querySelector("[data-slot='preview-pane']") as HTMLElement;

    it("does not let the gesture scroll whatever is under it", () => {
        stubPaneGeometry();
        draw("full");

        // React's synthetic `onWheel` is registered passive, so `preventDefault` on it is a silent
        // no-op - which is why the listener is a native, non-passive one.
        expect(wheel(pane()).defaultPrevented).toBe(true);
    });

    it("magnifies by one step, and reduces by one", () => {
        stubPaneGeometry();
        draw("full");

        wheel(pane());
        expect(drawn().scale).toBeCloseTo(ZOOM_MIN + ZOOM_WHEEL_STEP, 10);

        wheel(pane(), { deltaY: 100 });
        expect(drawn().scale).toBeCloseTo(ZOOM_MIN, 10);
    });

    it("keeps the point under the pointer under the pointer", () => {
        stubPaneGeometry();
        draw("full");

        // The pointer is over the middle of the photograph: 500px across a pane whose photograph
        // starts at 50 and is 900 wide, so the anchor is the halfway point of it on both axes.
        const before = drawn();
        const anchor = {
            x: (500 - before.x) / (FITTED.width * before.scale),
            y: (300 - before.y) / (FITTED.height * before.scale),
        };

        wheel(pane());

        const after = drawn();
        expect(after.x + anchor.x * FITTED.width * after.scale).toBeCloseTo(500, 6);
        expect(after.y + anchor.y * FITTED.height * after.scale).toBeCloseTo(300, 6);
        expect(after.scale).toBeGreaterThan(before.scale);
    });

    it("holds at each end of the range rather than passing it", () => {
        stubPaneGeometry();
        magnify({ scale: ZOOM_MAX, x: 0, y: 0 });
        draw("full");

        wheel(pane());
        expect(drawn().scale).toBe(ZOOM_MAX);

        magnify({ scale: ZOOM_MIN, x: 0, y: 0 });
        wheel(pane(), { deltaY: 100 });
        expect(drawn().scale).toBe(ZOOM_MIN);
    });

    it("records where the anchoring landed, not where the photograph was standing", () => {
        stubPaneGeometry();
        draw("full");

        // Anchored at the far right of the photograph, which is where the correction is largest.
        for (let notch = 0; notch < 6; notch += 1) wheel(pane(), { clientX: 950 });

        // The store is what a photograph is handed back from when it becomes current again, so a
        // store holding the pre-zoom position would return the user to a crop one notch's anchoring
        // correction away from the one on screen - `anchor * fitted * ZOOM_WHEEL_STEP`, which is 43px
        // of this 1000px pane.
        const stored = useTransformStore.getState().transforms.get(HOLIDAY.identity ?? "");
        expect(stored?.x).toBeCloseTo(drawn().x, 6);
        expect(stored?.y).toBeCloseTo(drawn().y, 6);
    });

    it("returns a photograph to the part of it that was being examined", () => {
        stubPaneGeometry();
        openFiles(SUNSET);
        act(() => useFileStore.getState().setCurrentIndex(0));
        draw("full");

        for (let notch = 0; notch < 6; notch += 1) wheel(pane(), { clientX: 950 });
        const examined = drawn();

        act(() => useFileStore.getState().setCurrentIndex(1));
        act(() => useFileStore.getState().setCurrentIndex(0));

        expect(drawn()).toEqual(examined);
    });

    it("leaves the photograph alone for a gesture that is not over a pane", () => {
        stubPaneGeometry();
        draw("full");

        const before = drawn();
        // The listener is the pane's own rather than the document's, which is what keeps a wheel over
        // the sidebar or the drawer from magnifying the canvas behind them.
        wheel(document.body);

        expect(drawn()).toEqual(before);
    });
});

/** What the canvas reports to the sidebar, which is the one thing the two regions share. */
describe("the published viewport", () => {
    const viewport = () => useTransformStore.getState().viewport;

    it("is the whole photograph while it is drawn whole", () => {
        stubPaneGeometry();
        draw("full");

        // The true answer rather than a special case: all of the photograph is being shown.
        expect(viewport()).toEqual({ x: 0, y: 0, width: 1, height: 1 });
    });

    it("is the part of a magnified photograph that is inside the pane", () => {
        stubPaneGeometry();
        magnify({ scale: 2, x: 0, y: 0 });

        draw("full");

        // 1800x1200 of photograph against a 1000x600 pane, held at its top-left corner.
        expect(viewport()).toEqual({ x: 0, y: 0, width: 1000 / 1800, height: 0.5 });
    });

    it("is written by the enhanced pane alone", () => {
        stubPaneGeometry();
        const setViewport = vi.fn();
        useTransformStore.setState({ setViewport });

        // Counted against the mode that draws the enhanced pane by itself, rather than against a
        // literal: what this is about is that the second pane adds no writer, not how many times one
        // pane resolves itself while it is being measured.
        const alone = draw("full");
        const once = setViewport.mock.calls.length;
        alone.unmount();

        setViewport.mockClear();
        draw("side");

        // Both panes are always the same size, so a second writer would be producing a value
        // identical to the first's on every frame of every pan - which is the re-render storm the
        // reference added a comparison to undo.
        expect(setViewport).toHaveBeenCalledTimes(once);
    });
});

/** The split comparison's divider, which is the only draggable thing on a canvas that also pans. */
describe("the split divider", () => {
    const divider = () => screen.getByRole("slider", { name: "Divider" });
    const clipped = () => document.querySelector("[style*='clip-path']") as HTMLElement;

    it("starts at the middle of the canvas", () => {
        stubPaneGeometry();
        draw("split");

        expect(divider()).toHaveStyle({ left: "50%" });
        expect(clipped().style.clipPath).toBe("inset(0 0 0 50%)");
        expect(divider()).toHaveAttribute("aria-valuenow", "50");
    });

    it("moves the divider and the clip together", () => {
        stubPaneGeometry();
        draw("split");

        fireEvent.pointerDown(divider(), { pointerId: 1, clientX: 250, clientY: 300 });
        fireEvent.pointerMove(divider(), { pointerId: 1, clientX: 250, clientY: 300 });

        // One number now, where the divider's `left` and the enhanced pane's clip were a matched
        // pair of literals for as long as the divider could not move.
        expect(divider()).toHaveStyle({ left: "25%" });
        expect(clipped().style.clipPath).toBe("inset(0 0 0 25%)");
    });

    it("does not move when a pane is dragged", () => {
        stubPaneGeometry();
        magnify({ scale: 2, x: -100, y: -50 });
        draw("split");

        const pane = document.querySelector("[data-slot='preview-pane']") as HTMLElement;
        fireEvent.pointerDown(pane, { pointerId: 1, clientX: 500, clientY: 300 });
        fireEvent.pointerMove(pane, { pointerId: 1, clientX: 200, clientY: 300 });

        // The panes are the pan surface, so a drag started on one and a drag started on the divider
        // have to mean different things.
        expect(divider()).toHaveStyle({ left: "50%" });
    });

    it("moves under the arrow keys, so the third comparison is operable without a mouse", () => {
        stubPaneGeometry();
        draw("split");

        fireEvent.keyDown(divider(), { key: "ArrowLeft" });
        expect(divider()).toHaveAttribute("aria-valuenow", "48");

        fireEvent.keyDown(divider(), { key: "ArrowRight" });
        fireEvent.keyDown(divider(), { key: "ArrowRight" });
        expect(divider()).toHaveAttribute("aria-valuenow", "52");
    });

    it("stays where the user put it when another image becomes the current one", () => {
        stubPaneGeometry();
        openFiles(SUNSET);
        act(() => useFileStore.getState().setCurrentIndex(0));
        draw("split");

        fireEvent.pointerDown(divider(), { pointerId: 1, clientX: 250, clientY: 300 });
        fireEvent.pointerMove(divider(), { pointerId: 1, clientX: 250, clientY: 300 });

        act(() => useFileStore.getState().setCurrentIndex(1));

        // Where the divider sits is not a property of any photograph, and nothing remounts the canvas
        // to make that true - which is what a component keyed on the file would quietly undo.
        expect(divider()).toHaveStyle({ left: "25%" });
        expect(divider()).toHaveAttribute("aria-valuenow", "25");
    });

    it("leaves neither photograph where the divider is not", () => {
        stubPaneGeometry();
        magnify({ scale: 2, x: -100, y: -50 });
        draw("split");

        const before = [drawn(0), drawn(1)];
        fireEvent.pointerDown(divider(), { pointerId: 1, clientX: 250, clientY: 300 });
        fireEvent.pointerMove(divider(), { pointerId: 1, clientX: 250, clientY: 300 });

        // A feature being compared stays at the same point on the screen as the divider passes over
        // it, which is the whole of what makes the comparison readable.
        expect([drawn(0), drawn(1)]).toEqual(before);
    });
});

describe("what the enhanced pane draws", () => {
    it("draws the source in both panes while nothing has been produced", () => {
        // Before a run lands, after one was stopped, and for an image with no enhancements at all -
        // which is the state the canvas has drawn since it had two panes.
        draw("side");

        expect(sources()).toEqual([`opai://localhost/${HOLIDAY.identity}`, `opai://localhost/${HOLIDAY.identity}`]);
    });

    it("points the enhanced pane at the result while the original keeps the source", () => {
        run = { running: false, enhanced: ENHANCED };

        draw("side");

        // Which is what makes the two-pane and split comparisons show a comparison for the first
        // time: the original pane goes on drawing the photograph as it was.
        expect(sources()).toEqual([`opai://localhost/${HOLIDAY.identity}`, `opai://localhost/${ENHANCED.identity}`]);
    });

    it("draws the result across the whole canvas in full", () => {
        run = { running: false, enhanced: ENHANCED };

        draw("full");

        expect(sources()).toEqual([`opai://localhost/${ENHANCED.identity}`]);
    });

    it("moves both panes together although they draw different pixels", () => {
        run = { running: false, enhanced: ENHANCED };
        stubPaneGeometry();
        magnify({ scale: 2, x: -100, y: -50 });

        draw("side");

        // The enhanced pane is handed the *source's* identity as its transform key, so it reads and
        // writes the same stored view. Both fit into the same box - every operation this application
        // runs preserves the aspect ratio - so one transform resolves identically in each.
        expect(drawn(0)).toEqual(drawn(1));
        expect(drawn(1)).toEqual({ x: -100, y: -50, scale: 2 });
    });

    it("falls back to the source when a result is withdrawn", () => {
        run = { running: false, enhanced: ENHANCED };
        const view = draw("side");

        expect(sources()[1]).toBe(`opai://localhost/${ENHANCED.identity}`);

        run = { running: false };
        view.rerender(<PreviewImage />);

        expect(sources()[1]).toBe(`opai://localhost/${HOLIDAY.identity}`);
    });
});

/**
 * What the two chips say, which follows the pixels rather than the enhancement stack.
 *
 * The right-hand pane draws the source until a run lands, so calling it *Enhanced* before then would
 * name the photograph by what was asked for instead of by what is on screen.
 */
describe("what the pane labels say", () => {
    const labels = () => [...document.querySelectorAll("[data-slot='preview-chip']")].map((chip) => chip.textContent);

    it("calls both panes the original while nothing has been produced", () => {
        // No enhancements, a first run still working, and a run that was stopped are one state to
        // the canvas: the enhanced pane is pointed at the source, so it is the source it is called.
        draw("side");

        expect(labels()).toEqual(["Original", "Original"]);
    });

    it("goes on calling it the original while the first run is in flight", () => {
        run = { running: true, report: progress() };

        draw("side");

        expect(labels()).toEqual(["Original", "Original"]);
    });

    it("calls the second pane enhanced once a result lands", () => {
        run = { running: false, enhanced: ENHANCED };

        draw("side");

        expect(labels()).toEqual(["Original", "Enhanced"]);
    });

    it("keeps the enhanced label while a later run works over a landed result", () => {
        // A result is held until a newer one lands, so the pane is still drawing enhanced pixels -
        // and a chip that fell back to `Original` on every keystroke in the scale field would be
        // describing a photograph that is not on screen.
        run = { running: true, enhanced: ENHANCED, report: progress() };

        draw("side");

        expect(labels()).toEqual(["Original", "Enhanced"]);
    });

    it("calls the one pane in full by whatever it is drawing", () => {
        const view = draw("full");
        expect(labels()).toEqual(["Original"]);
        view.unmount();

        run = { running: false, enhanced: ENHANCED };
        draw("full");
        expect(labels()).toEqual(["Enhanced"]);
    });
});

/**
 * The show-original key held down (`hooks/useShortcuts.ts`): every pane that draws the result draws
 * the source instead and says so, and nothing else on the canvas moves.
 */
describe("while the original is held", () => {
    const labels = () => [...document.querySelectorAll("[data-slot='preview-chip']")].map((chip) => chip.textContent);
    const hold = (showing: boolean) => act(() => usePeekStore.getState().setShowingOriginal(showing));
    const SOURCE = `opai://localhost/${HOLIDAY.identity}`;
    const RESULT = `opai://localhost/${ENHANCED.identity}`;

    beforeEach(() => {
        run = { running: false, enhanced: ENHANCED };
    });

    it("draws the original across the whole canvas in full, and the result again once released", () => {
        draw("full");

        hold(true);
        expect(sources()).toEqual([SOURCE]);
        expect(labels()).toEqual(["Original"]);

        hold(false);
        expect(sources()).toEqual([RESULT]);
        expect(labels()).toEqual(["Enhanced"]);
    });

    it("draws the original in both panes side by side, at the same magnification and position", () => {
        stubPaneGeometry();
        magnify({ scale: 2, x: -100, y: -50 });
        draw("side");
        const before = [drawn(0), drawn(1)];

        hold(true);

        expect(sources()).toEqual([SOURCE, SOURCE]);
        expect(labels()).toEqual(["Original", "Original"]);
        expect([drawn(0), drawn(1)]).toEqual(before);
        expect(useTransformStore.getState().transforms.get(HOLIDAY.identity ?? "")).toEqual({
            scale: 2,
            x: -100,
            y: -50,
        });
    });

    it("draws the original on both sides of the split, leaving the divider where it was", () => {
        stubPaneGeometry();
        draw("split");
        const divider = screen.getByRole("slider", { name: "Divider" });
        fireEvent.keyDown(divider, { key: "ArrowLeft" });

        hold(true);

        expect(sources()).toEqual([SOURCE, SOURCE]);
        expect(labels()).toEqual(["Original", "Original"]);
        expect(divider).toHaveAttribute("aria-valuenow", "48");
        expect((document.querySelector("[style*='clip-path']") as HTMLElement).style.clipPath).toBe("inset(0 0 0 48%)");
    });

    it("leaves a running enhancement's bar in place", () => {
        run = { running: true, enhanced: ENHANCED, report: progress() };
        draw("side");

        hold(true);

        expect(bar()).toBeInTheDocument();
    });
});

describe("what the canvas reports about a run", () => {
    it("shows nothing while nothing is running", () => {
        draw("side");

        expect(bar()).toBeNull();
    });

    it("shows an empty bar as soon as a run starts, before it has reported", () => {
        run = { running: true };

        draw("side");

        // A run spends its first seconds loading a model and reports nothing while it does. Waiting
        // for the first report put the bar on screen long after the enhancement was added, and the
        // interval in between read as the application having ignored it.
        expect(bar()).toHaveTextContent("Enhancing");
        expect(filled()).toHaveStyle({ transform: "translateX(-100%)" });
    });

    it("says what the enhancement being run is doing, in the interface's own language", () => {
        run = { running: true, fraction: 0.62, report: progress() };

        draw("side");

        // From the report's family rather than from its `operation`, which is the library's composed
        // English sentence and not something a front end can translate.
        expect(bar()).toHaveTextContent("Upscaling");
        expect(filled()).toHaveStyle({ transform: "translateX(-38%)" });
    });

    it("reports a fetch with its own percentage instead", () => {
        run = {
            running: true,
            fraction: 0.02,
            report: progress({ stage: "installing", installFraction: 0.41, chainFraction: 0.02 }),
        };

        draw("side");

        // The bar tracks the whole chain, and a fetch occupies only the head of one operation's
        // share of it - so without this number a multi-gigabyte download reads as a stall.
        expect(bar()).toHaveTextContent("Downloading 41%");
        expect(filled()).toHaveStyle({ transform: "translateX(-98%)" });
    });

    it("draws the run's place on the bar rather than the report's own figure", () => {
        // A face recovery is two runs - a detection and then the chain - each reporting its own
        // `0..1`. Drawing `chainFraction` filled the bar, emptied it and filled it again; the hook
        // composes the one figure that spans both, and this is the bar drawing that figure.
        run = { running: true, fraction: 0.2, report: progress({ family: "detection", chainFraction: 1 }) };

        draw("side");

        expect(filled()).toHaveStyle({ transform: "translateX(-80%)" });
    });

    it("names a detection after the enhancement that asked for it", () => {
        // The detector is not an enhancement a user adds and is offered by no menu here, so what the
        // chip says is the enhancement that needed it. This is the first thing a user sees of a face
        // recovery, because the detector is fetched on first use. See design.md D8.
        run = { running: true, report: progress({ family: "detection" }) };

        draw("side");

        expect(bar()).toHaveTextContent("Restoring faces");
    });

    it("names the enhancement generically for a family it has no name for", () => {
        // Drift: a family the library publishes and this front end does not present. It cannot be
        // written as a `Family` because the union names every one it knows, which is the point - the
        // cast is what a renamed family on the wire would actually look like here.
        run = { running: true, report: progress({ family: "segmentation" as Family }) };

        draw("side");

        // The same class of drift `lib/enhancements.ts` answers for a model the catalogue stops
        // publishing, and the same answer: draw something honest rather than throw.
        expect(bar()).toHaveTextContent("Enhancing");
    });

    it("takes the bar down once the run has ended", () => {
        run = { running: false, enhanced: ENHANCED };

        draw("side");

        expect(bar()).toBeNull();
    });
});

/*
 * Where the boxes are not.
 *
 * The chooser is the only surface that draws them: the canvas is where a user judges the
 * photograph, and boxes standing over the faces are exactly what stops them from seeing the
 * restoration they asked for. `features/faces/FaceBoxes.tsx` has one production consumer today, so
 * this is a regression test rather than a behaviour one - which is the point, since adding a second
 * consumer here is precisely what it is meant to catch.
 */
describe("the faces in the photograph on the canvas", () => {
    /** One face, shaped as the detector answers them. */
    const face = (left: number): Face => ({
        bounding_box: { min: { x: left, y: 400 }, max: { x: left + 300, y: 700 } },
        landmarks: [
            { x: left + 100, y: 500 },
            { x: left + 200, y: 500 },
            { x: left + 150, y: 600 },
            { x: left + 100, y: 650 },
            { x: left + 200, y: 650 },
        ],
        confidence: 0.9,
        restorable: true,
        key: `${left},400,${left + 300},700`,
    });

    const boxes = () => document.querySelectorAll("[data-slot='face-boxes']");

    beforeEach(() => {
        const identity = HOLIDAY.identity ?? "";

        act(() => {
            useFacesStore.getState().setFaces(identity, undefined, [face(500), face(2000)]);
            useFacesStore.getState().setFaceChoice(identity, { skipped: [face(500).key], restored: [] });
        });
    });

    afterEach(() => {
        useFacesStore.setState(useFacesStore.getInitialState(), true);
    });

    it("draws no box over the photograph, in any mode", () => {
        for (const mode of ["full", "side", "split"] as const) {
            const view = draw(mode);

            // That the panes are there is what keeps the assertion below from passing because
            // nothing was drawn at all.
            expect(panes().length).toBeGreaterThan(0);
            expect(boxes()).toHaveLength(0);

            view.unmount();
        }
    });

    it("draws no box over the enhanced result either", () => {
        run = { running: false, enhanced: ENHANCED };

        draw("side");

        expect(sources()).toContain(`opai://localhost/${ENHANCED.identity}`);
        expect(boxes()).toHaveLength(0);
    });
});

describe("setting the focus of a lens blur by clicking the photograph", () => {
    const id = HOLIDAY.identity ?? "";

    /** A lens blur focused at the middle, on f/2.8. */
    const lensBlur = (extra: Record<string, number> = {}) => ({
        family: "lens_blur" as const,
        codename: "lisbon",
        precision: "fp16" as const,
        parameters: { f_number: 2.8, focus_x: 0.5, focus_y: 0.5, ...extra },
    });

    const stackOf = () => useEnhancementStore.getState().enhancements.get(HOLIDAY.path);
    const focusOf = () => ({ x: stackOf()?.[0]?.parameters.focus_x, y: stackOf()?.[0]?.parameters.focus_y });
    const pane = (index = 0) => screen.getAllByRole("img", { name: "Preview" })[index]?.parentElement as HTMLElement;
    const ring = () => document.querySelector("[data-slot='focus-ring']") as HTMLElement | null;
    const hint = () => screen.queryByText("Click the photograph where it should be sharp. Esc to cancel.");

    /** A press and release at one point, as a click is. HOLIDAY fills 900x600 of a 1000x600 pane, from x = 50. */
    const click = (element: HTMLElement, clientX: number, clientY: number) => {
        fireEvent.pointerDown(element, { pointerId: 1, button: 0, clientX, clientY });
        fireEvent.pointerUp(element, { pointerId: 1, button: 0, clientX, clientY });
    };

    beforeEach(() => {
        stubPaneGeometry();
        useFocusPickStore.setState(useFocusPickStore.getInitialState(), true);
        useEnhancementStore.setState({ enhancements: new Map([[HOLIDAY.path, [lensBlur()]]]) });
    });

    afterEach(() => {
        useFocusPickStore.setState(useFocusPickStore.getInitialState(), true);
        useEnhancementStore.setState({ enhancements: new Map() });
    });

    it("takes a click for nothing while the preview is not armed", () => {
        draw("full");

        click(pane(), 320, 360);

        expect(focusOf()).toEqual({ x: 0.5, y: 0.5 });
        expect(hint()).toBeNull();
        expect(ring()).toBeNull();
        expect(pane().className).toContain("cursor-grab");
    });

    it("says what the next click does, and draws the cursor for it, once armed", () => {
        draw("full");

        act(() => useFocusPickStore.getState().arm(id));

        expect(hint()).toBeInTheDocument();
        expect(pane().className).toContain("cursor-crosshair");
    });

    it("sets the focus to the fractions of the photograph a click lands on, and leaves the mode", () => {
        draw("full");
        act(() => useFocusPickStore.getState().arm(id));

        // 30 percent across the 900px photograph that starts at 50px, and 60 percent down its 600px.
        click(pane(), 320, 360);

        expect(focusOf()).toEqual({ x: 0.3, y: 0.6 });
        expect(useFocusPickStore.getState().armed).toBe(false);
        expect(hint()).toBeNull();
        // The rest of the lens blur is as it was.
        expect(stackOf()?.[0]?.parameters.f_number).toBe(2.8);
    });

    it("shows the ring at the current focus while armed, in percentages of the photograph", () => {
        useEnhancementStore.setState({
            enhancements: new Map([[HOLIDAY.path, [lensBlur({ focus_x: 0.25, focus_y: 0.75 })]]]),
        });
        draw("full");

        expect(ring()).toBeNull();
        act(() => useFocusPickStore.getState().arm(id));

        expect(ring()).toHaveAttribute("aria-hidden", "true");
        expect(ring()?.style.left).toBe("25%");
        expect(ring()?.style.top).toBe("75%");
        // In the box the photograph is drawn in, which is 900x600 and starts at x = 50.
        const box = document.querySelector("[data-slot='focus-pick']") as HTMLElement;
        expect(box.style.width).toBe("900px");
        expect(box.style.height).toBe("600px");
        expect(box.style.left).toBe("50px");
    });

    it("keeps the ring where the click set the focus for a couple of seconds, and then takes it away", () => {
        vi.useFakeTimers();
        try {
            draw("full");
            act(() => useFocusPickStore.getState().arm(id));
            click(pane(), 320, 360);

            expect(ring()?.style.left).toBe("30%");
            expect(ring()?.style.top).toBe("60%");

            act(() => vi.advanceTimersByTime(1900));
            expect(ring()).not.toBeNull();

            act(() => vi.advanceTimersByTime(200));
            expect(ring()).toBeNull();
        } finally {
            vi.useRealTimers();
        }
    });

    it("does not take a drag for a click: the photograph pans, the focus stays and the mode stays", () => {
        magnify({ scale: 2, x: -100, y: -50 });
        draw("full");
        act(() => useFocusPickStore.getState().arm(id));

        fireEvent.pointerDown(pane(), { pointerId: 1, button: 0, clientX: 500, clientY: 300 });
        fireEvent.pointerMove(pane(), { pointerId: 1, clientX: 470, clientY: 280 });
        fireEvent.pointerUp(pane(), { pointerId: 1, button: 0, clientX: 470, clientY: 280 });

        expect(focusOf()).toEqual({ x: 0.5, y: 0.5 });
        expect(useFocusPickStore.getState().armed).toBe(true);
        expect(hint()).toBeInTheDocument();
    });

    it("takes a press that moves a pixel or two for a click, and one that comes back to where it started for a drag", () => {
        draw("full");
        act(() => useFocusPickStore.getState().arm(id));

        fireEvent.pointerDown(pane(), { pointerId: 1, button: 0, clientX: 500, clientY: 300 });
        fireEvent.pointerMove(pane(), { pointerId: 1, clientX: 400, clientY: 300 });
        fireEvent.pointerMove(pane(), { pointerId: 1, clientX: 500, clientY: 300 });
        fireEvent.pointerUp(pane(), { pointerId: 1, button: 0, clientX: 500, clientY: 300 });
        expect(useFocusPickStore.getState().armed).toBe(true);

        fireEvent.pointerDown(pane(), { pointerId: 1, button: 0, clientX: 500, clientY: 300 });
        fireEvent.pointerMove(pane(), { pointerId: 1, clientX: 502, clientY: 301 });
        fireEvent.pointerUp(pane(), { pointerId: 1, button: 0, clientX: 502, clientY: 301 });

        expect(useFocusPickStore.getState().armed).toBe(false);
        expect(focusOf().x).toBeCloseTo(0.5, 2);
    });

    it("ignores a click in the margin beside the photograph, and stays armed", () => {
        draw("full");
        act(() => useFocusPickStore.getState().arm(id));

        // The photograph runs from x = 50 to 950 of the 1000px pane.
        click(pane(), 20, 300);

        expect(focusOf()).toEqual({ x: 0.5, y: 0.5 });
        expect(useFocusPickStore.getState().armed).toBe(true);
    });

    it("ignores a press that is not the primary button", () => {
        draw("full");
        act(() => useFocusPickStore.getState().arm(id));

        fireEvent.pointerDown(pane(), { pointerId: 1, button: 2, clientX: 320, clientY: 360 });
        fireEvent.pointerUp(pane(), { pointerId: 1, button: 2, clientX: 320, clientY: 360 });

        expect(focusOf()).toEqual({ x: 0.5, y: 0.5 });
        expect(useFocusPickStore.getState().armed).toBe(true);
    });

    it("means the same point in a magnified photograph as in a fitted one", () => {
        magnify({ scale: 2, x: -100, y: -50 });
        draw("full");
        act(() => useFocusPickStore.getState().arm(id));

        // The photograph is drawn 1800x1200 from (-100, -50): 600 across is 700 of 1800, and 350 down is 400 of 1200.
        click(pane(), 600, 350);

        expect(focusOf().x).toBeCloseTo(0.389, 3);
        expect(focusOf().y).toBeCloseTo(0.333, 3);
    });

    it("sets the same point from either pane of the side by side comparison", () => {
        draw("side");
        act(() => useFocusPickStore.getState().arm(id));

        click(pane(0), 320, 360);

        expect(focusOf()).toEqual({ x: 0.3, y: 0.6 });

        act(() => useFocusPickStore.getState().arm(id));
        click(pane(1), 590, 120);

        expect(focusOf()).toEqual({ x: 0.6, y: 0.2 });
    });

    it("draws the hint and the ring once, not once for each pane", () => {
        draw("side");

        act(() => useFocusPickStore.getState().arm(id));

        expect(screen.getAllByText("Click the photograph where it should be sharp. Esc to cancel.")).toHaveLength(1);
        // A ring in each pane, so the focus is visible whichever half is being looked at.
        expect(document.querySelectorAll("[data-slot='focus-ring']")).toHaveLength(2);
    });

    it("leaves the mode, with the focus as it was, on Escape", () => {
        draw("full");
        act(() => useFocusPickStore.getState().arm(id));

        fireEvent.keyDown(window, { key: "Escape" });

        expect(useFocusPickStore.getState().armed).toBe(false);
        expect(hint()).toBeNull();
        expect(focusOf()).toEqual({ x: 0.5, y: 0.5 });
    });

    it("keeps the mode on an Escape that something else has taken", () => {
        draw("full");
        act(() => useFocusPickStore.getState().arm(id));

        // A dialog's own layer prevents the default of the Escape that closes it.
        const taken = new KeyboardEvent("keydown", { key: "Escape", cancelable: true });
        taken.preventDefault();
        act(() => {
            window.dispatchEvent(taken);
        });

        expect(useFocusPickStore.getState().armed).toBe(true);
    });

    it("keeps the mode on an Escape while a dialog is open over the photograph", () => {
        draw("full");
        act(() => useFocusPickStore.getState().arm(id));
        const dialog = document.body.appendChild(document.createElement("div"));
        dialog.setAttribute("role", "dialog");
        dialog.setAttribute("data-state", "open");

        fireEvent.keyDown(window, { key: "Escape" });
        expect(useFocusPickStore.getState().armed).toBe(true);

        // One that is closing is not open.
        dialog.setAttribute("data-state", "closed");
        fireEvent.keyDown(window, { key: "Escape" });
        expect(useFocusPickStore.getState().armed).toBe(false);

        dialog.remove();
    });

    it("hides the ring from assistive technology, since the status line says what the mode is", () => {
        draw("full");
        act(() => useFocusPickStore.getState().arm(id));

        expect(ring()).toHaveAttribute("aria-hidden", "true");
        expect(screen.getByRole("status")).toHaveTextContent("Click the photograph");
    });

    it("leaves the mode when another photograph becomes the current one", () => {
        openFiles(SUNSET);
        useFileStore.getState().setCurrentIndex(0);
        draw("full");
        act(() => useFocusPickStore.getState().arm(id));

        act(() => useFileStore.getState().setCurrentIndex(1));

        expect(useFocusPickStore.getState().armed).toBe(false);
    });

    it("leaves the mode when the lens blur is taken off the stack", () => {
        draw("full");
        act(() => useFocusPickStore.getState().arm(id));

        act(() => useEnhancementStore.setState({ enhancements: new Map([[HOLIDAY.path, []]]) }));

        expect(useFocusPickStore.getState().armed).toBe(false);
        expect(hint()).toBeNull();
    });

    it("is not armed for a photograph the Set focus button was not pressed over", () => {
        draw("full");

        act(() => useFocusPickStore.getState().arm(SUNSET.identity ?? ""));

        expect(hint()).toBeNull();
        expect(pane().className).toContain("cursor-grab");
    });
});

describe("painting the mask", () => {
    const pane = (index = 0) => document.querySelectorAll("[data-slot='preview-pane']")[index] as HTMLElement;
    const store = () => useMaskStore.getState();

    // HOLIDAY drawn 900x600 in a 1000x600 pane sits at x = 50, so the photograph's middle is (500, 300) and a tenth of
    // its width is 90 pixels.
    const at = (x: number, y: number) => ({ pointerId: 1, button: 0, clientX: 50 + x * 900, clientY: y * 600 });

    beforeEach(() => {
        useMaskStore.setState(useMaskStore.getInitialState(), true);
        useEnhancementStore.setState({
            autopilot: true,
            enhancements: new Map([
                [HOLIDAY.path, [{ family: "sharpen", codename: "moscow", precision: "fp32", parameters: {} }]],
            ]),
        });
        stubPaneGeometry();
    });

    const paintMode = () => act(() => store().startEditing(HOLIDAY.path));

    it("paints a stroke along the drag, in fractions of the photograph, committed when the pointer comes up", () => {
        paintMode();
        draw("full");

        fireEvent.pointerDown(pane(), at(0.5, 0.5));
        expect(store().live?.stroke.points).toEqual([0.5, 0.5]);
        expect(store().masks.get(HOLIDAY.path)).toBeUndefined();

        fireEvent.pointerMove(pane(), at(0.6, 0.5));
        fireEvent.pointerMove(pane(), at(0.7, 0.4));
        expect(store().live?.stroke.points).toEqual([0.5, 0.5, 0.6, 0.5, 0.7, 0.4]);

        fireEvent.pointerUp(pane(), at(0.8, 0.4));

        expect(store().live).toBeUndefined();
        expect(store().masks.get(HOLIDAY.path)?.strokes).toEqual([
            { erase: false, radius: 0.025, softness: 0.5, points: [0.5, 0.5, 0.6, 0.5, 0.7, 0.4, 0.8, 0.4] },
        ]);
    });

    it("paints the area to remove while that is what is edited, into the removal and not the mask", () => {
        act(() => store().startEditing(HOLIDAY.path, "removal"));
        draw("side");

        fireEvent.pointerDown(pane(1), at(0.5, 0.5));
        fireEvent.pointerUp(pane(1), at(0.6, 0.5));

        expect(store().removals.get(HOLIDAY.path)?.strokes).toHaveLength(1);
        expect(store().masks.get(HOLIDAY.path)).toBeUndefined();
        expect(pane(1).querySelector("[data-slot='mask-overlay']")?.getAttribute("data-target")).toBe("removal");
    });

    it("clamps a stroke to the photograph where the pointer leaves it, and keeps following", () => {
        paintMode();
        draw("full");

        fireEvent.pointerDown(pane(), at(0.9, 0.5));
        fireEvent.pointerMove(pane(), { ...at(1, 0.5), clientX: 2000, clientY: -300 });
        fireEvent.pointerUp(pane(), { ...at(1, 0.5), clientX: 2000, clientY: -300 });

        expect(store().masks.get(HOLIDAY.path)?.strokes[0]?.points).toEqual([0.9, 0.5, 1, 0]);
        expect(pane().hasPointerCapture(1)).toBe(false);
    });

    it("erases with Option held, for that stroke only", () => {
        paintMode();
        draw("full");

        fireEvent.pointerDown(pane(), { ...at(0.5, 0.5), altKey: true });
        fireEvent.pointerUp(pane(), at(0.5, 0.5));
        fireEvent.pointerDown(pane(), at(0.2, 0.2));
        fireEvent.pointerUp(pane(), at(0.2, 0.2));

        expect(
            store()
                .masks.get(HOLIDAY.path)
                ?.strokes.map((stroke) => stroke.erase),
        ).toEqual([true, false]);
    });

    it("paints with the tool's size and softness, and erases when the tool does", () => {
        paintMode();
        act(() => {
            store().setMode("erase");
            store().setSize(0.1);
            store().setSoftness(0.8);
        });
        draw("full");

        fireEvent.pointerDown(pane(), at(0.5, 0.5));
        fireEvent.pointerUp(pane(), at(0.5, 0.5));

        expect(store().masks.get(HOLIDAY.path)?.strokes[0]).toMatchObject({ erase: true, radius: 0.05, softness: 0.8 });
    });

    it("starts nothing for a press beside the photograph", () => {
        paintMode();
        draw("full");

        // The pane is 1000 wide and the photograph 900, so the first 50 pixels are margin.
        fireEvent.pointerDown(pane(), { pointerId: 1, button: 0, clientX: 20, clientY: 300 });
        fireEvent.pointerUp(pane(), { pointerId: 1, clientX: 20, clientY: 300 });

        expect(store().live).toBeUndefined();
        expect(store().masks.get(HOLIDAY.path)).toBeUndefined();
        expect(pane().hasPointerCapture(1)).toBe(false);
    });

    it("pans, and paints nothing, while Space is held", () => {
        paintMode();
        magnify({ scale: 2, x: -100, y: -50 });
        draw("full");
        act(() => store().setPanning(true));

        fireEvent.pointerDown(pane(), at(0.5, 0.5));
        fireEvent.pointerMove(pane(), at(0.6, 0.5));
        fireEvent.pointerUp(pane(), at(0.6, 0.5));

        expect(store().live).toBeUndefined();
        expect(store().masks.get(HOLIDAY.path)).toBeUndefined();
        expect(pane().className).toContain("cursor-grab");
    });

    it("lets a stroke finish when Space is pressed half way through it", () => {
        paintMode();
        draw("full");

        fireEvent.pointerDown(pane(), at(0.3, 0.5));
        act(() => store().setPanning(true));
        fireEvent.pointerMove(pane(), at(0.4, 0.5));
        fireEvent.pointerUp(pane(), at(0.5, 0.5));

        expect(store().masks.get(HOLIDAY.path)?.strokes[0]?.points).toEqual([0.3, 0.5, 0.4, 0.5, 0.5, 0.5]);
    });

    it("drops a stroke the system takes away", () => {
        paintMode();
        draw("full");

        fireEvent.pointerDown(pane(), at(0.3, 0.5));
        fireEvent.pointerCancel(pane(), { pointerId: 1 });

        expect(store().live).toBeUndefined();
        expect(store().masks.get(HOLIDAY.path)).toBeUndefined();
    });

    it("pans, as it always has, when the mask is not being edited", () => {
        draw("full");

        fireEvent.pointerDown(pane(), at(0.5, 0.5));
        fireEvent.pointerUp(pane(), at(0.5, 0.5));

        expect(store().live).toBeUndefined();
        expect(store().masks.size).toBe(0);
        expect(pane().className).toContain("cursor-grab");
        expect(document.querySelector("[data-slot='brush-cursor']")).toBeNull();
    });

    it("paints nothing while the stack is empty", () => {
        useEnhancementStore.setState({ enhancements: new Map() });
        paintMode();
        draw("full");

        fireEvent.pointerDown(pane(), at(0.5, 0.5));

        expect(store().live).toBeUndefined();
    });

    it("paints the same mask from either pane of a side by side comparison", () => {
        paintMode();
        draw("side");

        fireEvent.pointerDown(pane(0), at(0.2, 0.2));
        fireEvent.pointerUp(pane(0), at(0.2, 0.2));
        fireEvent.pointerDown(pane(1), at(0.7, 0.7));
        fireEvent.pointerUp(pane(1), at(0.7, 0.7));

        expect(
            store()
                .masks.get(HOLIDAY.path)
                ?.strokes.map((stroke) => stroke.points),
        ).toEqual([
            [0.2, 0.2],
            [0.7, 0.7],
        ]);
    });

    it("paints the same mask in the split comparison", () => {
        paintMode();
        draw("split");

        fireEvent.pointerDown(pane(1), at(0.7, 0.7));
        fireEvent.pointerUp(pane(1), at(0.7, 0.7));

        expect(store().masks.get(HOLIDAY.path)?.strokes).toHaveLength(1);
    });

    it("tints the enhanced pane alone, and only while the tint is on", () => {
        act(() => {
            store().beginStroke(HOLIDAY.path, 0.5, 0.5, false);
            store().endStroke(0.6, 0.5);
        });
        paintMode();
        draw("side");

        expect(document.querySelectorAll("[data-slot='mask-overlay']")).toHaveLength(1);
        expect(pane(1).querySelector("[data-slot='mask-overlay']")).not.toBeNull();

        act(() => store().setOverlay(false));
        expect(document.querySelector("[data-slot='mask-overlay']")).toBeNull();
    });

    it("draws no tint when the mask is not being edited", () => {
        act(() => {
            store().beginStroke(HOLIDAY.path, 0.5, 0.5, false);
            store().endStroke(0.6, 0.5);
        });
        draw("side");

        expect(document.querySelector("[data-slot='mask-overlay']")).toBeNull();
    });

    it("maps the pointer to the photograph through the zoom and the pan, not the pane", () => {
        paintMode();
        // Twice as large, so 1800x1200, with its corner at (-100, -50) in the pane.
        magnify({ scale: 2, x: -100, y: -50 });
        draw("full");

        fireEvent.pointerDown(pane(), { pointerId: 1, button: 0, clientX: 500, clientY: 300 });
        fireEvent.pointerMove(pane(), { pointerId: 1, clientX: 800, clientY: 500 });
        // Beyond the photograph's far corner, which is at (1700, 1150) in the pane, and short of nowhere on the near
        // one.
        fireEvent.pointerUp(pane(), { pointerId: 1, clientX: 2500, clientY: 900 });

        // (500 + 100) / 1800 across and (300 + 50) / 1200 down.
        const [first, second, third, fourth, fifth, sixth] = store().masks.get(HOLIDAY.path)?.strokes[0]?.points ?? [];
        expect(first).toBeCloseTo(600 / 1800, 10);
        expect(second).toBeCloseTo(350 / 1200, 10);
        expect(third).toBeCloseTo(900 / 1800, 10);
        expect(fourth).toBeCloseTo(550 / 1200, 10);
        // Clamped to the photograph's edge, not to the pane's.
        expect(fifth).toBe(1);
        expect(sixth).toBeCloseTo(950 / 1200, 10);
    });

    it("counts a press in what was the margin at fit as on the photograph once it is magnified", () => {
        paintMode();
        magnify({ scale: 2, x: -100, y: -50 });
        draw("full");

        // At fit this is the 50 pixels of margin; magnified the photograph reaches under it.
        fireEvent.pointerDown(pane(), { pointerId: 1, button: 0, clientX: 20, clientY: 300 });

        expect(store().live?.stroke.points).toEqual([expect.closeTo(120 / 1800, 10), expect.closeTo(350 / 1200, 10)]);
    });

    describe("on a Mac, where Ctrl and a click is a right click", () => {
        beforeEach(() => {
            vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
        });

        it("starts no stroke", () => {
            paintMode();
            draw("full");

            fireEvent.pointerDown(pane(), { ...at(0.5, 0.5), ctrlKey: true });

            expect(store().live).toBeUndefined();
            expect(pane().hasPointerCapture(1)).toBe(false);
        });

        it("still paints with a plain press", () => {
            paintMode();
            draw("full");

            fireEvent.pointerDown(pane(), at(0.5, 0.5));

            expect(store().live).toBeDefined();
        });
    });

    it("paints with Ctrl held anywhere else, where it is only a key", () => {
        vi.spyOn(navigator, "platform", "get").mockReturnValue("Win32");
        paintMode();
        draw("full");

        fireEvent.pointerDown(pane(), { ...at(0.5, 0.5), ctrlKey: true });

        expect(store().live).toBeDefined();
    });

    it("ends the stroke where it last was when the capture is lost without a release", () => {
        paintMode();
        draw("full");

        fireEvent.pointerDown(pane(), at(0.3, 0.5));
        fireEvent.pointerMove(pane(), at(0.4, 0.5));
        fireEvent.lostPointerCapture(pane(), { pointerId: 1 });

        expect(store().live).toBeUndefined();
        expect(store().masks.get(HOLIDAY.path)?.strokes[0]?.points).toEqual([0.3, 0.5, 0.4, 0.5]);

        // A release that comes after it has nothing left to end.
        fireEvent.pointerUp(pane(), at(0.5, 0.5));
        expect(store().masks.get(HOLIDAY.path)?.strokes).toHaveLength(1);
    });

    it("does not end a stroke twice for the capture a release gives up", () => {
        paintMode();
        draw("full");

        fireEvent.pointerDown(pane(), at(0.3, 0.5));
        fireEvent.pointerUp(pane(), at(0.4, 0.5));
        fireEvent.lostPointerCapture(pane(), { pointerId: 1 });

        expect(store().masks.get(HOLIDAY.path)?.strokes).toHaveLength(1);
    });

    it("draws the ring inside the pane, except in the split comparison, where nothing clips it", () => {
        paintMode();
        draw("side");

        expect(pane(0).querySelector("[data-slot='brush-cursor']")).not.toBeNull();
        expect(pane(1).querySelector("[data-slot='brush-cursor']")).not.toBeNull();
    });

    it("draws the split comparison's ring in the canvas, so a stroke that crosses the divider keeps it", () => {
        paintMode();
        draw("split");

        const rings = [...document.querySelectorAll("[data-slot='brush-cursor']")];
        expect(rings).toHaveLength(2);

        // Neither is inside a pane, which the enhanced one is clipped to the divider's right.
        for (const ring of rings) {
            expect(ring.closest("[data-slot='preview-pane']")).toBeNull();
            expect(ring.parentElement).toBe(pane(0).parentElement);
        }
    });

    it("draws the ring at the brush's size, the same fraction of the photograph at every zoom", () => {
        paintMode();
        draw("full");

        // A diameter of 5 percent of the longer side, which is 900 pixels drawn: a ring of 45 across.
        const ring = () => document.querySelector("[data-slot='brush-radius']") as HTMLElement;
        expect(ring().style.width).toBe("45px");

        magnify({ scale: 2, x: -100, y: -50 });

        expect(ring().style.width).toBe("90px");
    });
});

describe("the brush and the focus pick", () => {
    const id = HOLIDAY.identity ?? "";

    beforeEach(() => {
        stubPaneGeometry();
        useMaskStore.setState(useMaskStore.getInitialState(), true);
        useFocusPickStore.setState(useFocusPickStore.getInitialState(), true);
        useEnhancementStore.setState({
            enhancements: new Map([
                [
                    HOLIDAY.path,
                    [
                        {
                            family: "lens_blur",
                            codename: "lisbon",
                            precision: "fp16",
                            parameters: { f_number: 2.8, focus_x: 0.5, focus_y: 0.5 },
                        },
                    ],
                ],
            ]),
        });
    });

    afterEach(() => {
        useMaskStore.setState(useMaskStore.getInitialState(), true);
        useFocusPickStore.setState(useFocusPickStore.getInitialState(), true);
        useEnhancementStore.setState({ enhancements: new Map() });
    });

    it("leaves the mask's editing when the focus pick is armed", () => {
        act(() => useMaskStore.getState().startEditing(HOLIDAY.path));
        draw("full");

        act(() => useFocusPickStore.getState().arm(id));

        expect(useMaskStore.getState().editing).toBeUndefined();
        expect(useFocusPickStore.getState().armed).toBe(true);
    });

    it("leaves the focus pick when the mask is edited", () => {
        draw("full");
        act(() => useFocusPickStore.getState().arm(id));

        act(() => useMaskStore.getState().startEditing(HOLIDAY.path));

        expect(useFocusPickStore.getState().armed).toBe(false);
        expect(useMaskStore.getState().editing).toEqual({ path: HOLIDAY.path, target: "limit" });
    });
});
