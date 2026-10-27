import { act, fireEvent, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "@/i18n";
import { isMacOs, isWindows } from "@/ipc/os";
import { useSettingsStore } from "@/stores/settings";
import { useShortcutsListStore } from "@/stores/shortcutsList";
import { render } from "@/test/support";
import { ShortcutsDialog } from "./ShortcutsDialog.tsx";

vi.mock("@/ipc/os", () => ({ isMacOs: vi.fn(() => false), isWindows: vi.fn(() => false) }));

const openList = () => {
    render(<ShortcutsDialog />);
    act(() => useShortcutsListStore.getState().setOpen(true));

    return screen.getByRole("dialog", { name: "Keyboard shortcuts" });
};

/** The keys one action's row shows. */
const keysFor = (list: HTMLElement, action: string) =>
    within(list).getByText(action).closest("div")?.querySelector("dd")?.textContent;

beforeEach(() => {
    vi.mocked(isMacOs).mockReturnValue(false);
    vi.mocked(isWindows).mockReturnValue(false);
    localStorage.clear();
    useSettingsStore.setState(useSettingsStore.getInitialState(), true);
    useShortcutsListStore.setState({ open: false });
});

describe("the keyboard shortcuts list", () => {
    it("is closed until asked for", () => {
        render(<ShortcutsDialog />);

        expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });

    it("lists every action with its keys, and says where to change them", () => {
        const list = openList();

        expect(keysFor(list, "Zoom in / Zoom out")).toBe("Ctrl+=Ctrl+-");
        expect(keysFor(list, "Show original while held")).toBe("\\");
        expect(keysFor(list, "Previous image / Next image")).toBe("←→");
        expect(list).toHaveAccessibleDescription("The keys bound to each action now.");
        expect(within(list).getByText("Change them in Settings > Keyboard.")).toBeInTheDocument();
    });

    it("lists the editor's actions with their keys, without being told about them", () => {
        const list = openList();

        expect(keysFor(list, "Turn Autopilot on or off")).toBe("A");
        expect(keysFor(list, "Add enhancement")).toBe("E");
        expect(keysFor(list, "Remove focused enhancement")).toBe("DeleteBackspace");
        expect(keysFor(list, "Options of enhancement 1–8")).toBe("1–8");
        expect(keysFor(list, "Crop/Rotate")).toBe("C");
        expect(keysFor(list, "Cycle comparison view")).toBe("V");
        expect(keysFor(list, "Show or hide images")).toBe("Ctrl+\\");
        expect(keysFor(list, "Show image in file manager")).toBe("Ctrl+Shift+R");
        expect(keysFor(list, "Pan")).toBe("Ctrl+←Ctrl+→Ctrl+↑Ctrl+↓");
    });

    it("writes the editor's keys as macOS does on macOS", () => {
        vi.mocked(isMacOs).mockReturnValue(true);
        const list = openList();

        expect(keysFor(list, "Remove focused enhancement")).toBe("⌦⌫");
        expect(keysFor(list, "Show or hide images")).toBe("⌘ \\");
        expect(keysFor(list, "Show image in file manager")).toBe("⇧⌘ R");
        expect(keysFor(list, "Pan")).toBe("⌘ ←⌘ →⌘ ↑⌘ ↓");
    });

    it("writes the keys as macOS does on macOS", () => {
        vi.mocked(isMacOs).mockReturnValue(true);
        const list = openList();

        expect(keysFor(list, "Actual pixels (100%)")).toBe("⌘ 1");
    });

    it("shows the user's bindings, and says when an action has none", () => {
        useSettingsStore.getState().apply({ shortcuts: { zoomIn: ["Shift+z"], fit: [] } });
        const list = openList();

        expect(keysFor(list, "Zoom in / Zoom out")).toBe("Shift+ZCtrl+-");
        expect(keysFor(list, "Fit to window")).toBe("No shortcut");
    });

    it.each([
        ["macOS", true, false, "⌘ W", "⌘ Q"],
        ["Windows", false, true, "Ctrl+W", "Alt+F4"],
        ["Linux", false, false, "Ctrl+W", "Ctrl+Q"],
    ])("heads the list with the application's own keys on %s", (_platform, mac, windows, close, quit) => {
        vi.mocked(isMacOs).mockReturnValue(mac);
        vi.mocked(isWindows).mockReturnValue(windows);
        const list = openList();

        const group = within(list).getByRole("region", { name: "Application" });
        expect(keysFor(group, "Close image")).toBe(close);
        expect(keysFor(group, "Quit")).toBe(quit);
        expect(keysFor(group, "Open images")).toBe(mac ? "⌘ O" : "Ctrl+O");
        expect(keysFor(group, "Export images")).toBe(mac ? "⌘ S⇧⌘ E" : "Ctrl+SCtrl+Shift+E");
    });

    it("groups the actions under View, Images and Enhancements", () => {
        const list = openList();

        expect(keysFor(within(list).getByRole("region", { name: "View" }), "Pan")).toBe("Ctrl+←Ctrl+→Ctrl+↑Ctrl+↓");
        expect(keysFor(within(list).getByRole("region", { name: "Images" }), "Crop/Rotate")).toBe("C");
        expect(keysFor(within(list).getByRole("region", { name: "Enhancements" }), "Add enhancement")).toBe("E");
    });

    it("lists each rebound direction of a combined row on its own", () => {
        useSettingsStore.getState().apply({ shortcuts: { panUp: ["Shift+ArrowUp"] } });
        const list = openList();

        expect(keysFor(list, "Pan")).toBe("Ctrl+←Ctrl+→Shift+↑Ctrl+↓");
    });

    it("lists Ctrl+slash beside ? for itself, as a binding the user may change", () => {
        const list = openList();

        expect(keysFor(list, "Show keyboard shortcuts")).toBe("?Ctrl+/");
    });

    it("closes with Escape", () => {
        const list = openList();

        fireEvent.keyDown(list, { key: "Escape" });

        expect(useShortcutsListStore.getState().open).toBe(false);
        expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
});
