import { act, fireEvent, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "@/i18n";
import { isMacOs } from "@/ipc/os";
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

        expect(keysFor(list, "Zoom in")).toBe("Ctrl+=");
        expect(keysFor(list, "Show original while held")).toBe("\\");
        expect(keysFor(list, "Next image")).toBe("→");
        expect(list).toHaveAccessibleDescription("The keys bound to each action now.");
        expect(within(list).getByText("Change them in Settings > Keyboard.")).toBeInTheDocument();
    });

    it("writes the keys as macOS does on macOS", () => {
        vi.mocked(isMacOs).mockReturnValue(true);
        const list = openList();

        expect(keysFor(list, "Actual pixels (100%)")).toBe("⌘ 1");
    });

    it("shows the user's bindings, and says when an action has none", () => {
        useSettingsStore.getState().apply({ shortcuts: { zoomIn: ["Shift+z"], fit: [] } });
        const list = openList();

        expect(keysFor(list, "Zoom in")).toBe("Shift+Z");
        expect(keysFor(list, "Fit to window")).toBe("No shortcut");
    });

    it("closes with Escape", () => {
        const list = openList();

        fireEvent.keyDown(list, { key: "Escape" });

        expect(useShortcutsListStore.getState().open).toBe(false);
        expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
});
