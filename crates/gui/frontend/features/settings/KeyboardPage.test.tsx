import { act, fireEvent, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "@/i18n";
import { isMacOs, isWindows } from "@/ipc/os";
import type { StoredShortcuts } from "@/lib/shortcuts";
import { type SettingsData, useSettingsStore } from "@/stores/settings";
import { DraftHarness, render } from "@/test/support";
import { isRecording, KeyboardPage } from "./KeyboardPage.tsx";

vi.mock("@/ipc/os", () => ({ isMacOs: vi.fn(() => false), isWindows: vi.fn(() => false) }));

/** What the page has written to its draft since it mounted. */
let drafted: () => SettingsData;

const renderPage = (shortcuts: StoredShortcuts = {}) => {
    let latest: SettingsData;

    render(
        <DraftHarness
            seed={{ shortcuts }}
            onDraft={(values) => {
                latest = values;
            }}
        >
            <KeyboardPage />
        </DraftHarness>,
    );

    drafted = () => latest;
};

const addButton = (action: string) => screen.getByRole("button", { name: `Add shortcut for ${action}` });

/** Presses Add shortcut on an action's row, and answers the recorder it became. */
const startRecording = (action: string) => {
    const add = addButton(action);
    act(() => {
        add.focus();
        fireEvent.click(add);
    });

    return add;
};

/** Records one key press on an action's row, and answers the dispatched keydown. */
const record = (action: string, init: KeyboardEventInit) => {
    const recorder = startRecording(action);
    const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
    act(() => {
        recorder.dispatchEvent(event);
    });

    return event;
};

beforeEach(() => {
    vi.mocked(isMacOs).mockReturnValue(false);
    vi.mocked(isWindows).mockReturnValue(false);
    localStorage.clear();
    useSettingsStore.setState(useSettingsStore.getInitialState(), true);
});

describe("the Keyboard page", () => {
    it("lists every action with its default bindings, as this platform writes them", () => {
        renderPage();

        for (const name of ["Zoom in", "Zoom out", "Fit to window", "Show original while held"]) {
            expect(screen.getByText(name)).toBeInTheDocument();
        }
        expect(screen.getByText("Ctrl+=")).toBeInTheDocument();
        expect(screen.getByText("Ctrl+0")).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Remove \\, Show original while held" })).toBeInTheDocument();
    });

    it("prints macOS's modifier glyphs on macOS", () => {
        vi.mocked(isMacOs).mockReturnValue(true);
        renderPage();

        expect(screen.getByRole("button", { name: "Remove ⌘ =, Zoom in" })).toBeInTheDocument();
    });

    it("removes a binding into the draft, says when an action has none, and moves the focus to Add", async () => {
        vi.useFakeTimers();
        try {
            renderPage();

            fireEvent.click(screen.getByRole("button", { name: "Remove \\, Show original while held" }));
            act(() => {
                vi.runAllTimers();
            });

            expect(drafted().shortcuts).toEqual({ showOriginal: [] });
            expect(screen.getByText("No shortcut")).toBeInTheDocument();
            expect(addButton("Show original while held")).toHaveFocus();
            // Never the store: that is Save's.
            expect(useSettingsStore.getState().shortcuts).toEqual({});
        } finally {
            vi.useRealTimers();
        }
    });

    it("records a combination into the draft", () => {
        renderPage();

        const event = record("Zoom in", { key: "Z", shiftKey: true });

        expect(event.defaultPrevented).toBe(true);
        expect(drafted().shortcuts).toEqual({ zoomIn: ["Mod+=", "Shift+z"] });
        expect(screen.getByText("Shift+Z")).toBeInTheDocument();
    });

    it("keeps recording through a lone modifier, and announces what it is waiting for", () => {
        renderPage();

        const recorder = startRecording("Fit to window");
        expect(recorder).toHaveAttribute("data-shortcut-recorder");
        expect(isRecording(recorder)).toBe(true);
        expect(screen.getByText("Press the keys for Fit to window, or Escape to cancel.")).toBeInTheDocument();

        fireEvent.keyDown(recorder, { key: "Control", ctrlKey: true });
        expect(recorder).toHaveAttribute("data-shortcut-recorder");

        fireEvent.keyDown(recorder, { key: "9", ctrlKey: true });
        expect(recorder).not.toHaveAttribute("data-shortcut-recorder");
        expect(isRecording(recorder)).toBe(false);
        expect(drafted().shortcuts).toEqual({ fit: ["Mod+0", "Mod+9"] });
    });

    it("ends the recording on Escape, on Tab without preventing it, and on blur, adding nothing", () => {
        renderPage();

        let recorder = startRecording("Zoom out");
        fireEvent.keyDown(recorder, { key: "Escape" });
        expect(recorder).not.toHaveAttribute("data-shortcut-recorder");

        recorder = startRecording("Zoom out");
        const tab = new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true });
        act(() => {
            recorder.dispatchEvent(tab);
        });
        expect(tab.defaultPrevented).toBe(false);
        expect(recorder).not.toHaveAttribute("data-shortcut-recorder");

        recorder = startRecording("Zoom out");
        act(() => {
            recorder.blur();
        });
        expect(recorder).not.toHaveAttribute("data-shortcut-recorder");

        expect(drafted().shortcuts).toEqual({});
    });

    it("refuses a key kept for moving around the window, saying so", () => {
        renderPage();

        record("Zoom in", { key: "Escape", shiftKey: true });

        expect(screen.getByText("Shift+Escape is kept for moving around the window.")).toBeInTheDocument();
        expect(drafted().shortcuts).toEqual({});
    });

    it("refuses Command-Q on macOS as the system's", () => {
        vi.mocked(isMacOs).mockReturnValue(true);
        renderPage();

        record("Fit to window", { key: "q", metaKey: true });

        expect(screen.getByText("⌘ Q is used by the system.")).toBeInTheDocument();
        expect(drafted().shortcuts).toEqual({});
    });

    it("refuses Command-O on macOS as the application's", () => {
        vi.mocked(isMacOs).mockReturnValue(true);
        renderPage();

        record("Zoom in", { key: "o", metaKey: true });

        expect(screen.getByText("⌘ O is used by the application.")).toBeInTheDocument();
        expect(drafted().shortcuts).toEqual({});
    });

    it("refuses Ctrl+W and Ctrl+comma elsewhere as the application's", () => {
        renderPage();

        record("Zoom in", { key: "w", ctrlKey: true });
        expect(screen.getByText("Ctrl+W is used by the application.")).toBeInTheDocument();

        record("Zoom in", { key: ",", ctrlKey: true });
        expect(screen.getByText("Ctrl+, is used by the application.")).toBeInTheDocument();
        expect(drafted().shortcuts).toEqual({});
    });

    describe("the application's own keys", () => {
        /** The keys one fixed row shows, by the action's name. */
        const fixedKeys = (name: string) =>
            within(screen.getByRole("region", { name: "Application" }))
                .getByText(name)
                .closest("[data-slot='app-shortcut-row']")?.textContent;

        it.each([
            ["macOS", true, false, ["⌘ O", "⌘ ,", "⌘ W", "⇧⌘ W", "⌘ Q"]],
            ["Windows", false, true, ["Ctrl+O", "Ctrl+,", "Ctrl+W", "Ctrl+Shift+W", "Alt+F4"]],
            ["Linux", false, false, ["Ctrl+O", "Ctrl+,", "Ctrl+W", "Ctrl+Shift+W", "Ctrl+Q"]],
        ])("are listed on %s, as that platform writes them", (_platform, mac, windows, keys) => {
            vi.mocked(isMacOs).mockReturnValue(mac);
            vi.mocked(isWindows).mockReturnValue(windows);
            renderPage();

            const names = ["Open images", "Open Settings", "Close image", "Close all images", "Quit"];
            names.forEach((name, at) => {
                expect(fixedKeys(name)).toBe(`${name}${keys[at]}`);
            });
            expect(screen.getByText("These keys are fixed and cannot be changed.")).toBeInTheDocument();
        });

        it("are read-only: nothing to remove, nothing to add", () => {
            renderPage();

            const group = screen.getByRole("region", { name: "Application" });
            expect(within(group).queryAllByRole("button")).toEqual([]);
        });
    });

    it("refuses a key another action holds, and moves it with one click", async () => {
        vi.useFakeTimers();
        try {
            renderPage();

            record("Zoom in", { key: "0", ctrlKey: true });

            expect(screen.getByText("Ctrl+0 is already used by Fit to window.")).toBeInTheDocument();
            expect(drafted().shortcuts).toEqual({});

            fireEvent.click(screen.getByRole("button", { name: "Use here instead" }));
            act(() => {
                vi.runAllTimers();
            });

            expect(drafted().shortcuts).toEqual({ zoomIn: ["Mod+=", "Mod+0"], fit: [] });
            expect(screen.queryByText("Ctrl+0 is already used by Fit to window.")).not.toBeInTheDocument();
            expect(addButton("Zoom in")).toHaveFocus();
        } finally {
            vi.useRealTimers();
        }
    });

    it("adds nothing for a combination the action already has, including an equivalent one", () => {
        renderPage();

        record("Zoom in", { key: "+", ctrlKey: true, shiftKey: true });

        expect(drafted().shortcuts).toEqual({});
        expect(screen.queryByText(/already used/)).not.toBeInTheDocument();
    });

    it("swallows the keyup of a recorded Space, so the button is not pressed again", () => {
        renderPage({ showOriginal: [] });

        const recorder = startRecording("Show original while held");
        fireEvent.keyDown(recorder, { key: " ", code: "Space" });
        const keyup = new KeyboardEvent("keyup", { key: " ", bubbles: true, cancelable: true });
        act(() => {
            recorder.dispatchEvent(keyup);
        });

        expect(keyup.defaultPrevented).toBe(true);
        expect(drafted().shortcuts).toEqual({ showOriginal: ["Space"] });
    });

    it("offers no Add once an action has four bindings", () => {
        renderPage({ fit: ["1", "2", "3", "4"] });

        expect(addButton("Fit to window")).toBeDisabled();
        expect(addButton("Zoom in")).toBeEnabled();
    });

    it("lists the actions added for discovery, and says which key opens the list", () => {
        renderPage();

        for (const name of ["Actual pixels (100%)", "Toggle fit and actual pixels", "Previous image", "Next image"]) {
            expect(screen.getByText(name)).toBeInTheDocument();
        }
        expect(screen.getByText("Press ? anywhere to see these shortcuts.")).toBeInTheDocument();
    });

    it("says nothing about the list when no key opens it", () => {
        renderPage({ showShortcuts: [] });

        expect(screen.queryByText(/anywhere to see these shortcuts/)).not.toBeInTheDocument();
    });
});
