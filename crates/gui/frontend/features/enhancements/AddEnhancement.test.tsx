import { invoke } from "@tauri-apps/api/core";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useShortcuts } from "@/hooks/useShortcuts";
import "@/i18n";
import { forgetCatalogue } from "@/ipc/catalogue";
import { forgetLensInfo } from "@/ipc/lens";
import { track } from "@/lib/faro";
import { useEnhancementStore } from "@/stores/enhancements";
import { useMaskStore } from "@/stores/mask";
import { useOptionsPanelStore } from "@/stores/optionsPanel";
import { useSettingsStore } from "@/stores/settings";
import {
    APPLY_ORDER,
    CATALOGUE,
    FRAMING,
    frame,
    HOLIDAY,
    openFiles,
    render,
    resetCropStore,
    resetFileStore,
} from "@/test/support";
import { AddEnhancement } from "./AddEnhancement";
import { EnhancementList } from "./EnhancementList";

// Mocked at `lib/faro.ts`'s own boundary: what `track` does with an event is pinned by `faro.test.ts`.
vi.mock("@/lib/faro", () => ({
    track: vi.fn(),
    sendError: vi.fn(),
    pauseFaro: vi.fn(),
    // Untraced, as before Faro starts: the request goes out exactly as `invoke` alone would send it.
    traced: (_name: string, send: () => Promise<unknown>) => send(),
}));

// The real `platform()` reads a global the plugin's init script installs, which jsdom has none of - and
// the shortcut hints ask it, to write their keys as this platform does.
vi.mock("@/ipc/os", () => ({ isMacOs: vi.fn(() => false), isWindows: vi.fn(() => false) }));

// The catalogue is an `invoke`, and what the menu adds is built from it: the model, its precision
// and the tier behind it all come from what the library publishes. So is the scale a new upscale starts
// at, which is the library's ladder - answered here as 2x, so the value visibly came from the backend.
vi.mock("@tauri-apps/api/core", () => ({
    // A new lens blur's aperture comes from the photograph's own, which is `opai`'s to work out: here an f/6
    // equivalent, as the bonsai's EXIF gives.
    invoke: vi.fn((command: string) =>
        Promise.resolve(
            command === "suggested_scale"
                ? 2
                : command === "lens_info"
                  ? { ownFNumber: 6, focal35mm: 36, cropFactor: 1.5, origin: "exif" }
                  : CATALOGUE,
        ),
    ),
    convertFileSrc: vi.fn((identity: string) => `opai://localhost/${identity}`),
}));

/** The control, and the menu it opens. */
const button = () => screen.getByRole("button", { name: "Add enhancement" });
const upscale = () => screen.findByRole("menuitem", { name: "Upscale" });

/**
 * Opens the menu.
 *
 * Through the keyboard, as `FileOptionsMenu.test.tsx` opens its own: jsdom dispatches no
 * `pointerdown` from a `click`, and Radix's trigger opens on the pointer rather than on the click.
 */
const open = () => fireEvent.keyDown(button(), { key: "Enter" });

/** The panel of options a new enhancement's row opens by itself. */
const optionsPanel = () => waitFor(() => document.querySelector("[data-slot='enhancement-options']") as HTMLElement);

/** What the current image is set to have done to it. */
const stack = () => useEnhancementStore.getState().enhancements.get(HOLIDAY.path);

describe("adding an enhancement", () => {
    beforeEach(() => {
        localStorage.clear();
        forgetCatalogue();
        forgetLensInfo();
        resetFileStore();
        resetCropStore();
        vi.mocked(invoke).mockClear();
        useEnhancementStore.setState({ autopilot: true, enhancements: new Map() });
        useSettingsStore.setState({ models: {}, autopilotExcluded: [] });
        useOptionsPanelStore.setState(useOptionsPanelStore.getInitialState(), true);
        vi.mocked(track).mockClear();
    });

    it("is unavailable with no image open", () => {
        render(<AddEnhancement />);

        expect(button()).toBeDisabled();
    });

    it("goes live as soon as an image is open", () => {
        openFiles(HOLIDAY);
        render(<AddEnhancement />);

        expect(button()).toBeEnabled();
    });

    it("offers every enhancement this application presents, in pipeline order", async () => {
        openFiles(HOLIDAY);
        render(<AddEnhancement />);

        open();

        // All eight: every family the wire can carry is in `ENHANCEMENTS`, which the typecheck holds.
        expect(await upscale()).toBeInTheDocument();

        // Remove first, then denoise, face recovery, colorization, light adjustment, colour balance and sharpen in
        // that order and all before upscale, then restore and lens blur last, and the menu says so: it draws
        // `ENHANCEMENTS` itself, which is what keeps the menu and the chain in one order.
        const offered = screen.getAllByRole("menuitem");
        expect(offered.map((item) => item.textContent)).toEqual([
            "Remove",
            "Denoise",
            "Face Recovery",
            "Colorization",
            "Light Adjustment",
            "Color Balance",
            "Sharpen",
            "Upscale",
            "Restore",
            "Lens Blur",
        ]);
    });

    it("lists them under headings, which are not entries the arrows land on", async () => {
        openFiles(HOLIDAY);
        render(<AddEnhancement />);

        open();
        await upscale();

        const headings = [...document.querySelectorAll('[data-slot="dropdown-menu-label"]')];
        expect(headings.map((heading) => heading.textContent)).toEqual([
            "Clean up",
            "Light & color",
            "Detail & size",
            "Generative",
            "Effects",
        ]);
        expect(screen.getAllByRole("menuitem")).toHaveLength(10);
    });

    it("adds it at the user's default model, at a scale suited to the photograph", async () => {
        openFiles(HOLIDAY);
        useSettingsStore.setState({ models: { upscale: "kyoto_fp16" } });

        render(<AddEnhancement />);
        open();
        fireEvent.click(await upscale());

        // The scale is the backend's answer for HOLIDAY's own 3000x2000. The model and its precision
        // are the stored selection, split through the catalogue's own codename.
        await waitFor(() =>
            expect(stack()).toEqual([
                { family: "upscale", codename: "kyoto", precision: "fp16", parameters: { scale: 2 } },
            ]),
        );
        expect(invoke).toHaveBeenCalledWith("suggested_scale", { width: 3000, height: 2000 });
        expect(track).toHaveBeenCalledExactlyOnceWith("enhancement_added", { family: "upscale", source: "manual" });
    });

    it("adds a lens blur at a blur amount of 50 on the photograph's own lens", async () => {
        openFiles(HOLIDAY);
        render(
            <>
                <AddEnhancement />
                <EnhancementList />
            </>,
        );

        open();
        fireEvent.click(await screen.findByRole("menuitem", { name: "Lens Blur" }));

        // Halfway in blur between the photograph's own f/6 and f/1.4, which is f/2.27; Lisbon, in the middle of the frame.
        await waitFor(() =>
            expect(stack()).toEqual([
                {
                    family: "lens_blur",
                    codename: "lisbon",
                    precision: "fp16",
                    parameters: { f_number: 2.27, focus_x: 0.5, focus_y: 0.5 },
                },
            ]),
        );
        expect(invoke).toHaveBeenCalledWith("lens_info", { source: HOLIDAY.identity, crop: undefined });
        expect(await screen.findByText("Lisbon, blur 50")).toBeInTheDocument();
        expect(await optionsPanel()).toBeInTheDocument();
        expect(track).toHaveBeenCalledExactlyOnceWith("enhancement_added", { family: "lens_blur", source: "manual" });
    });

    it("asks for a new lens blur's aperture as the photograph is framed", async () => {
        openFiles(HOLIDAY);
        frame(HOLIDAY);

        render(<AddEnhancement />);
        open();
        fireEvent.click(await screen.findByRole("menuitem", { name: "Lens Blur" }));

        await waitFor(() => expect(stack()).toHaveLength(1));
        expect(invoke).toHaveBeenCalledWith("lens_info", { source: HOLIDAY.identity, crop: FRAMING });
    });

    it("adds Remove as Lima and opens its brush at once, asking nothing of the backend", async () => {
        openFiles(HOLIDAY);
        useMaskStore.setState(useMaskStore.getInitialState(), true);
        render(<AddEnhancement />);
        open();

        fireEvent.click(await screen.findByRole("menuitem", { name: "Remove" }));

        await waitFor(() =>
            expect(stack()).toEqual([{ family: "remove", codename: "lima", precision: "fp32", parameters: {} }]),
        );
        expect(useMaskStore.getState().editing).toEqual({ path: HOLIDAY.path, target: "removal" });
        expect(invoke).not.toHaveBeenCalledWith("suggested_scale", expect.anything());
    });

    it("asks for the scale of the photograph as it is framed, not as the file is", async () => {
        openFiles(HOLIDAY);
        frame(HOLIDAY);

        render(<AddEnhancement />);
        open();
        fireEvent.click(await upscale());

        await waitFor(() => expect(stack()).toHaveLength(1));
        expect(invoke).toHaveBeenCalledWith("suggested_scale", { width: FRAMING.width, height: FRAMING.height });
    });

    it("adds a light adjustment at the user's default model and a bias of 50%, with its options open", async () => {
        openFiles(HOLIDAY);
        render(
            <>
                <AddEnhancement />
                <EnhancementList />
            </>,
        );

        open();
        fireEvent.click(await screen.findByRole("menuitem", { name: "Light Adjustment" }));

        await waitFor(() =>
            expect(stack()).toEqual([
                { family: "light_adjustment", codename: "paris", precision: "fp32", parameters: { bias: 0.5 } },
            ]),
        );
        expect(await screen.findByText("Paris, 50%, HD")).toBeInTheDocument();
        expect(await optionsPanel()).toBeInTheDocument();
    });

    it("adds a colour balance at the user's default model and a bias of 50%, with its options open", async () => {
        openFiles(HOLIDAY);
        render(
            <>
                <AddEnhancement />
                <EnhancementList />
            </>,
        );

        open();
        fireEvent.click(await screen.findByRole("menuitem", { name: "Color Balance" }));

        await waitFor(() =>
            expect(stack()).toEqual([
                { family: "color_balance", codename: "rio", precision: "fp32", parameters: { bias: 0.5 } },
            ]),
        );
        expect(await screen.findByText("Rio, 50%, HD")).toBeInTheDocument();
        expect(await optionsPanel()).toBeInTheDocument();
    });

    it("adds a denoise at the user's default model and a strength of 100%, with its options open", async () => {
        openFiles(HOLIDAY);
        render(
            <>
                <AddEnhancement />
                <EnhancementList />
            </>,
        );

        open();
        fireEvent.click(await screen.findByRole("menuitem", { name: "Denoise" }));

        // The model's own output, and Stockholm because the catalogue lists it first.
        await waitFor(() =>
            expect(stack()).toEqual([
                { family: "denoise", codename: "stockholm", precision: "fp32", parameters: { strength: 1 } },
            ]),
        );
        expect(await screen.findByText("Stockholm, 100%, HD")).toBeInTheDocument();
        expect(await optionsPanel()).toBeInTheDocument();
    });

    it("adds a sharpen at the user's default model and a strength of 100%, with its options open", async () => {
        openFiles(HOLIDAY);
        render(
            <>
                <AddEnhancement />
                <EnhancementList />
            </>,
        );

        open();
        fireEvent.click(await screen.findByRole("menuitem", { name: "Sharpen" }));

        // The model's own output, and Moscow because the catalogue lists it first.
        await waitFor(() =>
            expect(stack()).toEqual([
                { family: "sharpen", codename: "moscow", precision: "fp32", parameters: { strength: 1 } },
            ]),
        );
        expect(await screen.findByText("Moscow, 100%, HD")).toBeInTheDocument();
        expect(await optionsPanel()).toBeInTheDocument();
    });

    it("adds a colorization at the user's default model and nothing else, with its options open", async () => {
        openFiles(HOLIDAY);
        render(
            <>
                <AddEnhancement />
                <EnhancementList />
            </>,
        );

        open();
        fireEvent.click(await screen.findByRole("menuitem", { name: "Colorization" }));

        // Colorization takes no parameter, and Delhi because the catalogue lists it first.
        await waitFor(() =>
            expect(stack()).toEqual([{ family: "colorization", codename: "delhi", precision: "fp32", parameters: {} }]),
        );
        expect(await screen.findByText("Delhi, HD")).toBeInTheDocument();
        expect(await optionsPanel()).toBeInTheDocument();
    });

    it("still offers, and adds, an enhancement Autopilot may not suggest", async () => {
        openFiles(HOLIDAY);
        useSettingsStore.setState({ autopilotExcluded: ["face_recovery", "upscale"] });

        render(<AddEnhancement />);
        open();

        // The exclusion governs what Autopilot proposes, not what the user can do: the menu is the
        // same with every family switched off as with none.
        const offered = await screen.findAllByRole("menuitem");
        expect(offered.map((item) => item.textContent)).toEqual([
            "Remove",
            "Denoise",
            "Face Recovery",
            "Colorization",
            "Light Adjustment",
            "Color Balance",
            "Sharpen",
            "Upscale",
            "Restore",
            "Lens Blur",
        ]);
        expect(await upscale()).not.toHaveAttribute("data-disabled");

        fireEvent.click(await upscale());

        await waitFor(() => expect(stack()?.map(({ family }) => family)).toEqual(["upscale"]));
    });

    it("shows an enhancement already in the stack as unavailable", async () => {
        openFiles(HOLIDAY);
        useEnhancementStore
            .getState()
            .addEnhancement(
                HOLIDAY.path,
                { family: "upscale", codename: "kyoto", precision: "fp32", parameters: { scale: 2 } },
                APPLY_ORDER,
            );

        render(<AddEnhancement />);
        open();

        // Unavailable rather than absent: a menu that changed shape as a stack was built up would
        // move the remaining entries under the pointer.
        expect(await upscale()).toHaveAttribute("data-disabled");
    });

    it("leaves the stack alone when the same enhancement is chosen twice", async () => {
        openFiles(HOLIDAY);
        render(<AddEnhancement />);

        open();
        fireEvent.click(await upscale());
        await waitFor(() => expect(stack()).toHaveLength(1));

        open();
        fireEvent.click(await upscale());

        expect(stack()).toHaveLength(1);
    });
});

/** The button with the window's keys listening, as the shell mounts both. */
const WithKeys = () => {
    useShortcuts();

    return <AddEnhancement />;
};

/** Presses a key on the page, bubbling to the window, and answers the dispatched event. */
const press = (key: string) => {
    const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
    act(() => {
        document.body.dispatchEvent(event);
    });

    return event;
};

describe("the Add enhancement key", () => {
    beforeEach(() => {
        localStorage.clear();
        forgetCatalogue();
        resetFileStore();
        useEnhancementStore.setState({ autopilot: true, enhancements: new Map() });
        useSettingsStore.setState({ models: {}, autopilotExcluded: [] });
    });

    it("opens the menu with E, with the focus on its first entry so the arrows and Enter pick one", async () => {
        openFiles(HOLIDAY);
        render(<WithKeys />);

        const event = press("e");

        expect(event.defaultPrevented).toBe(true);
        await waitFor(() => expect(screen.getByRole("menuitem", { name: "Remove" })).toHaveFocus());

        fireEvent.keyDown(screen.getByRole("menuitem", { name: "Remove" }), { key: "ArrowDown" });
        await waitFor(() => expect(screen.getByRole("menuitem", { name: "Denoise" })).toHaveFocus());
    });

    it("opens nothing with no image open, as the button is unavailable, and leaves the key alone", () => {
        render(<WithKeys />);

        const event = press("e");

        expect(screen.queryByRole("menu")).toBeNull();
        expect(event.defaultPrevented).toBe(false);
    });

    it("tells assistive technology the button's key", () => {
        openFiles(HOLIDAY);
        render(<WithKeys />);

        expect(button()).toHaveAttribute("aria-keyshortcuts", "e");
    });

    it("opens the new enhancement's panel with the focus inside it, and one panel at a time", async () => {
        openFiles(HOLIDAY);
        render(
            <>
                <AddEnhancement />
                <EnhancementList />
            </>,
        );

        open();
        fireEvent.click(await screen.findByRole("menuitem", { name: "Denoise" }));
        const first = await optionsPanel();
        await waitFor(() => expect(first).toContainElement(document.activeElement as HTMLElement));

        open();
        fireEvent.click(await screen.findByRole("menuitem", { name: "Sharpen" }));

        await waitFor(() => expect(useOptionsPanelStore.getState().family).toBe("sharpen"));
        await waitFor(() => expect(document.querySelectorAll("[data-slot='enhancement-options']")).toHaveLength(1));
    });

    it("opens no panel for Remove, whose row is the brush", async () => {
        openFiles(HOLIDAY);
        render(
            <>
                <AddEnhancement />
                <EnhancementList />
            </>,
        );

        open();
        fireEvent.click(await screen.findByRole("menuitem", { name: "Remove" }));

        await waitFor(() => expect(stack()).toHaveLength(1));
        expect(useOptionsPanelStore.getState().family).toBeUndefined();
        expect(document.querySelector("[data-slot='enhancement-options']")).toBeNull();
    });

    it("leaves a panel closed for an enhancement added by anything but the menu", () => {
        openFiles(HOLIDAY);
        render(<EnhancementList />);

        act(() =>
            useEnhancementStore
                .getState()
                .addEnhancement(
                    HOLIDAY.path,
                    { family: "denoise", codename: "stockholm", precision: "fp32", parameters: { strength: 1 } },
                    APPLY_ORDER,
                ),
        );

        expect(useOptionsPanelStore.getState().family).toBeUndefined();
        expect(document.querySelector("[data-slot='enhancement-options']")).toBeNull();
    });
});
