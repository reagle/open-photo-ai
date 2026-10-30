import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cropQuery } from "@/ipc/crop";
import type { ImageRecord } from "@/ipc/images";
import { screenBound, THUMBNAIL_BOUND } from "@/lib/constants";
import { useCropStore } from "@/stores/crop";
import { FRAMING, HOLIDAY, SUNSET } from "@/test/support";
import { useStandIn } from "./useStandIn";

// `convertFileSrc` reads a global Tauri's init script installs, which jsdom has none of. Answered
// with the identity it was asked for, which is all this file needs of a rendition URL.
vi.mock("@tauri-apps/api/core", () => ({
    convertFileSrc: vi.fn((identity: string) => `opai://localhost/${identity}`),
    invoke: vi.fn(() => Promise.resolve()),
}));

/**
 * An `HTMLImageElement.decode` whose promises are settled on demand, as `useCropController`'s tests hold one: jsdom
 * implements none, and the hook then draws each stage at once.
 */
const controllable = () => {
    const pending: (() => void)[] = [];
    const requested: string[] = [];

    Object.defineProperty(HTMLImageElement.prototype, "decode", {
        configurable: true,
        writable: true,
        value: vi.fn(function (this: HTMLImageElement) {
            requested.push(this.src);

            return new Promise<void>((resolve) => pending.push(resolve));
        }),
    });

    /** Finishes every decode asked for so far, one stage at a time as the webview would. */
    const finish = async () => {
        while (pending.length > 0) {
            const waiting = pending.splice(0);
            await act(async () => {
                for (const decode of waiting) decode();
            });
        }
    };

    return { requested, finish };
};

const url = (file: ImageRecord, bound: number, framing = "") =>
    `opai://localhost/${file.identity}${bound > 0 ? `?size=${bound}` : ""}${framing}`;

beforeEach(() => useCropStore.setState(useCropStore.getInitialState(), true));

afterEach(() => {
    delete (HTMLImageElement.prototype as { decode?: unknown }).decode;
});

describe("the canvas's stand-in", () => {
    it("draws the strip's thumbnail on the click, before anything has decoded", () => {
        controllable();

        const { result } = renderHook(() => useStandIn(SUNSET, undefined, false));

        // The bitmap the strip already holds: the click is answered in the frame it lands in.
        expect(result.current).toBe(url(SUNSET, THUMBNAIL_BOUND));
    });

    it("moves up to a screen-sized rendition once it has decoded, and no further while the photograph is fitted", async () => {
        const decode = controllable();

        const { result } = renderHook(() => useStandIn(SUNSET, undefined, false));
        await decode.finish();

        expect(result.current).toBe(url(SUNSET, screenBound()));
        // Encoding and decoding the original costs seconds on a busy machine, for pixels a fitted view cannot show.
        expect(decode.requested).not.toContain(url(SUNSET, 0));
    });

    it("asks for the photograph at its own size once it is magnified", async () => {
        const decode = controllable();

        const { result, rerender } = renderHook(({ magnified }) => useStandIn(SUNSET, undefined, magnified), {
            initialProps: { magnified: false },
        });
        await decode.finish();
        rerender({ magnified: true });
        await decode.finish();

        expect(result.current).toBe(url(SUNSET, 0));
    });

    it("never draws the previous photograph after the click, even while the new one decodes", async () => {
        const decode = controllable();

        const { result, rerender } = renderHook(({ file }) => useStandIn(file, undefined, false), {
            initialProps: { file: HOLIDAY },
        });
        await decode.finish();
        rerender({ file: SUNSET });

        // The bug this replaces: the old photograph and its result stayed on screen until the new original decoded.
        expect(result.current).toBe(url(SUNSET, THUMBNAIL_BOUND));
    });

    it("draws a framed photograph from its framing's renditions, so the stand-in has the box's shape", () => {
        controllable();
        const framing = `&crop=${cropQuery(FRAMING)}`;

        const { result } = renderHook(() => useStandIn(SUNSET, FRAMING, false));

        expect(result.current).toBe(url(SUNSET, THUMBNAIL_BOUND, framing));
    });

    it("draws a photograph whose header gave no size from its own pixels, having no box to put a stand-in in", () => {
        controllable();
        const { width, height, ...unsized } = SUNSET;

        const { result } = renderHook(() => useStandIn(unsized, undefined, false));

        expect(result.current).toBe(url(SUNSET, 0));
    });
});
