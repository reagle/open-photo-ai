import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import { type ExportLosses, exportLosses, type FormatChoice, type Loss } from "@/ipc/export";
import { useExportLosses } from "./useExportLosses";

vi.mock("@/ipc/export", () => ({ exportLosses: vi.fn() }));
vi.mock("@/lib/report", () => ({ report: vi.fn() }));

const asked = exportLosses as unknown as Mock;

/** The answers the test settles by hand, in the order they were asked for. */
const pending: ((losses: ExportLosses[]) => void)[] = [];

const QUEUE = ["/photos/hawk.jpg", "/photos/beach.jpg"];
const HDR: Loss = { kind: "hdr", hdr: "gainMap", format: "PNG", cause: "format" };

beforeEach(() => {
    pending.length = 0;
    asked.mockReset();
    asked.mockImplementation(() => new Promise<ExportLosses[]>((resolve) => pending.push(resolve)));
});

describe("useExportLosses", () => {
    it("is nothing until the first answer, then each path's losses", async () => {
        const { result } = renderHook(() => useExportLosses(QUEUE, "jpeg"));
        expect(result.current).toBeUndefined();
        expect(asked).toHaveBeenCalledExactlyOnceWith(QUEUE, { prefix: "", suffix: "", format: "jpeg" });

        await act(async () =>
            pending[0]?.([
                { path: "/photos/hawk.jpg", losses: [HDR] },
                { path: "/photos/beach.jpg", losses: [] },
            ]),
        );

        expect(result.current?.get("/photos/hawk.jpg")).toEqual([HDR]);
        expect(result.current?.get("/photos/beach.jpg")).toEqual([]);
    });

    it("asks again when the format changes, one question at a time, keeping only the latest answer", async () => {
        const { result, rerender } = renderHook(({ format }) => useExportLosses(QUEUE, format), {
            initialProps: { format: "jpeg" as FormatChoice },
        });

        // Asked while the first is unanswered: it waits, and a later one replaces it.
        rerender({ format: "avif" });
        rerender({ format: "heic" });
        expect(asked).toHaveBeenCalledOnce();

        // The first answer is superseded, and only then is the latest question asked; the one in between never is.
        await act(async () => pending[0]?.([{ path: "/photos/beach.jpg", losses: [HDR] }]));
        expect(result.current).toBeUndefined();
        expect(asked).toHaveBeenCalledTimes(2);
        expect(asked).toHaveBeenLastCalledWith(QUEUE, { prefix: "", suffix: "", format: "heic" });

        await act(async () => pending[1]?.([{ path: "/photos/beach.jpg", losses: [] }]));
        expect(result.current?.get("/photos/beach.jpg")).toEqual([]);
    });

    it("asks the waiting question after a failed one, and none after the dialog closes", async () => {
        asked.mockImplementationOnce(() => Promise.reject(new Error("unreadable")));
        const { rerender, unmount } = renderHook(({ format }) => useExportLosses(QUEUE, format), {
            initialProps: { format: "jpeg" as FormatChoice },
        });
        rerender({ format: "avif" });
        await act(async () => {});
        expect(asked).toHaveBeenCalledTimes(2);

        rerender({ format: "png" });
        unmount();
        await act(async () => pending[0]?.([]));
        expect(asked).toHaveBeenCalledTimes(2);
    });

    it("does not ask again for the same queue and format", () => {
        const { rerender } = renderHook(({ format }) => useExportLosses(QUEUE, format), {
            initialProps: { format: "png" as FormatChoice },
        });

        rerender({ format: "png" });

        expect(asked).toHaveBeenCalledOnce();
    });
});
