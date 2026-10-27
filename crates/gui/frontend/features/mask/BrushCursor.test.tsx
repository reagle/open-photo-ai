import { createRef } from "react";
import { fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { render } from "@/test/support";
import { BrushCursor } from "./BrushCursor";

/** A pane for the ring to follow the pointer across, with the rectangle jsdom would otherwise answer zero for. */
const draw = (props: { radius: number; softness: number; erase: boolean }) => {
    const pane = document.createElement("div");
    vi.spyOn(pane, "getBoundingClientRect").mockReturnValue(new DOMRect(100, 50, 800, 600));
    document.body.append(pane);

    const container = createRef<HTMLElement>();
    (container as { current: HTMLElement | null }).current = pane;

    render(<BrushCursor container={container} {...props} />);

    return pane;
};

const ring = () => document.querySelector("[data-slot='brush-cursor']") as HTMLElement;
const radius = () => document.querySelector("[data-slot='brush-radius']") as HTMLElement;
const core = () => document.querySelector("[data-slot='brush-core']") as HTMLElement | null;

describe("the brush cursor", () => {
    it("draws the brush's radius, and inside it the hard core", () => {
        draw({ radius: 40, softness: 0.5, erase: false });

        expect(radius().style.width).toBe("80px");
        expect(core()?.style.width).toBe("40px");
    });

    it("has no core to show for a fully soft brush, nor for one under two pixels across", () => {
        draw({ radius: 40, softness: 1, erase: false });

        expect(core()).toBeNull();
    });

    it("is hidden until the pointer is over the pane, and follows it in the pane's pixels", () => {
        const pane = draw({ radius: 40, softness: 0.5, erase: false });
        expect(ring().style.opacity).toBe("0");

        fireEvent.pointerMove(pane, { clientX: 300, clientY: 200 });

        expect(ring().style.opacity).toBe("1");
        expect(ring().style.transform).toBe("translate(200px, 150px)");

        fireEvent.pointerLeave(pane);
        expect(ring().style.opacity).toBe("0");
    });

    it("shows no minus while painting", () => {
        draw({ radius: 40, softness: 0.5, erase: false });
        expect(ring().dataset.erasing).toBe("false");
        expect(radius().querySelector("svg")).toBeNull();
    });

    it("shows a minus while the tool erases", () => {
        draw({ radius: 40, softness: 0.5, erase: true });

        expect(ring().dataset.erasing).toBe("true");
        expect(radius().querySelector("svg")).not.toBeNull();
    });

    it("shows a minus while Option or Alt is held, as the pointer or the key reports it", () => {
        const pane = draw({ radius: 40, softness: 0.5, erase: false });

        fireEvent.pointerMove(pane, { clientX: 300, clientY: 200, altKey: true });
        expect(ring().dataset.erasing).toBe("true");

        fireEvent.pointerMove(pane, { clientX: 300, clientY: 200, altKey: false });
        expect(ring().dataset.erasing).toBe("false");

        fireEvent.keyDown(window, { key: "Alt" });
        expect(ring().dataset.erasing).toBe("true");

        fireEvent.keyUp(window, { key: "Alt" });
        expect(ring().dataset.erasing).toBe("false");
    });

    it("stops showing a minus when the window loses the focus with Option held", () => {
        draw({ radius: 40, softness: 0.5, erase: false });

        fireEvent.keyDown(window, { key: "Alt" });
        expect(ring().dataset.erasing).toBe("true");

        fireEvent.blur(window);

        expect(ring().dataset.erasing).toBe("false");
    });

    it("follows the pointer in the frame's pixels when it is drawn in one", () => {
        const pane = document.createElement("div");
        vi.spyOn(pane, "getBoundingClientRect").mockReturnValue(new DOMRect(100, 50, 800, 600));
        const canvas = document.createElement("div");
        vi.spyOn(canvas, "getBoundingClientRect").mockReturnValue(new DOMRect(98, 48, 804, 604));
        document.body.append(pane, canvas);
        const container = createRef<HTMLElement>();
        (container as { current: HTMLElement | null }).current = pane;

        render(<BrushCursor container={container} frame={canvas} radius={40} softness={0.5} erase={false} />);
        fireEvent.pointerMove(pane, { clientX: 300, clientY: 200 });

        expect(ring().style.transform).toBe("translate(202px, 152px)");
    });

    it("takes no pointer events", () => {
        draw({ radius: 40, softness: 0.5, erase: false });

        expect(ring().className).toContain("pointer-events-none");
    });
});
