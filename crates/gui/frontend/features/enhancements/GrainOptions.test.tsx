import { fireEvent, screen, waitFor } from "@testing-library/react";
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

const grain = (amount: number, match = 1): Operation => ({
    family: "grain",
    codename: "sensor",
    precision: "fp32",
    parameters: { amount, match },
});

/** What the current image is set to have done to it. */
const stack = () => useEnhancementStore.getState().enhancements.get(HOLIDAY.path);

const panel = () => document.querySelector<HTMLElement>("[data-slot='enhancement-options']");

/** Puts `operations` on the current image, draws the list, and opens the grain's options. */
const open = async (operations: Operation[]) => {
    useEnhancementStore.setState({ enhancements: new Map([[HOLIDAY.path, operations]]) });
    render(<EnhancementList />);

    await screen.findAllByText(/Sensor/);

    fireEvent.click(screen.getByRole("button", { name: /Grain/ }));
    await waitFor(() => expect(panel()).not.toBeNull());
};

beforeEach(() => {
    localStorage.clear();
    forgetCatalogue();
    resetFileStore();
    useEnhancementStore.setState({ autopilot: true, enhancements: new Map() });
    openFiles(HOLIDAY);
});

describe("a grain's options", () => {
    it("offer the amount and the switch, matching the source to start, and say what that does", async () => {
        await open([grain(0.6)]);

        const slider = screen.getByRole("slider", { name: "Amount" });
        expect(slider).toHaveAttribute("aria-valuenow", "60");
        expect(slider).toHaveAttribute("aria-valuemax", "100");
        expect(screen.getByRole("switch", { name: "Match source" })).toBeChecked();
        expect(document.querySelector("[data-slot='grain-hint']")).toHaveTextContent(
            "Puts back the photograph's own noise where the enhancements smoothed it away",
        );
    });

    it("write the mode as the switch the library reads, keeping the amount, and change nothing by being opened", async () => {
        await open([grain(0.6)]);
        const before = stack();

        fireEvent.click(screen.getByRole("switch", { name: "Match source" }));

        await waitFor(() => expect(stack()).toEqual([grain(0.6, 0)]));
        expect(before).toEqual([grain(0.6)]);
        expect(document.querySelector("[data-slot='grain-hint']")).toHaveTextContent(
            "Adds a camera sensor's grain everywhere",
        );
    });

    it("report the amount and what it is a share of on the row", async () => {
        await open([grain(0.5)]);
        expect(document.querySelector("[data-slot='enhancement-info']")).toHaveTextContent("Sensor, 50%, matched");
    });

    it("report an added grain as added", async () => {
        await open([grain(0.25, 0)]);
        expect(document.querySelector("[data-slot='enhancement-info']")).toHaveTextContent("Sensor, 25%, added");
    });
});

describe("adding a grain", () => {
    it("starts it at all of the photograph's own grain, matched", async () => {
        render(<AddEnhancement />);

        fireEvent.pointerDown(screen.getByRole("button", { name: "Add enhancement" }), { button: 0, ctrlKey: false });
        fireEvent.click(await screen.findByRole("menuitem", { name: "Grain" }));

        await waitFor(() => expect(stack()).toEqual([grain(1, 1)]));
    });
});
