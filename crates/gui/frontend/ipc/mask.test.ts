import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import { type Mask, maskPreview } from "./mask";

// Mocked at the `invoke` boundary, so the names asserted below are the ones that would actually go on the wire. This is
// the frontend half of a contract whose Rust half is `#[tauri::command] fn mask_preview` in crates/gui/src/mask.rs.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invoked = invoke as unknown as Mock;

const MASK: Mask = {
    strokes: [{ erase: false, radius: 0.025, softness: 0.5, points: [0.1, 0.2, 0.3, 0.4] }],
    inverted: false,
};

beforeEach(() => invoked.mockReset());

describe("maskPreview", () => {
    it("calls the command Rust registers, by name, with the mask and the size asked for", () => {
        invoked.mockResolvedValue(new ArrayBuffer(0));

        void maskPreview(MASK, 640, 480);

        expect(invoked).toHaveBeenCalledWith("mask_preview", { mask: MASK, width: 640, height: 480 });
    });

    it("answers the PNG's bytes as Rust sends them", async () => {
        const bytes = new Uint8Array([137, 80, 78, 71]).buffer;
        invoked.mockResolvedValue(bytes);

        await expect(maskPreview(MASK, 64, 48)).resolves.toBe(bytes);
    });

    it("rejects with what Rust rejected with", async () => {
        const refusal = { kind: "noSize", width: 0, height: 10 };
        invoked.mockRejectedValueOnce(refusal);

        await expect(maskPreview(MASK, 0, 10)).rejects.toEqual(refusal);
    });
});
