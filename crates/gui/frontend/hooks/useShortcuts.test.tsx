import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { revealImage } from "@/ipc/images";
import { isMacOs } from "@/ipc/os";
import { ZOOM_BUTTON_STEP, ZOOM_MIN } from "@/lib/constants";
import { useAddEnhancementMenuStore } from "@/stores/addEnhancementMenu";
import { useCropDialogStore } from "@/stores/cropDialog";
import { useDrawerStore } from "@/stores/drawer";
import { useEnhancementStore } from "@/stores/enhancements";
import { useFileStore } from "@/stores/files";
import { useOptionsPanelStore } from "@/stores/optionsPanel";
import { usePeekStore } from "@/stores/peek";
import { useSettingsStore } from "@/stores/settings";
import { useShortcutsListStore } from "@/stores/shortcutsList";
import { FITTED, registerPanner, useTransformStore } from "@/stores/transform";
import { HOLIDAY, openFiles, resetFileStore, SUNSET } from "@/test/support";
import { useShortcuts } from "./useShortcuts";

// The real `platform()` reads a global the plugin's init script installs, which jsdom has none of.
vi.mock("@/ipc/os", () => ({ isMacOs: vi.fn(() => false), isWindows: vi.fn(() => false) }));

// What showing a file in the file manager does is Rust's; here only that it is asked for, and for which file.
vi.mock("@/ipc/images", async (importOriginal) => ({
    ...(await importOriginal<typeof import("@/ipc/images")>()),
    revealImage: vi.fn(() => Promise.resolve()),
}));

/** Presses (or releases) a key at `target`, bubbling to the window, and answers the dispatched event. */
const fire = (type: "keydown" | "keyup", init: KeyboardEventInit, target: EventTarget = document.body) => {
    const event = new KeyboardEvent(type, { bubbles: true, cancelable: true, ...init });
    act(() => {
        target.dispatchEvent(event);
    });

    return event;
};

const SPACE = { key: " ", code: "Space" };
const BACKSLASH = { key: "\\", code: "Backslash" };

const scale = () => useTransformStore.getState().transforms.get(HOLIDAY.identity ?? "")?.scale;
const showing = () => usePeekStore.getState().showingOriginal;

beforeEach(() => {
    // Set every time rather than left to the mock's default: a value one case sets outlives it.
    vi.mocked(isMacOs).mockReturnValue(false);
    resetFileStore();
    useTransformStore.setState(useTransformStore.getInitialState(), true);
    usePeekStore.setState(usePeekStore.getInitialState(), true);
    localStorage.clear();
    useSettingsStore.setState(useSettingsStore.getInitialState(), true);
    openFiles(HOLIDAY);
});

afterEach(() => {
    document.body.replaceChildren();
});

describe("useShortcuts", () => {
    describe.each([
        ["macOS", true, { metaKey: true }],
        ["Windows and Linux", false, { ctrlKey: true }],
    ])("on %s", (_platform, mac, mod) => {
        beforeEach(() => {
            vi.mocked(isMacOs).mockReturnValue(mac);
        });

        it("zooms in, zooms out and fits the current image", () => {
            renderHook(() => useShortcuts());

            const zoomIn = fire("keydown", { key: "=", code: "Equal", ...mod });
            expect(scale()).toBe(ZOOM_MIN + ZOOM_BUTTON_STEP);
            expect(zoomIn.defaultPrevented).toBe(true);

            fire("keydown", { key: "=", code: "Equal", ...mod });
            fire("keydown", { key: "-", code: "Minus", ...mod });
            expect(scale()).toBe(ZOOM_MIN + ZOOM_BUTTON_STEP);

            act(() => useTransformStore.getState().setTransform(HOLIDAY.identity ?? "", { scale: 4, x: -200, y: -90 }));
            fire("keydown", { key: "0", code: "Digit0", ...mod });
            expect(useTransformStore.getState().transforms.get(HOLIDAY.identity ?? "")).toEqual(FITTED);
        });

        it("ignores the other platform's modifier", () => {
            renderHook(() => useShortcuts());

            const other = mac ? { ctrlKey: true } : { metaKey: true };
            const event = fire("keydown", { key: "=", code: "Equal", ...other });

            expect(scale()).toBeUndefined();
            expect(event.defaultPrevented).toBe(false);
        });
    });

    it("shows the original while backslash is held, ignoring key repeat, and the result once it is released", () => {
        renderHook(() => useShortcuts());
        const writes = vi.fn();
        const unsubscribe = usePeekStore.subscribe(writes);

        const press = fire("keydown", BACKSLASH);
        expect(showing()).toBe(true);
        expect(press.defaultPrevented).toBe(true);

        // Repeats are prevented (no scrolling) but write nothing, so nothing re-renders or flickers.
        const repeat = fire("keydown", { ...BACKSLASH, repeat: true });
        fire("keydown", { ...BACKSLASH, repeat: true });
        expect(repeat.defaultPrevented).toBe(true);
        expect(showing()).toBe(true);
        expect(writes).toHaveBeenCalledTimes(1);

        fire("keyup", BACKSLASH);
        expect(showing()).toBe(false);
        expect(writes).toHaveBeenCalledTimes(2);

        unsubscribe();
    });

    it("releases on the held key even if Shift went down meanwhile", () => {
        renderHook(() => useShortcuts());

        fire("keydown", BACKSLASH);
        expect(showing()).toBe(true);

        // Shift with backslash reads `|`; the physical key is still the one that started it.
        fire("keyup", { key: "|", code: "Backslash", shiftKey: true });
        expect(showing()).toBe(false);
    });

    it("ends the glance when Command is released, whose keyup is all macOS delivers", () => {
        renderHook(() => useShortcuts());

        fire("keydown", BACKSLASH);
        // Backslash let go while Command was down: macOS sends no keyup for it, only for Command.
        fire("keyup", { key: "Meta", code: "MetaLeft" });

        expect(showing()).toBe(false);
    });

    it("does not end the glance on some other key's release", () => {
        renderHook(() => useShortcuts());

        fire("keydown", BACKSLASH);
        fire("keyup", { key: "a", code: "KeyA" });

        expect(showing()).toBe(true);
    });

    it("puts the result back when the window loses focus while the key is held", () => {
        renderHook(() => useShortcuts());

        fire("keydown", BACKSLASH);
        act(() => {
            window.dispatchEvent(new Event("blur"));
        });

        expect(showing()).toBe(false);

        // And a fresh press after returning works again.
        fire("keydown", BACKSLASH);
        expect(showing()).toBe(true);
    });

    it("puts the result back when the window is hidden while the key is held", () => {
        renderHook(() => useShortcuts());
        fire("keydown", BACKSLASH);

        vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
        act(() => {
            document.dispatchEvent(new Event("visibilitychange"));
        });

        expect(showing()).toBe(false);
    });

    it("leaves every key in a text field to it", () => {
        renderHook(() => useShortcuts());
        const field = document.createElement("input");
        document.body.append(field);

        const backslash = fire("keydown", BACKSLASH, field);
        const zoom = fire("keydown", { key: "-", code: "Minus", ctrlKey: true }, field);
        fire("keydown", { key: "=", code: "Equal", ctrlKey: true }, field);

        expect(showing()).toBe(false);
        expect(scale()).toBeUndefined();
        expect(backslash.defaultPrevented).toBe(false);
        expect(zoom.defaultPrevented).toBe(false);
    });

    it("takes backslash on a focused button, and leaves Space there to the button", () => {
        renderHook(() => useShortcuts());
        const button = document.createElement("button");
        document.body.append(button);

        const space = fire("keydown", SPACE, button);
        expect(showing()).toBe(false);
        expect(space.defaultPrevented).toBe(false);

        fire("keydown", BACKSLASH, button);
        expect(showing()).toBe(true);
    });

    it("leaves keys alone while a dialog is open, but still lets a held key go", () => {
        renderHook(() => useShortcuts());

        fire("keydown", BACKSLASH);
        const dialog = document.createElement("div");
        dialog.setAttribute("role", "dialog");
        document.body.append(dialog);

        fire("keydown", { key: "0", code: "Digit0", ctrlKey: true });
        fire("keydown", { key: "=", code: "Equal", ctrlKey: true });
        expect(scale()).toBeUndefined();

        fire("keyup", BACKSLASH);
        expect(showing()).toBe(false);
    });

    it("leaves an unbound key alone, Space among them", () => {
        renderHook(() => useShortcuts());

        const event = fire("keydown", { key: "k", code: "KeyK" });
        // No default: a menu's trigger button gets the focus back on closing and reopens on Space.
        const space = fire("keydown", SPACE);

        expect(event.defaultPrevented).toBe(false);
        expect(space.defaultPrevented).toBe(false);
        expect(showing()).toBe(false);
    });

    it("stops listening, and lets go of a held key, once unmounted", () => {
        const { unmount } = renderHook(() => useShortcuts());
        fire("keydown", BACKSLASH);

        unmount();
        expect(showing()).toBe(false);

        const event = fire("keydown", { key: "=", code: "Equal", ctrlKey: true });
        fire("keydown", BACKSLASH);
        expect(scale()).toBeUndefined();
        expect(showing()).toBe(false);
        expect(event.defaultPrevented).toBe(false);
    });

    describe("with the user's bindings", () => {
        it("acts on a saved rebinding at once, without being mounted again, and drops the default it replaced", () => {
            renderHook(() => useShortcuts());

            act(() => useSettingsStore.getState().apply({ shortcuts: { zoomIn: ["Shift+z"] } }));

            const old = fire("keydown", { key: "=", code: "Equal", ctrlKey: true });
            expect(scale()).toBeUndefined();
            expect(old.defaultPrevented).toBe(false);

            fire("keydown", { key: "Z", code: "KeyZ", shiftKey: true });
            expect(scale()).toBe(ZOOM_MIN + ZOOM_BUTTON_STEP);
        });

        it("does nothing for an action bound to no key", () => {
            renderHook(() => useShortcuts());
            act(() => useSettingsStore.getState().apply({ shortcuts: { showOriginal: [] } }));

            const space = fire("keydown", SPACE);
            fire("keydown", BACKSLASH);

            expect(showing()).toBe(false);
            expect(space.defaultPrevented).toBe(false);
        });

        it("holds the original on Space once the user binds it, but leaves Space on a focused button to it", () => {
            renderHook(() => useShortcuts());
            act(() => useSettingsStore.getState().apply({ shortcuts: { showOriginal: ["Space", "\\"] } }));

            const press = fire("keydown", SPACE);
            expect(showing()).toBe(true);
            expect(press.defaultPrevented).toBe(true);
            fire("keyup", SPACE);
            expect(showing()).toBe(false);

            const button = document.createElement("button");
            document.body.append(button);
            const onButton = fire("keydown", SPACE, button);
            expect(showing()).toBe(false);
            expect(onButton.defaultPrevented).toBe(false);
        });

        it("gives a key the user bound to one action over another action's default", () => {
            renderHook(() => useShortcuts());
            act(() => useSettingsStore.getState().apply({ shortcuts: { zoomIn: ["Mod+0"] } }));
            act(() => useTransformStore.getState().setTransform(HOLIDAY.identity ?? "", { scale: 2, x: 0, y: 0 }));

            fire("keydown", { key: "0", code: "Digit0", ctrlKey: true });

            expect(scale()).toBe(2 + ZOOM_BUTTON_STEP);
        });
    });

    describe("the actions added for discovery", () => {
        afterEach(() => {
            vi.unstubAllGlobals();
            useShortcutsListStore.setState({ open: false });
        });

        it("shows actual pixels with Mod+1, and toggles it with Z", () => {
            vi.stubGlobal("devicePixelRatio", 1);
            renderHook(() => useShortcuts());
            act(() => useTransformStore.getState().setFit({ identity: HOLIDAY.identity ?? "", ratio: 0.25 }));

            fire("keydown", { key: "1", code: "Digit1", ctrlKey: true });
            expect(scale()).toBe(4);

            fire("keydown", { key: "z", code: "KeyZ" });
            expect(useTransformStore.getState().transforms.get(HOLIDAY.identity ?? "")).toEqual(FITTED);

            fire("keydown", { key: "z", code: "KeyZ" });
            expect(scale()).toBe(4);

            // Held down, the toggle does not flip back and forth on the key's repeat.
            fire("keydown", { key: "z", code: "KeyZ", repeat: true });
            expect(scale()).toBe(4);
        });

        it("moves to the next and previous image with the arrows", () => {
            act(() => {
                openFiles(SUNSET);
                // Opening makes the newcomer current; start from the first.
                useFileStore.getState().setCurrentIndex(0);
            });
            renderHook(() => useShortcuts());

            const next = fire("keydown", { key: "ArrowRight", code: "ArrowRight" });
            expect(useFileStore.getState().currentIndex).toBe(1);
            expect(next.defaultPrevented).toBe(true);

            fire("keydown", { key: "ArrowLeft", code: "ArrowLeft" });
            expect(useFileStore.getState().currentIndex).toBe(0);
        });

        it("leaves an arrow on a focused slider to the slider", () => {
            act(() => {
                openFiles(SUNSET);
                // Opening makes the newcomer current; start from the first.
                useFileStore.getState().setCurrentIndex(0);
            });
            renderHook(() => useShortcuts());
            const slider = document.createElement("span");
            slider.setAttribute("role", "slider");
            document.body.append(slider);

            const event = fire("keydown", { key: "ArrowRight", code: "ArrowRight" }, slider);

            expect(useFileStore.getState().currentIndex).toBe(0);
            expect(event.defaultPrevented).toBe(false);
        });

        it("opens the shortcuts list with ?, typed with Shift", () => {
            renderHook(() => useShortcuts());

            fire("keydown", { key: "?", code: "Slash", shiftKey: true });

            expect(useShortcutsListStore.getState().open).toBe(true);
        });

        it.each([
            ["Command-slash on macOS", true, { metaKey: true }],
            ["Ctrl+slash elsewhere", false, { ctrlKey: true }],
        ])("opens the shortcuts list with %s, Topaz's key", (_name, mac, mod) => {
            vi.mocked(isMacOs).mockReturnValue(mac);
            renderHook(() => useShortcuts());

            const event = fire("keydown", { key: "/", code: "Slash", ...mod });

            expect(event.defaultPrevented).toBe(true);
            expect(useShortcutsListStore.getState().open).toBe(true);
        });

        it("leaves Ctrl+slash to a text field", () => {
            renderHook(() => useShortcuts());
            const field = document.createElement("input");
            document.body.append(field);

            fire("keydown", { key: "/", code: "Slash", ctrlKey: true }, field);

            expect(useShortcutsListStore.getState().open).toBe(false);
        });
    });

    describe("the editor's actions", () => {
        beforeEach(() => {
            vi.mocked(revealImage).mockClear();
            useDrawerStore.setState(useDrawerStore.getInitialState(), true);
            useEnhancementStore.setState({ autopilot: true, enhancements: new Map() });
        });

        it.each([
            ["Command-Shift-R on macOS", true, { metaKey: true }],
            ["Ctrl+Shift+R elsewhere", false, { ctrlKey: true }],
        ])("shows the current image in the file manager with %s", (_name, mac, mod) => {
            vi.mocked(isMacOs).mockReturnValue(mac);
            renderHook(() => useShortcuts());

            const event = fire("keydown", { key: "R", code: "KeyR", shiftKey: true, ...mod });

            expect(event.defaultPrevented).toBe(true);
            expect(revealImage).toHaveBeenCalledExactlyOnceWith(HOLIDAY.path);
        });

        it("reveals nothing with no image open, and leaves the key alone", () => {
            resetFileStore();
            renderHook(() => useShortcuts());

            const event = fire("keydown", { key: "R", code: "KeyR", shiftKey: true, ctrlKey: true });

            expect(revealImage).not.toHaveBeenCalled();
            expect(event.defaultPrevented).toBe(false);
        });

        it("flips Autopilot once for one press, however long A is held", () => {
            renderHook(() => useShortcuts());

            fire("keydown", { key: "a", code: "KeyA" });
            const repeat = fire("keydown", { key: "a", code: "KeyA", repeat: true });
            fire("keydown", { key: "a", code: "KeyA", repeat: true });

            expect(useEnhancementStore.getState().autopilot).toBe(false);
            expect(repeat.defaultPrevented).toBe(true);
        });

        it("asks the canvas to pan, and repeats while the key is held", () => {
            const pan = vi.fn(() => true);
            const unregister = registerPanner(pan);
            renderHook(() => useShortcuts());

            fire("keydown", { key: "ArrowUp", code: "ArrowUp", ctrlKey: true });
            fire("keydown", { key: "ArrowUp", code: "ArrowUp", ctrlKey: true, repeat: true });
            fire("keydown", { key: "ArrowLeft", code: "ArrowLeft", ctrlKey: true });

            expect(pan.mock.calls).toEqual([["up"], ["up"], ["left"]]);
            // The bare arrows still step through the images.
            expect(useFileStore.getState().currentIndex).toBe(0);
            unregister();
        });

        it("opens the Add enhancement menu, Crop/Rotate and the first enhancement's options", () => {
            act(() =>
                useEnhancementStore.setState({
                    enhancements: new Map([
                        [HOLIDAY.path, [{ family: "upscale", codename: "kyoto", precision: "fp32", parameters: {} }]],
                    ]),
                }),
            );
            renderHook(() => useShortcuts());

            fire("keydown", { key: "e", code: "KeyE" });
            fire("keydown", { key: "c", code: "KeyC" });
            fire("keydown", { key: "1", code: "Digit1" });

            expect(useAddEnhancementMenuStore.getState().open).toBe(true);
            expect(useCropDialogStore.getState().open).toBe(true);
            expect(useOptionsPanelStore.getState().family).toBe("upscale");
        });

        it("leaves the editor's keys to a text field", () => {
            renderHook(() => useShortcuts());
            const field = document.createElement("input");
            document.body.append(field);

            for (const key of ["a", "e", "c", "v", "1", "Backspace", "Delete"]) fire("keydown", { key }, field);

            expect(useEnhancementStore.getState().autopilot).toBe(true);
            expect(useAddEnhancementMenuStore.getState().open).toBe(false);
            expect(useCropDialogStore.getState().open).toBe(false);
        });

        it("leaves the editor's keys alone while a dialog or a menu is open", () => {
            renderHook(() => useShortcuts());
            const menu = document.createElement("div");
            menu.setAttribute("role", "menu");
            document.body.append(menu);

            fire("keydown", { key: "a" });
            fire("keydown", { key: "\\", code: "Backslash", ctrlKey: true });

            expect(useEnhancementStore.getState().autopilot).toBe(true);
            expect(useDrawerStore.getState().open).toBe(false);
        });

        it("follows a rebinding of an editor action", () => {
            renderHook(() => useShortcuts());
            act(() => useSettingsStore.getState().apply({ shortcuts: { toggleAutopilot: ["Shift+p"] } }));

            fire("keydown", { key: "a", code: "KeyA" });
            expect(useEnhancementStore.getState().autopilot).toBe(true);

            fire("keydown", { key: "P", code: "KeyP", shiftKey: true });
            expect(useEnhancementStore.getState().autopilot).toBe(false);
        });
    });
});
