import { beforeEach, describe, expect, it, vi } from "vitest";
import { fNumberForAmount } from "@/lib/blurAmount";
import { useEnhancementStore } from "@/stores/enhancements";
import { CATALOGUE, HOLIDAY, resetSettingsStore, SUNSET } from "@/test/support";
import { applyRequest, prepareForRequest } from "./openRequest";

// Mocked at the IPC boundary: the catalogue the add menu reads, and a lens of f/8, the one assumed without EXIF.
vi.mock("@/ipc/catalogue", async (original) => ({
    ...(await original<typeof import("@/ipc/catalogue")>()),
    catalogue: vi.fn(() => Promise.resolve(CATALOGUE)),
}));
vi.mock("@/ipc/lens", async (original) => ({
    ...(await original<typeof import("@/ipc/lens")>()),
    lensInfo: vi.fn(() => Promise.resolve({ ownFNumber: 8, focal35mm: null, cropFactor: null, origin: "assumed" })),
}));

const stack = (path: string) => useEnhancementStore.getState().enhancements.get(path);

beforeEach(() => {
    useEnhancementStore.setState(useEnhancementStore.getInitialState(), true);
    resetSettingsStore();
});

describe("what opai --gui asked for a batch", () => {
    it("switches Autopilot off", () => {
        useEnhancementStore.getState().setAutopilot(true);

        prepareForRequest();

        expect(useEnhancementStore.getState().autopilot).toBe(false);
    });

    it("gives each photograph it names a lens blur at its f-number and focus, and none it does not name", async () => {
        await applyRequest({ paths: [HOLIDAY.path], lensBlur: { fNumber: 1.4, focusX: 0.3, focusY: 0.6 } }, [
            HOLIDAY,
            SUNSET,
        ]);

        expect(stack(HOLIDAY.path)).toEqual([
            {
                family: "lens_blur",
                codename: "lisbon",
                precision: "fp16",
                parameters: { f_number: 1.4, focus_x: 0.3, focus_y: 0.6 },
            },
        ]);
        expect(stack(SUNSET.path)).toBeUndefined();
    });

    it("reads a blur amount against the photograph's own lens, and starts at the default amount without either", async () => {
        await applyRequest({ paths: [HOLIDAY.path], lensBlur: { amount: 80 } }, [HOLIDAY]);
        await applyRequest({ paths: [SUNSET.path], lensBlur: {} }, [SUNSET]);

        expect(stack(HOLIDAY.path)?.[0]?.parameters).toEqual({
            f_number: fNumberForAmount(80, 8),
            focus_x: 0.5,
            focus_y: 0.5,
        });
        expect(stack(SUNSET.path)?.[0]?.parameters.f_number).toBe(fNumberForAmount(50, 8));
    });

    it("replaces a lens blur the photograph already has rather than adding a second", async () => {
        await applyRequest({ paths: [HOLIDAY.path], lensBlur: { fNumber: 4 } }, [HOLIDAY]);
        await applyRequest({ paths: [HOLIDAY.path], lensBlur: { fNumber: 1.4 } }, [HOLIDAY]);

        expect(stack(HOLIDAY.path)?.map((operation) => operation.parameters.f_number)).toEqual([1.4]);
    });
});
