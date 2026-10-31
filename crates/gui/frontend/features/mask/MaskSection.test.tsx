import { act, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "@/i18n";
import type { Operation } from "@/ipc/enhance";
import { useEnhancementStore } from "@/stores/enhancements";
import { useFileStore } from "@/stores/files";
import { useMaskStore } from "@/stores/mask";
import { useSettingsStore } from "@/stores/settings";
import { HOLIDAY, openFiles, render, resetFileStore, SUNSET } from "@/test/support";
import { MaskSection } from "./MaskSection";

// The comparison hint names the key as this platform writes it.
vi.mock("@/ipc/os", () => ({ isMacOs: vi.fn(() => false), isWindows: vi.fn(() => false) }));

const sharpen: Operation = { family: "sharpen", codename: "moscow", precision: "fp32", parameters: { strength: 1 } };

const store = () => useMaskStore.getState();

/** One committed stroke on `path`. */
const paint = (path = HOLIDAY.path) =>
    act(() => {
        store().beginStroke(path, 0.2, 0.2, false);
        store().endStroke(0.4, 0.2);
    });

const stack = (...paths: string[]) =>
    act(() => useEnhancementStore.setState({ enhancements: new Map(paths.map((path) => [path, [sharpen]])) }));

beforeEach(() => {
    resetFileStore();
    useMaskStore.setState(useMaskStore.getInitialState(), true);
    useEnhancementStore.setState({ autopilot: true, enhancements: new Map() });
    openFiles(HOLIDAY, SUNSET);
});

describe("the mask section", () => {
    it("is not there for a photograph with nothing in its stack", () => {
        render(<MaskSection />);

        expect(document.querySelector("[data-slot='mask-section']")).toBeNull();
    });

    it("is not there with no photograph open", () => {
        resetFileStore();
        render(<MaskSection />);

        expect(document.querySelector("[data-slot='mask-section']")).toBeNull();
    });

    it("says there is no mask and offers to paint one", () => {
        stack(HOLIDAY.path);
        render(<MaskSection />);

        expect(screen.getByText("No mask")).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Paint where to enhance" })).toBeInTheDocument();
        expect(screen.queryByRole("button", { name: "Edit" })).toBeNull();
    });

    it("reads Mask with Edit, Invert and Clear once something is painted", () => {
        stack(HOLIDAY.path);
        paint();
        render(<MaskSection />);

        expect(screen.getByText("Mask")).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Edit" })).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Invert" })).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Clear" })).toBeInTheDocument();
        expect(screen.queryByRole("button", { name: "Paint where to enhance" })).toBeNull();
    });

    it("opens the panel in its place, and Done goes back", () => {
        stack(HOLIDAY.path);
        render(<MaskSection />);

        fireEvent.click(screen.getByRole("button", { name: "Paint where to enhance" }));

        expect(store().editing).toEqual({ path: HOLIDAY.path, target: "limit" });
        expect(document.querySelector("[data-slot='mask-panel']")).not.toBeNull();
        expect(screen.queryByRole("button", { name: "Paint where to enhance" })).toBeNull();

        fireEvent.click(screen.getByRole("button", { name: "Done" }));

        expect(store().editing).toBeUndefined();
        expect(document.querySelector("[data-slot='mask-panel']")).toBeNull();
        expect(screen.getByRole("button", { name: "Paint where to enhance" })).toBeInTheDocument();
    });

    it("turns the weight over and clears it from the section, each a step Edit can undo", () => {
        stack(HOLIDAY.path);
        paint();
        render(<MaskSection />);

        fireEvent.click(screen.getByRole("button", { name: "Invert" }));
        expect(store().masks.get(HOLIDAY.path)?.inverted).toBe(true);

        fireEvent.click(screen.getByRole("button", { name: "Clear" }));
        expect(store().masks.get(HOLIDAY.path)?.strokes).toEqual([]);
        expect(screen.getByText("No mask")).toBeInTheDocument();

        fireEvent.click(screen.getByRole("button", { name: "Paint where to enhance" }));
        fireEvent.click(screen.getByRole("button", { name: "Undo" }));

        expect(store().masks.get(HOLIDAY.path)?.strokes).toHaveLength(1);
    });

    it("stops painting when the stack empties, and keeps the strokes for when it is not", () => {
        stack(HOLIDAY.path);
        paint();
        store().startEditing(HOLIDAY.path);
        render(<MaskSection />);

        stack();

        expect(store().editing).toBeUndefined();
        expect(document.querySelector("[data-slot='mask-section']")).toBeNull();

        stack(HOLIDAY.path);
        expect(screen.getByText("Mask")).toBeInTheDocument();
    });

    it("stops painting when another photograph becomes the current one", () => {
        stack(HOLIDAY.path, SUNSET.path);
        store().startEditing(HOLIDAY.path);
        render(<MaskSection />);

        act(() => useFileStore.getState().setCurrentIndex(1));

        expect(store().editing).toBeUndefined();
        expect(screen.getByRole("button", { name: "Paint where to enhance" })).toBeInTheDocument();
    });
});

describe("the panel", () => {
    beforeEach(() => {
        stack(HOLIDAY.path);
        store().startEditing(HOLIDAY.path);
    });

    it("chooses paint or erase, and keeps one of them chosen", () => {
        render(<MaskSection />);

        fireEvent.click(screen.getByRole("radio", { name: /Erase/ }));
        expect(store().tool.mode).toBe("erase");

        // Pressing the one that is on would turn it off: the brush is always one or the other.
        fireEvent.click(screen.getByRole("radio", { name: /Erase/ }));
        expect(store().tool.mode).toBe("erase");

        fireEvent.click(screen.getByRole("radio", { name: /Paint/ }));
        expect(store().tool.mode).toBe("paint");
    });

    it("moves the brush's size and softness with its two sliders", () => {
        render(<MaskSection />);
        const size = store().tool.size;

        fireEvent.keyDown(screen.getByRole("slider", { name: "Size" }), { key: "ArrowRight" });
        expect(store().tool.size).toBeGreaterThan(size);

        fireEvent.keyDown(screen.getByRole("slider", { name: "Softness" }), { key: "ArrowRight" });
        expect(store().tool.softness).toBeCloseTo(0.55);
    });

    it("shows the tint switch on, and turns it off", () => {
        render(<MaskSection />);

        const overlay = screen.getByRole("switch", { name: "Show overlay" });
        expect(overlay).toBeChecked();

        fireEvent.click(overlay);
        expect(store().tool.overlay).toBe(false);
    });

    it("has Undo, Redo, Invert and Clear unavailable while there is nothing to act on", () => {
        render(<MaskSection />);

        for (const name of ["Undo", "Redo", "Invert", "Clear"]) {
            expect(screen.getByRole("button", { name })).toBeDisabled();
        }
    });

    it("enables them as a stroke is painted, and steps with Undo and Redo", () => {
        render(<MaskSection />);
        paint();

        expect(screen.getByRole("button", { name: "Undo" })).toBeEnabled();
        expect(screen.getByRole("button", { name: "Invert" })).toBeEnabled();
        expect(screen.getByRole("button", { name: "Redo" })).toBeDisabled();

        fireEvent.click(screen.getByRole("button", { name: "Undo" }));
        expect(store().masks.get(HOLIDAY.path)?.strokes).toEqual([]);
        expect(screen.getByRole("button", { name: "Redo" })).toBeEnabled();

        fireEvent.click(screen.getByRole("button", { name: "Redo" }));
        expect(store().masks.get(HOLIDAY.path)?.strokes).toHaveLength(1);
    });

    it("inverts and clears", () => {
        render(<MaskSection />);
        paint();

        fireEvent.click(screen.getByRole("button", { name: "Invert" }));
        expect(store().masks.get(HOLIDAY.path)?.inverted).toBe(true);

        fireEvent.click(screen.getByRole("button", { name: "Clear" }));
        expect(store().masks.get(HOLIDAY.path)?.strokes).toEqual([]);
    });

    it("says which way round the mask is, how to paint and how to move the photograph", () => {
        render(<MaskSection />);

        expect(screen.getByText("Enhance only where painted")).toBeInTheDocument();
        expect(screen.getByRole("group", { name: "Enhance only where painted" })).toBeInTheDocument();
        expect(
            screen.getByText(
                "Painted areas get the enhancements; the rest stays as the original. Drag to paint, hold Space to move the photograph.",
            ),
        ).toBeInTheDocument();
    });

    it("says beside the tint's switch what the tint stands for, without renaming the switch", () => {
        render(<MaskSection />);

        expect(document.querySelector("[data-slot='mask-legend']")?.textContent).toBe("= enhanced");
        expect(screen.getByRole("switch", { name: "Show overlay" })).toBeInTheDocument();
    });

    it("lets go of the focus a mouse click gave a button, so Space pans instead of pressing it again", () => {
        render(<MaskSection />);
        paint();
        paint();

        const undo = screen.getByRole("button", { name: "Undo" });
        undo.focus();
        // A click with a pointer behind it has a detail of one; a keyboard's has none.
        fireEvent.click(undo, { detail: 1 });
        expect(document.activeElement).not.toBe(undo);

        const invert = screen.getByRole("button", { name: "Invert" });
        invert.focus();
        expect(document.activeElement).toBe(invert);
        fireEvent.click(invert, { detail: 0 });
        expect(document.activeElement).toBe(invert);
    });
});

describe("the brush for Remove", () => {
    const remove: Operation = { family: "remove", codename: "lima", precision: "fp32", parameters: {} };

    const holding = (...operations: Operation[]) =>
        act(() => useEnhancementStore.setState({ enhancements: new Map([[HOLIDAY.path, operations]]) }));

    it("stands in for the section while the area to remove is painted, titled for it and with no Invert", () => {
        holding(remove, sharpen);
        act(() => store().startEditing(HOLIDAY.path, "removal"));
        render(<MaskSection />);

        expect(screen.getByText("Paint area to remove")).toBeInTheDocument();
        expect(document.querySelector("[data-slot='mask-panel']")?.getAttribute("data-target")).toBe("removal");
        expect(screen.queryByRole("button", { name: "Invert" })).toBeNull();
        expect(screen.getByRole("button", { name: "Clear" })).toBeInTheDocument();
        expect(screen.queryByText("No mask")).toBeNull();
    });

    it("undoes and clears the removal's strokes and not the mask's", () => {
        holding(remove, sharpen);
        paint();
        act(() => store().startEditing(HOLIDAY.path, "removal"));
        paint();
        paint();
        render(<MaskSection />);

        fireEvent.click(screen.getByRole("button", { name: "Undo" }));
        expect(store().removals.get(HOLIDAY.path)?.strokes).toHaveLength(1);

        fireEvent.click(screen.getByRole("button", { name: "Clear" }));
        expect(store().removals.get(HOLIDAY.path)?.strokes).toEqual([]);
        expect(store().masks.get(HOLIDAY.path)?.strokes).toHaveLength(1);
    });

    it("gives the mask's buttons back when Done is pressed", () => {
        holding(remove, sharpen);
        act(() => store().startEditing(HOLIDAY.path, "removal"));
        render(<MaskSection />);

        fireEvent.click(screen.getByRole("button", { name: "Done" }));

        expect(store().editing).toBeUndefined();
        expect(screen.getByRole("button", { name: "Paint where to enhance" })).toBeInTheDocument();
    });

    it("offers no limiting mask for a stack that holds only Remove, which it would have nothing to limit", () => {
        holding(remove);
        render(<MaskSection />);

        expect(document.querySelector("[data-slot='mask-section']")).toBeNull();
    });

    it("stops when Remove leaves the stack", () => {
        holding(remove, sharpen);
        act(() => store().startEditing(HOLIDAY.path, "removal"));
        render(<MaskSection />);

        holding(sharpen);

        expect(store().editing).toBeUndefined();
    });
});

describe("the uses of a mask under a lens blur", () => {
    const blur: Operation = {
        family: "lens_blur",
        codename: "lisbon",
        precision: "fp16",
        parameters: { f_number: 1.4, focus_x: 0.5, focus_y: 0.5 },
    };

    const holding = (...operations: Operation[]) =>
        act(() => useEnhancementStore.setState({ enhancements: new Map([[HOLIDAY.path, operations]]) }));

    const limits = () => screen.queryByRole("switch", { name: "Limit the enhancements" });
    const keepsSharp = () => screen.queryByRole("switch", { name: "Keep the mask in focus" });

    beforeEach(() => store().startEditing(HOLIDAY.path));

    it("are not offered without a lens blur", () => {
        holding(sharpen);
        render(<MaskSection />);

        expect(limits()).toBeNull();
        expect(keepsSharp()).toBeNull();
    });

    it("are both offered, both on, with a lens blur and another enhancement", () => {
        holding(sharpen, blur);
        render(<MaskSection />);

        expect(limits()).toHaveAttribute("aria-checked", "true");
        expect(keepsSharp()).toHaveAttribute("aria-checked", "true");
    });

    it("switch the mask's use as a step Undo takes back, and Clear keeps it", () => {
        holding(sharpen, blur);
        paint();
        render(<MaskSection />);

        fireEvent.click(limits() as HTMLElement);
        expect(store().masks.get(HOLIDAY.path)?.limitsEnhancements).toBe(false);
        expect(limits()).toHaveAttribute("aria-checked", "false");

        fireEvent.click(screen.getByRole("button", { name: "Clear" }));
        expect(store().masks.get(HOLIDAY.path)?.limitsEnhancements).toBe(false);

        fireEvent.click(screen.getByRole("button", { name: "Undo" }));
        fireEvent.click(screen.getByRole("button", { name: "Undo" }));
        expect(store().masks.get(HOLIDAY.path)?.limitsEnhancements).toBeUndefined();
        expect(limits()).toHaveAttribute("aria-checked", "true");
        expect(store().masks.get(HOLIDAY.path)?.strokes).toHaveLength(1);
    });
});

describe("what the panel says the mask does not do", () => {
    const remove: Operation = { family: "remove", codename: "lima", precision: "fp32", parameters: {} };
    const blur: Operation = {
        family: "lens_blur",
        codename: "lisbon",
        precision: "fp16",
        parameters: { f_number: 1.4, focus_x: 0.5, focus_y: 0.5 },
    };
    const grain: Operation = { family: "grain", codename: "sensor", precision: "fp32", parameters: { amount: 1 } };
    const upscale = (scale: number): Operation => ({
        family: "upscale",
        codename: "tokyo",
        precision: "fp32",
        parameters: { scale },
    });
    const restore = (scale: number): Operation => ({
        family: "restore",
        codename: "kyoto",
        precision: "fp16",
        parameters: { scale },
    });

    const holding = (...operations: Operation[]) =>
        act(() => useEnhancementStore.setState({ enhancements: new Map([[HOLIDAY.path, operations]]) }));
    const line = (slot: string) => document.querySelector(`[data-slot='${slot}']`)?.textContent;

    beforeEach(() => store().startEditing(HOLIDAY.path));
    afterEach(() => useSettingsStore.getState().apply({ shortcuts: {} }));

    it("says outside the mask an enlarged photograph has no AI detail, only where the stack enlarges", () => {
        const enlarged =
            "Outside the mask the photograph is enlarged without AI detail; use Softness to blend the edge.";
        for (const [stack, said] of [
            [[sharpen, upscale(2)], true],
            [[sharpen, restore(2)], true],
            [[sharpen, upscale(1)], false],
            [[sharpen, restore(1)], false],
            [[sharpen], false],
        ] as const) {
            holding(...stack);
            const { unmount } = render(<MaskSection />);

            expect(line("mask-enlarged")).toBe(said ? enlarged : undefined);
            unmount();
        }
    });

    it("names the enhancements in the stack the mask does not limit, and only those", () => {
        for (const [stack, said] of [
            [[remove, sharpen, blur, grain], "Not limited by the mask: Remove, Lens Blur, Grain."],
            [[sharpen, grain], "Not limited by the mask: Grain."],
            [[remove, sharpen], "Not limited by the mask: Remove."],
            [[sharpen], undefined],
        ] as const) {
            holding(...stack);
            const { unmount } = render(<MaskSection />);

            expect(line("mask-unlimited")).toBe(said);
            unmount();
        }
    });

    it("names the key that compares with the original, as the user has it bound", () => {
        holding(sharpen);
        const { unmount } = render(<MaskSection />);

        expect(line("mask-compare")).toBe("Hold \\ to compare with the original; only the painted area changes.");
        unmount();

        act(() => useSettingsStore.getState().apply({ shortcuts: { showOriginal: ["Shift+o"] } }));
        const rebound = render(<MaskSection />);
        expect(line("mask-compare")).toMatch(
            /^Hold .*O to compare with the original; only the painted area changes\.$/,
        );
        expect(line("mask-compare")).not.toContain("\\");
        rebound.unmount();

        act(() => useSettingsStore.getState().apply({ shortcuts: { showOriginal: [] } }));
        render(<MaskSection />);
        expect(line("mask-compare")).toBeUndefined();
    });
});

describe("a stack with nothing for the mask to limit", () => {
    const remove: Operation = { family: "remove", codename: "lima", precision: "fp32", parameters: {} };
    const blur: Operation = {
        family: "lens_blur",
        codename: "lisbon",
        precision: "fp16",
        parameters: { f_number: 1.4, focus_x: 0.5, focus_y: 0.5 },
    };
    const grain: Operation = { family: "grain", codename: "sensor", precision: "fp32", parameters: { amount: 1 } };

    const holding = (...operations: Operation[]) =>
        act(() => useEnhancementStore.setState({ enhancements: new Map([[HOLIDAY.path, operations]]) }));

    it("offers no mask for Grain, alone or after Remove, which the mask never limits", () => {
        for (const stack of [[grain], [remove, grain]]) {
            holding(...stack);
            const { unmount } = render(<MaskSection />);

            expect(document.querySelector("[data-slot='mask-section']")).toBeNull();
            unmount();
        }
    });

    it("stops painting the limiting mask when only Grain is left", () => {
        holding(sharpen, grain);
        act(() => store().startEditing(HOLIDAY.path));
        render(<MaskSection />);

        holding(grain);

        expect(store().editing).toBeUndefined();
    });

    it("offers no mask under a lens blur with nothing to limit, only a line pointing to Refine", () => {
        for (const stack of [[blur], [blur, grain], [remove, blur, grain]]) {
            holding(...stack);
            const { unmount } = render(<MaskSection />);

            expect(screen.getByText("To keep areas sharp, use Refine in the Lens Blur panel.")).toBeInTheDocument();
            expect(screen.queryByRole("button", { name: /^Paint/ })).toBeNull();
            expect(screen.queryByRole("button", { name: "Edit" })).toBeNull();
            expect(screen.queryByRole("button", { name: "Clear" })).toBeNull();
            unmount();
        }
    });

    it("says nothing of Refine where there is no lens blur, and keeps the strokes painted meanwhile", () => {
        paint();
        holding(grain);
        render(<MaskSection />);

        expect(screen.queryByText(/use Refine/)).toBeNull();
        expect(store().masks.get(HOLIDAY.path)?.strokes).toHaveLength(1);
    });

    it("stops painting the limiting mask when only a lens blur is left", () => {
        holding(sharpen, blur);
        act(() => store().startEditing(HOLIDAY.path));
        render(<MaskSection />);

        holding(blur);

        expect(store().editing).toBeUndefined();
    });

    it("says the same once the limit is switched off under a lens blur", () => {
        holding(sharpen, blur);
        act(() => store().startEditing(HOLIDAY.path));
        render(<MaskSection />);

        expect(screen.getByText("Enhance only where painted")).toBeInTheDocument();
        fireEvent.click(screen.getByRole("switch", { name: "Limit the enhancements" }));

        expect(screen.getByText("Keep the painted area in focus")).toBeInTheDocument();
        expect(document.querySelector("[data-slot='mask-legend']")?.textContent).toBe("= in focus");
    });
});
