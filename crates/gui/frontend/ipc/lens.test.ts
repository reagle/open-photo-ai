import { invoke } from "@tauri-apps/api/core";
import { act } from "@testing-library/react";
import { beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import { useFileStore } from "@/stores/files";
import { HOLIDAY, resetFileStore, SUNSET } from "@/test/support";
import type { CropInfo } from "./crop";
import { forgetLensInfo, type LensInfo, lensInfo } from "./lens";

// Mocked at the `invoke` boundary, so the name asserted below is the one that would actually go on the wire. This is
// the frontend half of a contract whose Rust half is `#[tauri::command] fn lens_info` in crates/gui/src/lens.rs.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invoked = invoke as unknown as Mock;

/** One framing, as the Crop/Rotate dialog hands it over. */
const framing: CropInfo = {
    left: 10,
    top: 20,
    width: 30,
    height: 40,
    millidegrees: 0,
    flipHorizontal: false,
    flipVertical: false,
};

const LENS: LensInfo = { ownFNumber: 6, focal35mm: 36, origin: "exif" };

beforeEach(() => {
    invoked.mockReset();
    forgetLensInfo();
    resetFileStore();
});

describe("lensInfo", () => {
    it("calls the command Rust registers, by name, with the photograph's identity", async () => {
        invoked.mockResolvedValue(LENS);

        await expect(lensInfo("0123456789abcdef")).resolves.toEqual(LENS);

        expect(invoked).toHaveBeenCalledWith("lens_info", { source: "0123456789abcdef", crop: undefined });
    });

    it("sends the framing the lens is wanted for, and asks again for another framing", async () => {
        invoked.mockResolvedValue(LENS);

        await lensInfo("a", framing);
        await lensInfo("a", { ...framing });
        await lensInfo("a", { ...framing, width: 31 });
        await lensInfo("a");

        expect(invoked).toHaveBeenCalledTimes(3);
        expect(invoked).toHaveBeenNthCalledWith(1, "lens_info", { source: "a", crop: framing });
    });

    it("asks once per photograph, however many ask together or afterwards", async () => {
        invoked.mockResolvedValue(LENS);

        await Promise.all([lensInfo("a"), lensInfo("a")]);
        await lensInfo("a");
        await lensInfo("b");

        expect(invoked).toHaveBeenCalledTimes(2);
    });

    it("does not keep a rejection, so a failed question can be asked again", async () => {
        invoked.mockRejectedValueOnce({ kind: "unknownSource", identity: "a" });
        invoked.mockResolvedValueOnce(LENS);

        await expect(lensInfo("a")).rejects.toEqual({ kind: "unknownSource", identity: "a" });
        await expect(lensInfo("a")).resolves.toEqual(LENS);
    });

    it("forgets every framing of a photograph when it is closed, and no other's", async () => {
        invoked.mockResolvedValue(LENS);
        act(() => useFileStore.getState().addFiles([HOLIDAY, SUNSET]));
        const [held, other] = [HOLIDAY.identity ?? "", SUNSET.identity ?? ""];

        await lensInfo(held);
        await lensInfo(held, framing);
        await lensInfo(other);
        invoked.mockClear();

        act(() => useFileStore.getState().closeFile(HOLIDAY.path));
        await lensInfo(held);
        await lensInfo(held, framing);
        await lensInfo(other);

        // Both of the closed photograph's answers are asked for again; the other's is still held.
        expect(invoked).toHaveBeenCalledTimes(2);
    });

    it("forgets every answer when every photograph is closed", async () => {
        invoked.mockResolvedValue(LENS);
        act(() => useFileStore.getState().addFiles([HOLIDAY, SUNSET]));
        await lensInfo(HOLIDAY.identity ?? "");
        await lensInfo(SUNSET.identity ?? "");
        invoked.mockClear();

        act(() => useFileStore.getState().closeAll());
        await lensInfo(HOLIDAY.identity ?? "");

        expect(invoked).toHaveBeenCalledTimes(1);
    });
});
