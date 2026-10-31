import { beforeEach, describe, expect, it, vi } from "vitest";
import { fNumberForAmount } from "@/lib/blurAmount";
import { useEnhancementStore } from "@/stores/enhancements";
import { useSettingsStore } from "@/stores/settings";
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

// The batch itself is `lib/batch.test.ts`'s; here only that a request sets one going, over what it named.
vi.mock("@/lib/batch", () => ({ startBatch: vi.fn() }));
const { startBatch } = await import("@/lib/batch");

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

    it("then pages through the photographs it named, each with its enhancements already in place", async () => {
        vi.mocked(startBatch).mockImplementationOnce(() => expect(stack(HOLIDAY.path)).toHaveLength(1));

        await applyRequest({ paths: [HOLIDAY.path], sharpen: { model: "novgorod", strength: 1.5 } }, [HOLIDAY, SUNSET]);

        expect(startBatch).toHaveBeenCalledWith([HOLIDAY.path]);
    });

    it("gives each photograph it names a sharpen of its model at its strength, and nothing else", async () => {
        await applyRequest({ paths: [HOLIDAY.path], sharpen: { model: "novgorod", strength: 1.5 } }, [HOLIDAY, SUNSET]);

        expect(stack(HOLIDAY.path)).toEqual([
            { family: "sharpen", codename: "novgorod", precision: "fp32", parameters: { strength: 1.5 } },
        ]);
        expect(stack(SUNSET.path)).toBeUndefined();
    });

    it("runs a sharpen at the tier of the saved sharpen model, as a run without -q does, and its default strength", async () => {
        useSettingsStore.setState({ models: { sharpen: "moscow_fp16" } });

        await applyRequest({ paths: [HOLIDAY.path], sharpen: { model: "petersburg" } }, [HOLIDAY]);

        expect(stack(HOLIDAY.path)).toEqual([
            { family: "sharpen", codename: "petersburg", precision: "fp16", parameters: { strength: 1 } },
        ]);
    });

    it("adds a sharpen and a lens blur together in the order a chain runs them", async () => {
        await applyRequest({ paths: [HOLIDAY.path], lensBlur: { fNumber: 2 }, sharpen: { model: "moscow" } }, [
            HOLIDAY,
        ]);

        expect(stack(HOLIDAY.path)?.map((operation) => operation.family)).toEqual(["sharpen", "lens_blur"]);
    });

    it("adds nothing for a sharpen model the catalogue does not publish", async () => {
        await applyRequest({ paths: [HOLIDAY.path], sharpen: { model: "atlantis" } }, [HOLIDAY]);

        expect(stack(HOLIDAY.path)).toBeUndefined();
    });

    it("replaces a sharpen the photograph already has rather than adding a second", async () => {
        await applyRequest({ paths: [HOLIDAY.path], sharpen: { model: "moscow", strength: 0.5 } }, [HOLIDAY]);
        await applyRequest({ paths: [HOLIDAY.path], sharpen: { model: "novgorod" } }, [HOLIDAY]);

        expect(stack(HOLIDAY.path)?.map((operation) => operation.codename)).toEqual(["novgorod"]);
    });

    it("replaces a lens blur the photograph already has rather than adding a second", async () => {
        await applyRequest({ paths: [HOLIDAY.path], lensBlur: { fNumber: 4 } }, [HOLIDAY]);
        await applyRequest({ paths: [HOLIDAY.path], lensBlur: { fNumber: 1.4 } }, [HOLIDAY]);

        expect(stack(HOLIDAY.path)?.map((operation) => operation.parameters.f_number)).toEqual([1.4]);
    });
});
