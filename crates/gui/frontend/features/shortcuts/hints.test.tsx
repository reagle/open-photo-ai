import { act, fireEvent, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { EnhancementList } from "@/features/enhancements/EnhancementList";
import { Navbar } from "@/features/navbar/Navbar";
import { Sidebar } from "@/features/sidebar/Sidebar";
import "@/i18n";
import { isMacOs } from "@/ipc/os";
import { useAutopilotStore } from "@/stores/autopilot";
import { useEnhancementStore } from "@/stores/enhancements";
import { useFileStore } from "@/stores/files";
import { useSettingsStore } from "@/stores/settings";
import { useTransformStore } from "@/stores/transform";
import { CATALOGUE, EXPORT_FORMATS, HOLIDAY, openFiles, render, resetCropStore, resetFileStore } from "@/test/support";

// The keyboard hints on the sidebar's and the navbar's controls: each names its key in a tooltip on hover and
// focus, as this platform writes it and as the user has bound it, and says nothing for an action with no key.
// The drawer's and the file menu's are in their own tests, beside the controls.
vi.mock("@/ipc/os", () => ({ isMacOs: vi.fn(() => false), isWindows: vi.fn(() => false) }));
vi.mock("@/ipc/app", () => ({
    appVersion: vi.fn(() => Promise.resolve("26.9.0")),
    isOutdated: vi.fn(() => Promise.resolve(false)),
}));
vi.mock("@tauri-apps/api/core", () => ({
    convertFileSrc: vi.fn((identity: string) => `opai://localhost/${identity}`),
    invoke: vi.fn((command: string) =>
        Promise.resolve(command === "suggest" ? [] : command === "export_formats" ? EXPORT_FORMATS : CATALOGUE),
    ),
}));

/** Focuses a control and answers the tooltip it opens. */
const tooltipOf = async (control: HTMLElement) => {
    await act(async () => {
        fireEvent.focus(control);
    });

    return screen.findByRole("tooltip");
};

/** Two enhancements on the current image, so the rows have places 1 and 2. */
const stack = () => {
    openFiles(HOLIDAY);
    useEnhancementStore.setState({
        autopilot: false,
        enhancements: new Map([
            [
                HOLIDAY.path,
                [
                    { family: "denoise", codename: "tokyo", precision: "fp32", parameters: { strength: 0.5 } },
                    { family: "upscale", codename: "kyoto", precision: "fp32", parameters: { scale: 2 } },
                ],
            ],
        ]),
    });
};

beforeEach(() => {
    vi.mocked(isMacOs).mockReturnValue(false);
    localStorage.clear();
    resetFileStore();
    resetCropStore();
    useTransformStore.setState(useTransformStore.getInitialState(), true);
    useSettingsStore.setState(useSettingsStore.getInitialState(), true);
    useEnhancementStore.setState({ autopilot: true, enhancements: new Map() });
    useAutopilotStore.setState(useAutopilotStore.getInitialState(), true);
});

describe("the sidebar's hints", () => {
    it("names Autopilot's key on the switch's label, and to assistive technology", async () => {
        render(<Sidebar />);
        const toggle = screen.getByRole("switch");

        expect(await tooltipOf(toggle)).toHaveTextContent("Autopilot (A)");
        expect(toggle).toHaveAttribute("aria-keyshortcuts", "a");
    });

    it("names Add enhancement's key", async () => {
        openFiles(HOLIDAY);
        render(<Sidebar />);

        expect(await tooltipOf(screen.getByRole("button", { name: "Add enhancement" }))).toHaveTextContent(
            "Add enhancement (E)",
        );
    });

    it.each([
        [true, "Export image (⌘ S)", "Meta+s Meta+Shift+e"],
        [false, "Export image (Ctrl+S)", "Control+s Control+Shift+e"],
    ])("names Export's fixed key on this platform (mac: %s)", async (mac, tip, aria) => {
        vi.mocked(isMacOs).mockReturnValue(mac);
        openFiles(HOLIDAY);
        useFileStore.getState().selectAll();
        render(<Sidebar />);
        const button = screen.getByRole("button", { name: "Export image" });

        expect(await tooltipOf(button)).toHaveTextContent(tip);
        expect(button).toHaveAttribute("aria-keyshortcuts", aria);
    });

    it("follows a rebinding, and says nothing of a key once the action is unbound", async () => {
        useSettingsStore.getState().apply({ shortcuts: { toggleAutopilot: ["Shift+p"], addEnhancement: [] } });
        openFiles(HOLIDAY);
        render(<Sidebar />);

        expect(await tooltipOf(screen.getByRole("switch"))).toHaveTextContent("Autopilot (Shift+P)");

        const add = screen.getByRole("button", { name: "Add enhancement" });
        expect(await tooltipOf(add)).toHaveTextContent(/^Add enhancement$/);
        expect(add).not.toHaveAttribute("aria-keyshortcuts");
    });
});

describe("the enhancement rows' hints", () => {
    it("names each row's place as the key that opens its options", async () => {
        stack();
        render(<EnhancementList />);
        const rows = document.querySelectorAll<HTMLElement>("[data-slot='enhancement-trigger']");

        expect(rows).toHaveLength(2);
        expect(await tooltipOf(rows[0] as HTMLElement)).toHaveTextContent("Denoise (1)");
        expect(rows[0]).toHaveAttribute("aria-keyshortcuts", "1");
        expect(rows[1]).toHaveAttribute("aria-keyshortcuts", "2");
    });

    it("keeps the open row painted, which the tooltip's own state attribute must not take away", async () => {
        stack();
        render(<EnhancementList />);
        const row = document.querySelector<HTMLElement>("[data-slot='enhancement-trigger']") as HTMLElement;

        fireEvent.click(row);

        expect(row).toHaveAttribute("aria-expanded", "true");
    });

    it.each([
        [false, "Remove enhancement (Delete)"],
        [true, "Remove enhancement (⌫)"],
    ])("names the remove button's key as this platform writes it (mac: %s)", async (mac, tip) => {
        vi.mocked(isMacOs).mockReturnValue(mac);
        stack();
        render(<EnhancementList />);
        const [remove] = screen.getAllByRole("button", { name: "Remove enhancement" });

        expect(await tooltipOf(remove as HTMLElement)).toHaveTextContent(tip);
        expect(remove).toHaveAttribute("aria-keyshortcuts", "Delete Backspace");
    });

    it("follows a rebinding of the options key and drops it once unbound", async () => {
        useSettingsStore.getState().apply({ shortcuts: { openOptions1: ["Shift+q"], openOptions2: [] } });
        stack();
        render(<EnhancementList />);
        const rows = document.querySelectorAll<HTMLElement>("[data-slot='enhancement-trigger']");

        expect(await tooltipOf(rows[0] as HTMLElement)).toHaveTextContent("Denoise (Shift+Q)");
        expect(rows[1]).not.toHaveAttribute("aria-keyshortcuts");
    });
});

describe("the navbar's hint", () => {
    it.each([
        [true, "Settings (⌘ ,)", "Meta+,"],
        [false, "Settings (Ctrl+,)", "Control+,"],
    ])("names Settings' fixed key on this platform (mac: %s)", async (mac, tip, aria) => {
        vi.mocked(isMacOs).mockReturnValue(mac);
        render(<Navbar />);
        const button = screen.getByRole("button", { name: "Settings" });

        expect(await tooltipOf(button)).toHaveTextContent(tip);
        expect(button).toHaveAttribute("aria-keyshortcuts", aria);
    });
});
