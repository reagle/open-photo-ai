import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "@/i18n";
import { forgetCatalogue } from "@/ipc/catalogue";
import type { Operation } from "@/ipc/enhance";
import { useEnhancementStore } from "@/stores/enhancements";
import { CATALOGUE, HOLIDAY, openFiles, render, resetFileStore } from "@/test/support";
import { AddEnhancement } from "./AddEnhancement";
import { EnhancementList } from "./EnhancementList";

vi.mock("@tauri-apps/api/core", () => ({
    invoke: vi.fn(() => Promise.resolve(CATALOGUE)),
    convertFileSrc: vi.fn((identity: string) => `opai://localhost/${identity}`),
}));

const restore = (scale: number, strength = 1, precision: Operation["precision"] = "fp16"): Operation => ({
    family: "restore",
    codename: "kazan",
    precision,
    parameters: { scale, strength },
});

const upscale: Operation = { family: "upscale", codename: "kyoto", precision: "fp32", parameters: { scale: 4 } };

/** What the current image is set to have done to it. */
const stack = () => useEnhancementStore.getState().enhancements.get(HOLIDAY.path);

const panel = () => document.querySelector<HTMLElement>("[data-slot='enhancement-options']");

/** Puts `operations` on the current image, draws the list, and opens the options of the row named `row`. */
const open = async (operations: Operation[], row: RegExp = /Restore/) => {
    useEnhancementStore.setState({ enhancements: new Map([[HOLIDAY.path, operations]]) });
    render(<EnhancementList />);

    // The rows name their models off the catalogue, which is an `invoke`: waiting for it is waiting for the
    // panel's bounds to have arrived.
    await screen.findAllByText(/Kazan/);

    fireEvent.click(screen.getByRole("button", { name: row }));
    await waitFor(() => expect(panel()).not.toBeNull());
};

beforeEach(() => {
    localStorage.clear();
    forgetCatalogue();
    resetFileStore();
    useEnhancementStore.setState({ autopilot: true, enhancements: new Map() });
    openFiles(HOLIDAY);
});

describe("a restore's options", () => {
    it("offer the model, the three sizes and the strength up to the model's own output", async () => {
        await open([restore(2, 0.75)]);

        expect(screen.getByRole("radio", { name: "Kazan SD" })).toBeChecked();
        expect(screen.getByRole("radio", { name: "Kazan HD" })).toBeInTheDocument();

        const sizes = within(screen.getByRole("radiogroup", { name: "Size" }));
        expect(sizes.getAllByRole("radio").map((size) => size.textContent)).toEqual(["1x", "2x", "4x"]);
        expect(sizes.getByRole("radio", { name: "2x" })).toBeChecked();

        const slider = screen.getByRole("slider", { name: "Strength" });
        expect(slider).toHaveAttribute("aria-valuenow", "75");
        expect(slider).toHaveAttribute("aria-valuemax", "100");
    });

    it("say what a generative model does, without being asked", async () => {
        await open([restore(1)]);

        expect(document.querySelector("[data-slot='restore-generative']")).toHaveTextContent(
            "Generative: redraws detail, and may change faces, textures and lettering.",
        );
    });

    it("write a chosen size, and change nothing by being opened", async () => {
        await open([restore(1)]);
        const before = stack();

        fireEvent.click(screen.getByRole("radio", { name: "4x" }));

        await waitFor(() => expect(stack()).toEqual([restore(4)]));
        expect(before).toEqual([restore(1)]);
    });

    it("report the size and the strength on the row", async () => {
        await open([restore(2, 0.5)]);

        expect(document.querySelector("[data-slot='enhancement-info']")).toHaveTextContent("Kazan, 2x, 50%, SD");
    });
});

describe("an upscale beside a restore", () => {
    it("runs, and says nothing about it, while the restore is at 1x", async () => {
        await open([upscale, restore(1)], /Upscale/);

        expect(document.querySelector("[data-slot='enhancement-suspended']")).toBeNull();
        expect(screen.getByRole("textbox", { name: "Scale" })).toBeEnabled();
    });

    it("says on its row and in its panel that the restore is enlarging, and offers nothing to change", async () => {
        await open([upscale, restore(2)], /Upscale/);

        const reason = "Restore is enlarging this photograph";
        expect(screen.getAllByText(reason)).toHaveLength(2);
        expect(screen.getByRole("textbox", { name: "Scale" })).toBeDisabled();
        expect(within(panel() as HTMLElement).getByRole("radio", { name: "Kyoto HD" })).toBeDisabled();

        // Held out, not removed: the stack still carries it as it was.
        expect(stack()).toEqual([upscale, restore(2)]);
    });

    it("cannot be added while the restore enlarges", async () => {
        useEnhancementStore.setState({ enhancements: new Map([[HOLIDAY.path, [restore(4)]]]) });
        render(<AddEnhancement />);

        fireEvent.pointerDown(screen.getByRole("button", { name: "Add enhancement" }), { button: 0, ctrlKey: false });
        const item = await screen.findByRole("menuitem", { name: "Upscale" });

        expect(item).toHaveAttribute("data-disabled");
    });
});
