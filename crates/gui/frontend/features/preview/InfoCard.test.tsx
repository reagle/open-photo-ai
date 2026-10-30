import { invoke } from "@tauri-apps/api/core";
import { act, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import "@/i18n";
import { forgetPhotoInfo, type PhotoInfo } from "@/ipc/photoInfo";
import { toggleInfo } from "@/lib/editorActions";
import { useEnhancementStore } from "@/stores/enhancements";
import { useInfoCardStore } from "@/stores/infoCard";
import { HOLIDAY, openFiles, render, resetFileStore } from "@/test/support";
import { InfoCard } from "./InfoCard";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invoked = invoke as unknown as Mock;

const SHOT: PhotoInfo = {
    make: "Sony",
    model: "ILCE-6700",
    lensModel: null,
    focalMm: 24,
    focal35mm: 36,
    fNumber: 4,
    exposureSeconds: 1 / 250,
    iso: 100,
    taken: "2026-10-18 16:03:50",
    takenOffset: null,
    width: 3000,
    height: 2000,
    colorProfile: null,
    hdr: null,
    located: false,
    renderedFNumber: null,
    rendered: [],
};

beforeEach(() => {
    invoked.mockReset().mockResolvedValue(SHOT);
    forgetPhotoInfo();
    resetFileStore();
    useInfoCardStore.setState(useInfoCardStore.getInitialState(), true);
    useEnhancementStore.setState(useEnhancementStore.getInitialState(), true);
});

describe("InfoCard", () => {
    it("is hidden until it is toggled, and asks for nothing while hidden", async () => {
        openFiles(HOLIDAY);
        render(<InfoCard bottom={0} />);
        await act(async () => {});

        expect(document.querySelector("[data-slot='info-card']")).toBeNull();
        expect(invoked).not.toHaveBeenCalled();
    });

    it("shows the photograph as shot, and a lens blur in the stack as rendered", async () => {
        openFiles(HOLIDAY);
        useEnhancementStore.setState({
            enhancements: new Map([
                [
                    HOLIDAY.path,
                    [{ family: "lens_blur", codename: "lisbon", precision: "fp16", parameters: { f_number: 1.4 } }],
                ],
            ]),
        });
        render(<InfoCard bottom={0} />);

        await act(async () => {
            expect(toggleInfo()).toBe(true);
        });

        expect(invoked).toHaveBeenCalledWith("photo_info", { source: HOLIDAY.identity });
        expect(await screen.findByText("Sony ILCE-6700")).toBeTruthy();
        expect(screen.getByText("24 mm · 36 mm eq. · f/4 → f/1.4 simulated · 1/250 s · ISO 100")).toBeTruthy();
        expect(screen.getByText("Edits: Lens Blur")).toBeTruthy();
    });

    it("is not toggled with no photograph open", () => {
        expect(toggleInfo()).toBe(false);
        expect(useInfoCardStore.getState().visible).toBe(false);
    });
});
