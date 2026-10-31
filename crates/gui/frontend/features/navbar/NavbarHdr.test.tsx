import { invoke } from "@tauri-apps/api/core";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import "@/i18n";
import { forgetPhotoInfo, type PhotoInfo } from "@/ipc/photoInfo";
import { HOLIDAY, openFiles, render, resetFileStore } from "@/test/support";
import { NavbarHdr } from "./NavbarHdr";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invoked = invoke as unknown as Mock;

/** A photograph whose file declares `hdr`, and says nothing else. */
const declaring = (hdr: PhotoInfo["hdr"]): PhotoInfo => ({
    make: null,
    model: null,
    lensModel: null,
    focalMm: null,
    focal35mm: null,
    fNumber: null,
    exposureSeconds: null,
    iso: null,
    taken: null,
    takenOffset: null,
    width: 3000,
    height: 2000,
    colorProfile: null,
    hdr,
    located: false,
    renderedFNumber: null,
    rendered: [],
});

const badge = () => document.querySelector<HTMLElement>("[data-slot='navbar-hdr']");

/** Renders the badge over `HOLIDAY` and lets the question about it be answered. */
const shown = async () => {
    openFiles(HOLIDAY);
    render(<NavbarHdr file={HOLIDAY} />);
    await act(async () => {});
};

/** What the badge's tooltip says, once it is focused open as a keyboard would open it. */
const tip = async () => {
    const trigger = badge();
    if (!trigger) throw new Error("no badge is drawn");
    fireEvent.focus(trigger);

    return within(await screen.findByRole("tooltip")).getByText(/./).textContent;
};

beforeEach(() => {
    invoked.mockReset();
    forgetPhotoInfo();
    resetFileStore();
});

describe("the navbar's HDR badge", () => {
    it("is absent for a photograph whose file declares no HDR", async () => {
        invoked.mockResolvedValue(declaring(null));

        await shown();

        expect(invoked).toHaveBeenCalledExactlyOnceWith("photo_info", { source: HOLIDAY.identity });
        expect(badge()).toBeNull();
    });

    it("says HDR for a gain map, and that only a JPEG export keeps it", async () => {
        invoked.mockResolvedValue(declaring("gainMap"));

        await shown();

        expect(badge()?.textContent).toBe("HDR");
        // What `opai::export::loss::assess` decides: kept by a JPEG written with it, SDR in every other format.
        expect(await tip()).toBe(
            "This photograph has an HDR gain map. Exporting as JPG (Ultra HDR) keeps it; plain JPG leaves it out, and the other formats are written as SDR for now.",
        );
    });

    it("names a PQ or HLG transfer, which no format keeps yet", async () => {
        invoked.mockResolvedValue(declaring("hlg"));

        await shown();

        expect(await tip()).toBe("This photograph is HDR (HLG). Every format is written as SDR for now.");
    });

    it("draws nothing while the question is out, nor once it has failed", async () => {
        let fail: (error: unknown) => void = () => {};
        invoked.mockReturnValue(
            new Promise((_, reject) => {
                fail = reject;
            }),
        );
        const logged = vi.spyOn(console, "error").mockImplementation(() => {});

        await shown();
        expect(badge()).toBeNull();

        await act(async () => fail({ kind: "read", message: "the runtime is going down" }));
        await waitFor(() => expect(logged).toHaveBeenCalled());
        expect(badge()).toBeNull();
        logged.mockRestore();
    });

    it("asks once per photograph, sharing the answer with the information card", async () => {
        invoked.mockResolvedValue(declaring("gainMap"));

        await shown();
        render(<NavbarHdr file={HOLIDAY} />);
        await act(async () => {});

        expect(invoked).toHaveBeenCalledOnce();
        expect(document.querySelectorAll("[data-slot='navbar-hdr']")).toHaveLength(2);
    });
});
