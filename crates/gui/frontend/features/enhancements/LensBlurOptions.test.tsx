import { invoke } from "@tauri-apps/api/core";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "@/i18n";
import { forgetCatalogue } from "@/ipc/catalogue";
import type { Operation } from "@/ipc/enhance";
import { forgetLensInfo, type LensInfo } from "@/ipc/lens";
import { fNumberForAmount } from "@/lib/blurAmount";
import { useEnhancementStore } from "@/stores/enhancements";
import { useFocusPickStore } from "@/stores/focusPick";
import { CATALOGUE, FRAMING, frame, HOLIDAY, openFiles, render, resetCropStore, resetFileStore } from "@/test/support";
import { EnhancementList } from "./EnhancementList";

/** What `lens_info` answers, which a case sets before it opens the panel; absent, the question is left unanswered. */
let lens: LensInfo | Error | undefined;

// The real `platform()` reads a global the plugin's init script installs, which jsdom has none of - and
// the shortcut hints ask it, to write their keys as this platform does.
vi.mock("@/ipc/os", () => ({ isMacOs: vi.fn(() => false), isWindows: vi.fn(() => false) }));

vi.mock("@tauri-apps/api/core", () => ({
    invoke: vi.fn((command: string) => {
        if (command !== "lens_info") return Promise.resolve(CATALOGUE);
        if (lens instanceof Error) return Promise.reject(lens);

        return lens ? Promise.resolve(lens) : new Promise(() => {});
    }),
    convertFileSrc: vi.fn((identity: string) => `opai://localhost/${identity}`),
}));

/** The bonsai's lens: 36 mm at f/6 in full-frame terms, from a real f/4 behind a crop factor of 1.5. */
const BONSAI: LensInfo = { ownFNumber: 6, focal35mm: 36, cropFactor: 1.5, origin: "exif" };

const lensBlur = (fNumber: number, extra: Record<string, number> = {}): Operation => ({
    family: "lens_blur",
    codename: "lisbon",
    precision: "fp16",
    parameters: { f_number: fNumber, focus_x: 0.5, focus_y: 0.5, ...extra },
});

/** What the current image is set to have done to it. */
const stack = () => useEnhancementStore.getState().enhancements.get(HOLIDAY.path);

/** Puts one lens blur on the current image, draws the list, and opens its options. */
const open = async (operation = lensBlur(2.27)) => {
    useEnhancementStore.setState({ enhancements: new Map([[HOLIDAY.path, [operation]]]) });
    render(<EnhancementList />);

    await screen.findByText(/^Lisbon, /);

    fireEvent.click(screen.getByRole("button", { name: /Lens Blur/ }));
    await waitFor(() => expect(document.querySelector("[data-slot='enhancement-options']")).not.toBeNull());
};

const slider = () => screen.getByRole("slider", { name: "Blur amount" });

/** The readout beside the slider, which says what lens the amount corresponds to. */
const readout = () => document.querySelector("[data-slot='blur-amount-readout']")?.textContent;

/** Releases the slider at `clientX` of a track `width` pixels wide: jsdom lays nothing out, so the track is given one. */
const release = (clientX: number, width: number) => {
    const root = document.querySelector("[data-slot='slider']") as HTMLElement;
    vi.spyOn(root, "getBoundingClientRect").mockReturnValue(DOMRect.fromRect({ x: 0, y: 0, width, height: 16 }));

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
    it("reads the model and the blur amount the stack carries, against the photograph's own aperture", async () => {
        useEnhancementStore.setState({ enhancements: new Map([[HOLIDAY.path, [lensBlur(1.4)]]]) });
        render(<EnhancementList />);

        expect(await screen.findByText("Lisbon, blur 100")).toBeInTheDocument();
    });

    it("reads a photograph as taken as a blur of 0", async () => {
        useEnhancementStore.setState({ enhancements: new Map([[HOLIDAY.path, [lensBlur(6)]]]) });
        render(<EnhancementList />);

        expect(await screen.findByText("Lisbon, blur 0")).toBeInTheDocument();
    });

    it("reads the model alone until the photograph's lens has been answered, rather than an amount that would change", async () => {
        lens = undefined;
        useEnhancementStore.setState({ enhancements: new Map([[HOLIDAY.path, [lensBlur(2)]]]) });
        render(<EnhancementList />);

        expect(await screen.findByText(/^Lisbon, /)).not.toHaveTextContent("blur");
    });
});

describe("a lens blur's options", () => {
    it("offer Lisbon, the blur amount, the lens it stands for, and Set focus", async () => {
        await open();

        expect(screen.getByRole("radio", { name: "Lisbon" })).toBeChecked();
        expect(screen.getByText("Blur amount")).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Set focus" })).toBeEnabled();
    });

    it("runs the slider from 0, As taken, to 100, and shows the amount the f-number carried is", async () => {
        await open();

        // f/2.27 is halfway in blur between the bonsai's f/6 and f/1.4.
        await waitFor(() => expect(slider()).toHaveAttribute("aria-valuenow", "50"));
        expect(slider()).toHaveAttribute("aria-valuemin", "0");
        expect(slider()).toHaveAttribute("aria-valuemax", "100");
        expect(document.querySelector("[data-slot='blur-amount-value']")).toHaveTextContent("50");
        expect(screen.getByText("As taken")).toBeInTheDocument();
        expect(invoke).toHaveBeenCalledWith("lens_info", { source: HOLIDAY.identity, crop: undefined });
    });

    it("reads the real f-number and the full-frame equivalent beside it", async () => {
        await open();

        // Amount 50 is f/2.27 equivalent: f/1.5 real behind a crop factor of 1.5.
        await waitFor(() => expect(readout()).toBe("≈ f/1.5 (f/2.3 full-frame)"));
        expect(slider()).toHaveAttribute("aria-valuetext", "50, ≈ f/1.5 (f/2.3 full-frame)");
    });

    it("reads the full-frame equivalent alone where the crop is not known", async () => {
        lens = { ownFNumber: 8, focal35mm: null, cropFactor: null, origin: "assumed" };
        await open(lensBlur(8));

        // Amount 0 is the photograph as it is: its own f/8.
        await waitFor(() => expect(readout()).toBe("≈ f/8 full-frame equivalent"));
    });

    it("reads the lens of a photograph as taken at 0, and names it", async () => {
        await open(lensBlur(6));

        await waitFor(() => expect(readout()).toBe("≈ f/4 (f/6 full-frame)"));
        expect(slider()).toHaveAttribute("aria-valuetext", "0, As taken");
    });

    it("leaves the slider unavailable until the photograph's lens has been answered", async () => {
        lens = undefined;
        await open();

        expect(slider()).toHaveAttribute("data-disabled");
        expect(screen.queryByText("As taken")).not.toBeInTheDocument();
    });

    it("asks for the lens as the photograph is framed, and lays the amount out against the crop's own f-number", async () => {
        lens = { ownFNumber: 12, focal35mm: 72, cropFactor: 3, origin: "exif" };
        frame(HOLIDAY);
        await open(lensBlur(2.507));

        // Amount 50 of f/12 is f/2.507 equivalent, and a real f/0.8 behind a crop factor of 3.
        await waitFor(() => expect(slider()).toHaveAttribute("aria-valuenow", "50"));
        expect(readout()).toBe("≈ f/0.8 (f/2.5 full-frame)");
        expect(invoke).toHaveBeenCalledWith("lens_info", { source: HOLIDAY.identity, crop: FRAMING });
    });

    it("disables the slider, with a note, for a photograph already as wide as the blur goes", async () => {
        lens = { ownFNumber: 1.2, focal35mm: 35, cropFactor: 1, origin: "exif" };
        await open(lensBlur(1.2));

        expect(await screen.findByText(/cannot be blurred more/)).toBeInTheDocument();
        expect(slider()).toHaveAttribute("data-disabled");
        expect(slider()).toHaveAttribute("aria-valuenow", "0");
    });

    it("says nothing of it for a photograph that can be blurred", async () => {
        await open();

        await waitFor(() => expect(slider()).not.toHaveAttribute("data-disabled"));
        expect(screen.queryByText(/cannot be blurred more/)).not.toBeInTheDocument();
    });

    it("say so when the photograph's own aperture is assumed, and lay the amount out against that f/8", async () => {
        lens = { ownFNumber: 8, focal35mm: null, cropFactor: null, origin: "assumed" };
        await open(lensBlur(8));

        expect(await screen.findByText("This photograph has no lens information; f/8 assumed")).toBeInTheDocument();
        expect(slider()).toHaveAttribute("aria-valuenow", "0");
    });

    it("say nothing about an assumption for a lens that was read", async () => {
        await open();

        await waitFor(() => expect(slider()).not.toHaveAttribute("data-disabled"));
        expect(screen.queryByText(/assumed/)).not.toBeInTheDocument();
    });

    it("take a lens that could not be asked about as the assumed f/8", async () => {
        vi.spyOn(console, "error").mockImplementation(() => {});
        lens = new Error("the bridge went away");
        await open(lensBlur(8));

        expect(await screen.findByText("This photograph has no lens information; f/8 assumed")).toBeInTheDocument();
        expect(slider()).toHaveAttribute("aria-valuenow", "0");
    });

    it("change nothing by being opened", async () => {
        await open();
        const before = stack();

        await waitFor(() => expect(slider()).not.toHaveAttribute("data-disabled"));
        fireEvent.click(screen.getByRole("button", { name: "Close" }));

        await waitFor(() => expect(document.querySelector("[data-slot='enhancement-options']")).toBeNull());
        expect(stack()).toBe(before);
    });

    it("write the f-number of the amount the slider is released on, and nothing during the drag", async () => {
        await open();
        await waitFor(() => expect(slider()).not.toHaveAttribute("data-disabled"));

        const writes: unknown[] = [];
        const unsubscribe = useEnhancementStore.subscribe((state) => writes.push(state.enhancements));

        // 100 steps over 100px is a pixel an amount: amount 70 is f/1.818 of the bonsai's f/6.
        release(70, 100);
        unsubscribe();

        expect(writes).toHaveLength(1);
        expect(stack()).toEqual([lensBlur(1.818)]);
        expect(await screen.findByText("Lisbon, blur 70")).toBeInTheDocument();
    });

    it("write nothing for a release on the amount it is already at", async () => {
        await open();
        await waitFor(() => expect(slider()).toHaveAttribute("aria-valuenow", "50"));
        const before = stack();

        release(50, 100);

        expect(stack()).toBe(before);
    });

    it("write the photograph's own f-number for an amount of 0, which adds nothing", async () => {
        await open();
        await waitFor(() => expect(slider()).not.toHaveAttribute("data-disabled"));

        release(0, 100);

        await waitFor(() => expect(stack()).toEqual([lensBlur(6)]));
    });

    it("write one amount for one key press", async () => {
        await open();
        await waitFor(() => expect(slider()).toHaveAttribute("aria-valuenow", "50"));

        fireEvent.keyDown(slider(), { key: "ArrowRight" });

        // Amount 51 of f/6 is f/2.2...: the one step wider than 50.
        await waitFor(() => expect(stack()).toEqual([lensBlur(fNumberForAmount(51, 6))]));
        expect(fNumberForAmount(51, 6)).toBeLessThan(2.27);
    });

    it("draw an f-number above the photograph's own at 0, and write nothing for it", async () => {
        await open(lensBlur(20));

        await waitFor(() => expect(slider()).toHaveAttribute("aria-valuenow", "0"));
        expect(stack()).toEqual([lensBlur(20)]);
    });

    it("keep the focus point and the f-number when the focus is set", async () => {
        await open(lensBlur(2.27, { focus_x: 0.3, focus_y: 0.6 }));

        fireEvent.click(screen.getByRole("button", { name: "Set focus" }));

        expect(stack()).toEqual([lensBlur(2.27, { focus_x: 0.3, focus_y: 0.6 })]);
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
