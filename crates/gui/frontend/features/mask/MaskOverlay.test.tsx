import { act, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import { maskPreview } from "@/ipc/mask";
import { useMaskStore } from "@/stores/mask";
import { render } from "@/test/support";
import { MaskOverlay } from "./MaskOverlay";

// Rust draws the picture; what is asked of it, and what is done with each answer, is what is under test.
vi.mock("@/ipc/mask", () => ({ maskPreview: vi.fn() }));

const asked = maskPreview as unknown as Mock;

const PATH = "/pictures/holiday.jpg";
const BOX = { x: 10, y: 20, width: 800, height: 600 };

/** One request the test answers by hand. */
type Pending = { resolve: (bytes: ArrayBuffer) => void; reject: (error: unknown) => void };
let pending: Pending[] = [];

const PNG = (marker: number) => new Uint8Array([137, 80, 78, 71, marker]).buffer;

const store = () => useMaskStore.getState();

const stroke = (x: number) =>
    act(() => {
        store().beginStroke(PATH, x, 0.5, false);
        store().endStroke(x + 0.1, 0.5);
    });

const answer = async (at: number, marker: number) => {
    await act(async () => pending[at]?.resolve(PNG(marker)));
};

const tint = () => document.querySelector("[data-slot='mask-tint']") as HTMLElement | null;

beforeEach(() => {
    useMaskStore.setState(useMaskStore.getInitialState(), true);
    pending = [];
    asked.mockReset();
    asked.mockImplementation(
        () =>
            new Promise<ArrayBuffer>((resolve, reject) => {
                pending.push({ resolve, reject });
            }),
    );
});

describe("the mask overlay", () => {
    it("asks for nothing, and draws nothing, for a photograph with no mask", () => {
        render(<MaskOverlay path={PATH} box={BOX} />);

        expect(asked).not.toHaveBeenCalled();
        expect(tint()).toBeNull();
    });

    it("asks Rust for the mask at the box's size in device pixels, rounded up to a step", () => {
        stroke(0.2);
        render(<MaskOverlay path={PATH} box={BOX} />);

        // jsdom's density is one: 800 pixels is four steps of 256.
        expect(asked).toHaveBeenCalledTimes(1);
        // The points as they go over the wire: rounded, so `0.2 + 0.1` is `0.3` and not `0.30000000000000004`.
        expect(asked).toHaveBeenCalledWith(
            {
                strokes: [{ erase: false, radius: 0.025, softness: 0.5, points: [0.2, 0.5, 0.3, 0.5] }],
                inverted: false,
            },
            1024,
            768,
        );
    });

    it("shapes the tint by the picture that comes back", async () => {
        stroke(0.2);
        render(<MaskOverlay path={PATH} box={BOX} />);
        expect(tint()).toBeNull();

        await answer(0, 1);

        expect(tint()?.style.maskImage).toContain("data:image/png;base64,");
        expect(tint()?.style.maskSize).toBe("100% 100%");
    });

    it("lays itself over the box the photograph is drawn in, and takes no pointer", () => {
        stroke(0.2);
        render(<MaskOverlay path={PATH} box={BOX} />);

        const overlay = document.querySelector("[data-slot='mask-overlay']") as HTMLElement;

        // Placed by a transform on a layer of its own, as the photograph is, and not by `left` and `top`.
        expect(overlay.style).toMatchObject({
            width: "800px",
            height: "600px",
            transform: "translate(10px, 20px)",
            willChange: "transform",
        });
        expect(overlay.style.left).toBe("");
        expect(overlay.style.top).toBe("");
        expect(overlay.className).toContain("pointer-events-none");
    });

    it("moves with the box by its transform alone", () => {
        stroke(0.2);
        const { rerender } = render(<MaskOverlay path={PATH} box={BOX} />);

        rerender(<MaskOverlay path={PATH} box={{ ...BOX, x: -30, y: 5 }} />);

        const overlay = document.querySelector("[data-slot='mask-overlay']") as HTMLElement;
        expect(overlay.style.transform).toBe("translate(-30px, 5px)");
    });

    it("has one request in flight, and keeps showing a picture that a newer stroke has superseded", async () => {
        stroke(0.2);
        render(<MaskOverlay path={PATH} box={BOX} />);
        stroke(0.5);
        stroke(0.7);

        // Two more strokes while the first request is out: none of them is a request yet.
        expect(asked).toHaveBeenCalledTimes(1);

        await answer(0, 1);

        // What came back is the same mask with fewer strokes: shown, with the two it lacks drawn over it, and the
        // newest is asked for.
        expect(tint()).not.toBeNull();
        expect(document.querySelectorAll("[data-slot='mask-strokes'] path")).toHaveLength(2);
        expect(asked).toHaveBeenCalledTimes(2);
        expect((asked.mock.calls[1] as [{ strokes: unknown[] }])[0].strokes).toHaveLength(3);

        await answer(1, 2);

        expect(tint()).not.toBeNull();
        expect(document.querySelector("[data-slot='mask-strokes']")).toBeNull();
        expect(asked).toHaveBeenCalledTimes(2);
    });

    it("drops a picture that an invert has made one of something else", async () => {
        stroke(0.2);
        render(<MaskOverlay path={PATH} box={BOX} />);
        act(() => store().invert(PATH));

        await answer(0, 1);

        expect(tint()).toBeNull();
        expect(asked).toHaveBeenCalledTimes(2);
        expect(asked).toHaveBeenLastCalledWith(expect.objectContaining({ inverted: true }), 1024, 768);
    });

    it("drops a picture that an undo has made one of something else", async () => {
        stroke(0.2);
        stroke(0.5);
        render(<MaskOverlay path={PATH} box={BOX} />);
        act(() => store().undo(PATH));

        await answer(0, 1);

        // Two strokes were drawn and one is painted: the picture shows a stroke that is gone.
        expect(tint()).toBeNull();
        expect(asked).toHaveBeenCalledTimes(2);
    });

    it("shows a stroke just committed as the pointer drew it until its picture arrives", async () => {
        stroke(0.2);
        render(<MaskOverlay path={PATH} box={BOX} />);
        await answer(0, 1);
        expect(document.querySelector("[data-slot='mask-strokes']")).toBeNull();

        stroke(0.5);

        // The new picture is on its way; the stroke is not left out in the meantime.
        expect(document.querySelectorAll("[data-slot='mask-strokes'] path")).toHaveLength(1);

        await answer(1, 2);

        expect(document.querySelector("[data-slot='mask-strokes']")).toBeNull();
    });

    it("draws strokes as what they do to the tint: painting adds it, and erasing takes it away", () => {
        render(<MaskOverlay path={PATH} box={BOX} />);

        act(() => {
            store().beginStroke(PATH, 0.2, 0.2, false);
            store().extendStroke(0.5, 0.5, 1.33);
        });
        const group = () => document.querySelector("[data-slot='mask-strokes'] g");
        expect(group()?.getAttribute("class")).toBe("text-primary");

        act(() => store().cancelStroke());
        act(() => {
            store().beginStroke(PATH, 0.2, 0.2, true);
            store().extendStroke(0.5, 0.5, 1.33);
        });
        expect(group()?.getAttribute("class")).toBe("text-foreground");
    });

    it("draws them the other way up once the mask is turned over, live and committed alike", async () => {
        stroke(0.2);
        render(<MaskOverlay path={PATH} box={BOX} />);
        await answer(0, 1);
        act(() => store().invert(PATH));
        await answer(1, 2);
        const groups = () => [...document.querySelectorAll("[data-slot='mask-strokes'] g")];

        // Painting into a mask that is turned over takes the tint away.
        act(() => {
            store().beginStroke(PATH, 0.2, 0.2, false);
            store().extendStroke(0.5, 0.5, 1.33);
        });
        expect(groups().map((group) => group.getAttribute("class"))).toEqual(["text-foreground"]);

        act(() => store().cancelStroke());
        act(() => {
            store().beginStroke(PATH, 0.2, 0.2, true);
            store().extendStroke(0.5, 0.5, 1.33);
        });
        expect(groups().map((group) => group.getAttribute("class"))).toEqual(["text-primary"]);

        // And a stroke committed whose picture has not arrived yet.
        act(() => store().cancelStroke());
        act(() => {
            store().beginStroke(PATH, 0.6, 0.6, false);
            store().endStroke(0.7, 0.7);
        });
        expect(groups().map((group) => group.getAttribute("class"))).toEqual(["text-foreground"]);
    });

    it("draws the stroke under the pointer, and no request for it", () => {
        render(<MaskOverlay path={PATH} box={BOX} />);

        act(() => {
            store().beginStroke(PATH, 0.2, 0.2, false);
            store().extendStroke(0.5, 0.5, 1.33);
        });

        expect(asked).not.toHaveBeenCalled();
        expect(document.querySelectorAll("[data-slot='mask-strokes'] path")).toHaveLength(1);

        act(() => store().cancelStroke());
        expect(document.querySelector("[data-slot='mask-strokes']")).toBeNull();
    });

    it("draws a stroke the pointer never moved as a dot", () => {
        render(<MaskOverlay path={PATH} box={BOX} />);

        act(() => store().beginStroke(PATH, 0.2, 0.2, false));

        expect(document.querySelectorAll("[data-slot='mask-strokes'] circle")).toHaveLength(1);
    });

    it("sizes the live stroke by the brush's radius in the box's own pixels", () => {
        render(<MaskOverlay path={PATH} box={BOX} />);

        act(() => {
            store().beginStroke(PATH, 0.2, 0.2, false);
            store().extendStroke(0.5, 0.5, 1.33);
        });

        // A radius of 2.5 percent of the longer side, 800 pixels: 20, so a line 40 wide.
        expect(document.querySelector("[data-slot='mask-strokes'] path")?.getAttribute("stroke-width")).toBe("40");
    });

    it("asks again at a new size only when the box moves to another step", () => {
        stroke(0.2);
        const { rerender } = render(<MaskOverlay path={PATH} box={BOX} />);
        expect(asked).toHaveBeenCalledTimes(1);

        rerender(<MaskOverlay path={PATH} box={{ ...BOX, width: 830, height: 622 }} />);
        expect(asked).toHaveBeenCalledTimes(1);
    });

    it("takes the tint away when the mask is undone back to nothing", async () => {
        stroke(0.2);
        render(<MaskOverlay path={PATH} box={BOX} />);
        await answer(0, 1);
        expect(tint()).not.toBeNull();

        act(() => store().undo(PATH));

        expect(tint()).toBeNull();
    });

    it("draws nothing for a box that has not been measured", () => {
        stroke(0.2);
        render(<MaskOverlay path={PATH} box={{ x: 0, y: 0, width: 0, height: 0 }} />);

        expect(asked).not.toHaveBeenCalled();
        expect(screen.queryByRole("img")).toBeNull();
        expect(document.querySelector("[data-slot='mask-overlay']")).toBeNull();
    });
});

describe("the overlay while the area to remove is painted", () => {
    it("shows the removal's region alone, in its own colour, never the mask's", async () => {
        stroke(0.2);
        act(() => store().startEditing(PATH, "removal"));
        stroke(0.6);
        render(<MaskOverlay path={PATH} box={BOX} />);

        // The removal's one stroke is what is asked for, not the limiting mask's.
        expect(asked).toHaveBeenCalledTimes(1);
        expect(asked.mock.calls[0]?.[0]).toEqual({
            strokes: [{ erase: false, radius: 0.025, softness: 0.5, points: [0.6, 0.5, 0.7, 0.5] }],
            inverted: false,
        });

        await answer(0, 1);

        expect(document.querySelector("[data-slot='mask-overlay']")?.getAttribute("data-target")).toBe("removal");
        expect(tint()?.className).toContain("bg-removal");
        expect(tint()?.className).not.toContain("bg-primary");
    });

    it("draws the stroke under the pointer in the removal's colour", () => {
        act(() => {
            store().startEditing(PATH, "removal");
            store().beginStroke(PATH, 0.2, 0.5, false);
            store().extendStroke(0.4, 0.5, 1.5);
        });
        render(<MaskOverlay path={PATH} box={BOX} />);

        const live = document.querySelector("[data-slot='mask-strokes'] g");
        expect(live?.getAttribute("class")).toBe("text-removal");
    });

    it("keeps the limiting mask's colour for the limiting mask", async () => {
        act(() => store().startEditing(PATH, "limit"));
        stroke(0.2);
        render(<MaskOverlay path={PATH} box={BOX} />);
        await answer(0, 1);

        expect(tint()?.className).toContain("bg-primary");
    });
});
