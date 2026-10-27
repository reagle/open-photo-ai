import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CropInfo } from "@/ipc/crop";
import { forgetLensInfo, type LensInfo, lensInfo } from "@/ipc/lens";
import { useLensInfo } from "./useLensInfo";

vi.mock("@/ipc/lens", async (importOriginal) => ({ ...(await importOriginal<object>()), lensInfo: vi.fn() }));
vi.mock("@/lib/report", () => ({ report: vi.fn() }));

const asked = lensInfo as unknown as ReturnType<typeof vi.fn>;

const BONSAI: LensInfo = { ownFNumber: 6, focal35mm: 36, origin: "exif" };
const NORA: LensInfo = { ownFNumber: 5.91, focal35mm: 24, origin: "exif" };

const framing: CropInfo = {
    left: 0,
    top: 0,
    width: 100,
    height: 80,
    millidegrees: 0,
    flipHorizontal: false,
    flipVertical: false,
};

beforeEach(() => {
    asked.mockReset();
    forgetLensInfo();
});

describe("the lens of the open photograph", () => {
    it("is unknown while the question is out, and then what was answered", async () => {
        asked.mockResolvedValue(BONSAI);

        const { result } = renderHook(() => useLensInfo("a"));

        expect(result.current).toBeUndefined();
        await waitFor(() => expect(result.current).toEqual(BONSAI));
        expect(asked).toHaveBeenCalledExactlyOnceWith("a", undefined);
    });

    it("is not asked about a file with no identity", () => {
        const { result } = renderHook(() => useLensInfo(undefined));

        expect(result.current).toBeUndefined();
        expect(asked).not.toHaveBeenCalled();
    });

    it("is null where the question failed", async () => {
        asked.mockRejectedValue(new Error("the bridge went away"));

        const { result } = renderHook(() => useLensInfo("a"));

        await waitFor(() => expect(result.current).toBeNull());
    });

    it("is never the previous photograph's answer for the next one", async () => {
        asked.mockImplementation((identity: string) => Promise.resolve(identity === "a" ? BONSAI : NORA));

        const { result, rerender } = renderHook(({ identity }) => useLensInfo(identity), {
            initialProps: { identity: "a" },
        });
        await waitFor(() => expect(result.current).toEqual(BONSAI));

        rerender({ identity: "b" });

        expect(result.current).toBeUndefined();
        await waitFor(() => expect(result.current).toEqual(NORA));
    });

    it("asks again for another framing, and not for the same one written again", async () => {
        asked.mockResolvedValue(BONSAI);

        const { result, rerender } = renderHook(({ crop }) => useLensInfo("a", crop), {
            initialProps: { crop: framing as CropInfo | undefined },
        });
        await waitFor(() => expect(result.current).toEqual(BONSAI));

        rerender({ crop: { ...framing } });
        expect(asked).toHaveBeenCalledTimes(1);

        rerender({ crop: { ...framing, width: 50 } });
        expect(result.current).toBeUndefined();
        await waitFor(() => expect(result.current).toEqual(BONSAI));
        expect(asked).toHaveBeenCalledTimes(2);
    });
});
