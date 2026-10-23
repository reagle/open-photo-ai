import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { describe, expect, it, type Mock, vi } from "vitest";
import { configureMenu, onMenuItem } from "./menu";

// Mocked at the boundary, so the names asserted below are the ones that would go on the wire. This is
// the frontend half of a contract whose Rust half is `configure_menu` and `EVENT` in
// crates/gui/src/menu.rs.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

describe("configureMenu", () => {
    it("calls the command Rust registers, by name, with the labels, whether images are open and whether Export is", () => {
        configureMenu({ file: "Fichier" }, true, false);

        expect(invoke as unknown as Mock).toHaveBeenCalledWith("configure_menu", {
            labels: { file: "Fichier" },
            hasFiles: true,
            canExport: false,
        });
    });
});

describe("onMenuItem", () => {
    it("listens for the event Rust emits, and hands over the item's id", () => {
        const handler = vi.fn();
        onMenuItem(handler);

        const [[name, listener]] = (listen as unknown as Mock).mock.calls as [[string, (event: object) => void]];
        expect(name).toBe("app-menu");

        listener({ payload: "closeImage" });
        expect(handler).toHaveBeenCalledWith("closeImage");
    });
});
