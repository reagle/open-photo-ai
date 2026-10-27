import { invoke } from "@tauri-apps/api/core";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "@/i18n";
import { forgetCatalogue } from "@/ipc/catalogue";
import type { Operation } from "@/ipc/enhance";
import { forgetLensInfo, type LensInfo } from "@/ipc/lens";
import { useEnhancementStore } from "@/stores/enhancements";
import { useFocusPickStore } from "@/stores/focusPick";
import { CATALOGUE, FRAMING, frame, HOLIDAY, openFiles, render, resetCropStore, resetFileStore } from "@/test/support";
import { EnhancementList } from "./EnhancementList";

/** What `lens_info` answers, which a case sets before it opens the panel; absent, the question is left unanswered. */
let lens: LensInfo | Error | undefined;

vi.mock("@tauri-apps/api/core", () => ({
    invoke: vi.fn((command: string) => {
        if (command !== "lens_info") return Promise.resolve(CATALOGUE);
        if (lens instanceof Error) return Promise.reject(lens);

        return lens ? Promise.resolve(lens) : new Promise(() => {});
    }),
    convertFileSrc: vi.fn((identity: string) => `opai://localhost/${identity}`),
}));

/** The bonsai's lens: 36 mm at f/6 in full-frame terms, so the slider ends at the f/6.3 stop. */
const BONSAI: LensInfo = { ownFNumber: 6, focal35mm: 36, origin: "exif" };

const lensBlur = (fNumber: number, extra: Record<string, number> = {}): Operation => ({
    family: "lens_blur",
    codename: "lisbon",
    precision: "fp16",
    parameters: { f_number: fNumber, focus_x: 0.5, focus_y: 0.5, ...extra },
});

/** What the current image is set to have done to it. */
const stack = () => useEnhancementStore.getState().enhancements.get(HOLIDAY.path);

/** Puts one lens blur on the current image, draws the list, and opens its options. */
const open = async (operation = lensBlur(6.3)) => {
    useEnhancementStore.setState({ enhancements: new Map([[HOLIDAY.path, [operation]]]) });
    render(<EnhancementList />);

    await screen.findByText(/Lisbon, f\//);

    fireEvent.click(screen.getByRole("button", { name: /Lens Blur/ }));
    await waitFor(() => expect(document.querySelector("[data-slot='enhancement-options']")).not.toBeNull());
};

const slider = () => screen.getByRole("slider", { name: "Aperture" });

/** Releases the slider on the stop at `index`: jsdom lays nothing out, so the track is given a 300px rectangle. */
const release = (clientX: number, stops: number) => {
    const root = document.querySelector("[data-slot='slider']") as HTMLElement;
    vi.spyOn(root, "getBoundingClientRect").mockReturnValue(DOMRect.fromRect({ x: 0, y: 0, width: stops, height: 16 }));

    fireEvent.pointerDown(root, { pointerId: 1, clientX });
    fireEvent.pointerUp(root, { pointerId: 1, clientX });
};

beforeEach(() => {
    localStorage.clear();
    forgetCatalogue();
    forgetLensInfo();
    resetFileStore();
    resetCropStore();
    vi.mocked(invoke).mockClear();
    useEnhancementStore.setState({ autopilot: true, enhancements: new Map() });
    useFocusPickStore.setState(useFocusPickStore.getInitialState(), true);
    lens = BONSAI;
    openFiles(HOLIDAY);
});

describe("a lens blur's row", () => {
    it("reads the model and the f-number the stack carries", async () => {
        useEnhancementStore.setState({ enhancements: new Map([[HOLIDAY.path, [lensBlur(1.4)]]]) });
        render(<EnhancementList />);

        expect(await screen.findByText("Lisbon, f/1.4")).toBeInTheDocument();
    });

    it("reads a whole stop without a decimal", async () => {
        useEnhancementStore.setState({ enhancements: new Map([[HOLIDAY.path, [lensBlur(2)]]]) });
        render(<EnhancementList />);

        expect(await screen.findByText("Lisbon, f/2")).toBeInTheDocument();
    });
});

describe("a lens blur's options", () => {
    it("offer Lisbon, the aperture in full-frame terms, and Set focus", async () => {
        await open();

        expect(screen.getByRole("radio", { name: "Lisbon" })).toBeChecked();
        expect(screen.getByText("Aperture")).toBeInTheDocument();
        expect(screen.getByText("full-frame equivalent")).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Set focus" })).toBeEnabled();
    });

    it("ends the slider at the photograph's own stop, As taken, and starts it at f/1.0", async () => {
        await open();

        // f/1.0 to f/6.3 is 17 stops, so the thumb's index runs 0 to 16, and it stands at the right end.
        await waitFor(() => expect(slider()).toHaveAttribute("aria-valuemax", "16"));
        expect(slider()).toHaveAttribute("aria-valuemin", "0");
        expect(slider()).toHaveAttribute("aria-valuenow", "16");
        // The right end is read out as what it is, since the visible "As taken" under the track is hidden from a screen reader.
        expect(slider()).toHaveAttribute("aria-valuetext", "f/6.3, As taken");
        expect(screen.getByText("As taken")).toBeInTheDocument();
        expect(invoke).toHaveBeenCalledWith("lens_info", { source: HOLIDAY.identity, crop: undefined });
    });

    it("leaves the slider unavailable until the photograph's lens has been answered", async () => {
        lens = undefined;
        await open();

        expect(slider()).toHaveAttribute("data-disabled");
        expect(screen.queryByText("As taken")).not.toBeInTheDocument();
    });

    it("asks for the lens as the photograph is framed, and ends the slider where the crop's own f-number is", async () => {
        lens = { ownFNumber: 12, focal35mm: 72, origin: "exif" };
        frame(HOLIDAY);
        await open(lensBlur(13));

        await waitFor(() => expect(slider()).toHaveAttribute("aria-valuetext", "f/13, As taken"));
        expect(invoke).toHaveBeenCalledWith("lens_info", { source: HOLIDAY.identity, crop: FRAMING });
    });

    it("say so when the photograph's own aperture is assumed, and end at that stop", async () => {
        lens = { ownFNumber: 8, focal35mm: null, origin: "assumed" };
        await open(lensBlur(8));

        expect(await screen.findByText("This photograph has no lens information; f/8 assumed")).toBeInTheDocument();
        expect(slider()).toHaveAttribute("aria-valuetext", "f/8, As taken");
    });

    it("say nothing about an assumption for a lens that was read", async () => {
        await open();

        await waitFor(() => expect(slider()).toHaveAttribute("aria-valuemax", "16"));
        expect(screen.queryByText(/assumed/)).not.toBeInTheDocument();
    });

    it("take a lens that could not be asked about as the assumed f/8", async () => {
        vi.spyOn(console, "error").mockImplementation(() => {});
        lens = new Error("the bridge went away");
        await open(lensBlur(8));

        expect(await screen.findByText("This photograph has no lens information; f/8 assumed")).toBeInTheDocument();
        expect(slider()).toHaveAttribute("aria-valuetext", "f/8, As taken");
    });

    it("change nothing by being opened", async () => {
        await open();
        const before = stack();

        await waitFor(() => expect(slider()).not.toHaveAttribute("data-disabled"));
        fireEvent.click(screen.getByRole("button", { name: "Close" }));

        await waitFor(() => expect(document.querySelector("[data-slot='enhancement-options']")).toBeNull());
        expect(stack()).toBe(before);
    });

    it("write the nominal f-number of the stop the slider is released on, and nothing during the drag", async () => {
        await open();
        await waitFor(() => expect(slider()).not.toHaveAttribute("data-disabled"));

        const writes: unknown[] = [];
        const unsubscribe = useEnhancementStore.subscribe((state) => writes.push(state.enhancements));

        // 16 steps over 160px is 10px a stop: 30px is the fourth stop, index 3, f/1.4.
        release(30, 160);
        unsubscribe();

        expect(writes).toHaveLength(1);
        expect(stack()).toEqual([lensBlur(1.4)]);
        expect(await screen.findByText("Lisbon, f/1.4")).toBeInTheDocument();
    });

    it("write one third of a stop for one key press", async () => {
        await open(lensBlur(2.8));
        await waitFor(() => expect(slider()).not.toHaveAttribute("data-disabled"));

        fireEvent.keyDown(slider(), { key: "ArrowLeft" });

        // f/2.8 is index 9; one stop narrower in index is f/2.5.
        await waitFor(() => expect(stack()).toEqual([lensBlur(2.5)]));
    });

    it("draw an f-number above the photograph's own at As taken, and write nothing for it", async () => {
        await open(lensBlur(20));

        await waitFor(() => expect(slider()).toHaveAttribute("aria-valuenow", "16"));
        expect(stack()).toEqual([lensBlur(20)]);
    });

    it("keep the focus point and the f-number when the focus is set", async () => {
        await open(lensBlur(2.8, { focus_x: 0.3, focus_y: 0.6 }));

        fireEvent.click(screen.getByRole("button", { name: "Set focus" }));

        expect(stack()).toEqual([lensBlur(2.8, { focus_x: 0.3, focus_y: 0.6 })]);
    });
});

describe("Set focus", () => {
    it("closes the panel and arms the preview for this photograph", async () => {
        await open();

        fireEvent.click(screen.getByRole("button", { name: "Set focus" }));

        await waitFor(() => expect(document.querySelector("[data-slot='enhancement-options']")).toBeNull());
        expect(useFocusPickStore.getState()).toMatchObject({ armed: true, imageId: HOLIDAY.identity });
    });

    it("is unavailable for a file with no identity to arm the preview for", async () => {
        useFileStoreWithoutIdentity();
        await open();

        expect(screen.getByRole("button", { name: "Set focus" })).toBeDisabled();
    });
});

/** Replaces the open photograph with one whose bytes could not be read, and so has no identity. */
const useFileStoreWithoutIdentity = () => {
    resetFileStore();
    openFiles({ path: HOLIDAY.path, extension: "png" });
};
