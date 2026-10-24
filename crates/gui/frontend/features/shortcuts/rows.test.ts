import { describe, expect, it } from "vitest";
import { SHORTCUT_ACTIONS } from "@/lib/shortcuts";
import { LIST_SECTIONS, rowKeys } from "./rows.ts";

describe("the shortcuts list's rows", () => {
    it("shows every action exactly once", () => {
        const listed = LIST_SECTIONS.flatMap((section) => section.rows.flatMap((row) => row.actions));

        expect([...listed].sort()).toEqual([...SHORTCUT_ACTIONS].sort());
    });
});

describe("rowKeys", () => {
    it("writes a run of digits as a range", () => {
        expect(rowKeys([["1"], ["2"], ["3"], ["4"]], false, "Space")).toEqual(["1–4"]);
    });

    it("writes each direction as its own whole combination", () => {
        const pan = [["Mod+ArrowLeft"], ["Mod+ArrowRight"], ["Mod+ArrowUp"], ["Mod+ArrowDown"]];

        expect(rowKeys(pan, true, "Space")).toEqual(["⌘ ←", "⌘ →", "⌘ ↑", "⌘ ↓"]);
        expect(rowKeys(pan, false, "Space")).toEqual(["Ctrl+←", "Ctrl+→", "Ctrl+↑", "Ctrl+↓"]);
    });

    it("writes a run of digits with a shared modifier as a range too", () => {
        expect(rowKeys([["Alt+1"], ["Alt+2"], ["Alt+3"]], true, "Space")).toEqual(["⌥ 1–3"]);
    });

    it("keeps a row of two as two key caps", () => {
        expect(rowKeys([["ArrowLeft"], ["ArrowRight"]], false, "Space")).toEqual(["←", "→"]);
    });

    it("lists every binding when the modifiers differ, an action has two, or one has none", () => {
        expect(rowKeys([["1"], ["Shift+2"], ["3"]], false, "Space")).toEqual(["1", "Shift+2", "3"]);
        expect(rowKeys([["1", "Mod+9"], ["2"], ["3"]], false, "Space")).toEqual(["1", "Ctrl+9", "2", "3"]);
        expect(rowKeys([["1"], [], ["3"]], false, "Space")).toEqual(["1", "3"]);
    });

    it("does not call digits out of order a range", () => {
        expect(rowKeys([["1"], ["3"], ["2"]], false, "Space")).toEqual(["1", "3", "2"]);
    });
});
