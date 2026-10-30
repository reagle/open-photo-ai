import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import { forgetPhotoInfo, type PhotoInfo, photoInfo } from "./photoInfo";

// Mocked at the `invoke` boundary, so the name asserted below is the one that would actually go on the wire: the frontend
// half of a contract whose Rust half is `#[tauri::command] fn photo_info` in crates/gui/src/photo_info.rs.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invoked = invoke as unknown as Mock;

const INFO: PhotoInfo = {
    make: "Apple",
    model: "iPhone 15 Pro",
    lensModel: null,
    focalMm: 6.86,
    focal35mm: 24,
    fNumber: 1.78,
    exposureSeconds: 1 / 120,
    iso: 50,
    taken: "2026-10-18 16:03:50",
    takenOffset: "+02:00",
    width: 4032,
    height: 3024,
    colorProfile: "Display P3",
    hdr: "gainMap",
    located: true,
    renderedFNumber: null,
    rendered: [],
};

beforeEach(() => {
    invoked.mockReset();
    forgetPhotoInfo();
});

describe("photoInfo", () => {
    it("calls the command Rust registers, by name, with the photograph's identity", async () => {
        invoked.mockResolvedValue(INFO);

        await expect(photoInfo("0123456789abcdef")).resolves.toEqual(INFO);

        expect(invoked).toHaveBeenCalledWith("photo_info", { source: "0123456789abcdef" });
    });

    it("asks once per photograph, and again after a failure", async () => {
        invoked.mockRejectedValueOnce({ kind: "read", message: "gone" }).mockResolvedValue(INFO);

        await expect(photoInfo("feed")).rejects.toEqual({ kind: "read", message: "gone" });
        await expect(photoInfo("feed")).resolves.toEqual(INFO);
        await expect(photoInfo("feed")).resolves.toEqual(INFO);

        expect(invoked).toHaveBeenCalledTimes(2);
    });
});
