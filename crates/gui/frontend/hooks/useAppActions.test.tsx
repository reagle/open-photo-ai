import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import "@/i18n";
import i18n from "@/i18n";
import type { ImageRecord } from "@/ipc/images";
import { configureMenu, onMenuItem } from "@/ipc/menu";
import { isMacOs, isWindows } from "@/ipc/os";
import { quit } from "@/ipc/setup";
import { AppProviders } from "@/providers";
import { useFileStore } from "@/stores/files";
import { useSettingsDialogStore } from "@/stores/settingsDialog";
import { HOLIDAY, openFiles, resetFileStore, SUNSET } from "@/test/support";
import { useAppActions } from "./useAppActions";

// The real `platform()` reads a global the plugin's init script installs, which jsdom has none of.
vi.mock("@/ipc/os", () => ({ isMacOs: vi.fn(() => false), isWindows: vi.fn(() => false) }));
// The two halves of the menu's wire, pinned by `ipc/menu.test.ts`; here, what the hook does with them.
vi.mock("@/ipc/menu", () => ({
    configureMenu: vi.fn(() => Promise.resolve()),
    onMenuItem: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("@/ipc/setup", () => ({ quit: vi.fn(() => Promise.resolve()) }));
vi.mock("@/ipc/images", async (original) => ({
    ...(await original<typeof import("@/ipc/images")>()),
    openImages: vi.fn(() => Promise.resolve<ImageRecord[]>([])),
}));
// Closing an image releases its enhanced result in Rust, and there is no Rust behind jsdom.
vi.mock("@/ipc/enhance", async (importOriginal) => ({
    ...(await importOriginal<typeof import("@/ipc/enhance")>()),
    releaseEnhanced: vi.fn(() => Promise.resolve()),
    releaseAllEnhanced: vi.fn(() => Promise.resolve()),
}));

const { openImages } = await import("@/ipc/images");
const picker = openImages as Mock;

/** Presses a key at `target`, bubbling to the window, and answers the dispatched event. */
const press = (init: KeyboardEventInit, target: EventTarget = document.body) => {
    const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
    act(() => {
        target.dispatchEvent(event);
    });

    return event;
};

/** Mounts the hook, and on macOS answers the function the menu's items reach it through. */
const mount = () => {
    renderHook(useAppActions, { wrapper: AppProviders });

    const calls = vi.mocked(onMenuItem).mock.calls;

    return (id: unknown) => act(() => calls.at(-1)?.[0](id));
};

/** Puts a dialog over the window, as Radix marks one. */
const openDialog = () => {
    const dialog = document.createElement("div");
    dialog.setAttribute("role", "dialog");
    document.body.append(dialog);
};

const paths = () => useFileStore.getState().files.map((file) => file.path);

beforeEach(() => {
    vi.mocked(isMacOs).mockReturnValue(false);
    vi.mocked(isWindows).mockReturnValue(false);
    vi.mocked(onMenuItem).mockClear();
    vi.mocked(configureMenu).mockClear();
    vi.mocked(quit).mockClear();
    picker.mockClear();
    resetFileStore();
    openFiles(HOLIDAY, SUNSET);
    useFileStore.getState().setCurrentIndex(0);
});

afterEach(() => {
    document.body.replaceChildren();
});

describe.each([
    ["Windows", true],
    ["Linux", false],
])("useAppActions by key on %s", (_platform, windows) => {
    beforeEach(() => {
        vi.mocked(isWindows).mockReturnValue(windows);
    });

    it("closes the current image with Ctrl+W, and leaves the rest open", () => {
        mount();

        const event = press({ key: "w", ctrlKey: true });

        expect(event.defaultPrevented).toBe(true);
        expect(paths()).toEqual([SUNSET.path]);
    });

    it("closes every image with Ctrl+Shift+W", () => {
        mount();

        press({ key: "W", ctrlKey: true, shiftKey: true });

        expect(paths()).toEqual([]);
    });

    it("does nothing with Ctrl+W when no image is open", () => {
        resetFileStore();
        mount();

        press({ key: "w", ctrlKey: true });

        expect(paths()).toEqual([]);
    });

    it("opens the picker with Ctrl+O", () => {
        mount();

        press({ key: "o", ctrlKey: true });

        expect(picker).toHaveBeenCalledTimes(1);
    });

    it("opens Settings with Ctrl+comma", () => {
        mount();

        press({ key: ",", ctrlKey: true });

        expect(useSettingsDialogStore.getState().open).toBe(true);
    });

    it("does nothing while a dialog is open", () => {
        mount();
        openDialog();

        const event = press({ key: "w", ctrlKey: true });
        press({ key: "o", ctrlKey: true });

        expect(event.defaultPrevented).toBe(false);
        expect(paths()).toEqual([HOLIDAY.path, SUNSET.path]);
        expect(picker).not.toHaveBeenCalled();
    });

    it("leaves a text field its keys", () => {
        mount();
        const field = document.createElement("input");
        document.body.append(field);

        press({ key: "w", ctrlKey: true }, field);

        expect(paths()).toEqual([HOLIDAY.path, SUNSET.path]);
    });

    it("closes one image per press, not one per repeat", () => {
        mount();

        press({ key: "w", ctrlKey: true, repeat: true });

        expect(paths()).toEqual([HOLIDAY.path, SUNSET.path]);
    });

    it("does not relabel a menu there is none of", () => {
        mount();

        expect(configureMenu).not.toHaveBeenCalled();
        expect(onMenuItem).not.toHaveBeenCalled();
    });
});

describe("useAppActions' Quit", () => {
    it("quits with Ctrl+Q on Linux, even with a dialog open", () => {
        mount();
        openDialog();

        press({ key: "q", ctrlKey: true });

        expect(quit).toHaveBeenCalledTimes(1);
    });

    it("leaves Ctrl+Q alone on Windows, which quits with Alt+F4 itself", () => {
        vi.mocked(isWindows).mockReturnValue(true);
        mount();

        const event = press({ key: "q", ctrlKey: true });
        press({ key: "F4", altKey: true });

        expect(event.defaultPrevented).toBe(false);
        expect(quit).not.toHaveBeenCalled();
    });
});

describe("useAppActions from the macOS menu", () => {
    beforeEach(() => {
        vi.mocked(isMacOs).mockReturnValue(true);
    });

    it("acts on each of the application's items", () => {
        const choose = mount();

        choose("closeImage");
        expect(paths()).toEqual([SUNSET.path]);

        choose("open");
        expect(picker).toHaveBeenCalledTimes(1);

        choose("settings");
        expect(useSettingsDialogStore.getState().open).toBe(true);
        act(() => useSettingsDialogStore.getState().closeSettings());

        choose("closeAll");
        expect(paths()).toEqual([]);
    });

    it("ignores an id that is not one of its actions", () => {
        const choose = mount();

        choose("zoomIn");
        choose(42);

        expect(paths()).toEqual([HOLIDAY.path, SUNSET.path]);
    });

    it("does nothing while a dialog is open", () => {
        const choose = mount();
        openDialog();

        choose("closeImage");
        choose("closeAll");

        expect(paths()).toEqual([HOLIDAY.path, SUNSET.path]);
    });

    it("does not also act on the keys, which the menu holds", () => {
        mount();

        press({ key: "w", metaKey: true });
        press({ key: "w", ctrlKey: true });

        expect(paths()).toEqual([HOLIDAY.path, SUNSET.path]);
    });

    it("relabels the menu in the interface language, and follows whether any image is open", async () => {
        mount();

        expect(configureMenu).toHaveBeenLastCalledWith(
            expect.objectContaining({ closeImage: "Close Image", quit: "Quit Open Photo AI" }),
            true,
        );

        act(() => useFileStore.getState().closeAll());
        expect(configureMenu).toHaveBeenLastCalledWith(expect.anything(), false);

        await act(() => i18n.changeLanguage("fr"));
        expect(configureMenu).toHaveBeenLastCalledWith(
            expect.objectContaining({ file: "Fichier", closeImage: "Fermer l'image" }),
            false,
        );
        await act(() => i18n.changeLanguage("en"));
    });
});
