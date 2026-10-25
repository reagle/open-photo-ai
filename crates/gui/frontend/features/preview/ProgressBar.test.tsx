import { act, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "@/i18n";
import { forgetCatalogue } from "@/ipc/catalogue";
import type { Operation, RunProgress } from "@/ipc/enhance";
import { CATALOGUE, render } from "@/test/support";
import { PREPARING_NOTICE_DELAY_MS, ProgressBar } from "./ProgressBar";

// The catalogue the model's label is read from, answered as the library publishes it.
vi.mock("@tauri-apps/api/core", () => ({
    invoke: vi.fn(() => Promise.resolve(CATALOGUE)),
}));

/** The chain being run: one Kyoto upscale. */
const CHAIN: Operation[] = [{ family: "upscale", codename: "kyoto", precision: "fp16", parameters: { scale: 2 } }];

/** A report that the upscale's session is being opened on `provider`. */
const preparing = (provider: RunProgress["provider"], extra: Partial<RunProgress> = {}): RunProgress => ({
    run: "run-1",
    operation: "Kyoto 2x (FP16)",
    family: "upscale",
    stage: "preparing",
    chainFraction: 0.25,
    ...(provider && { provider }),
    ...extra,
});

const chip = () => document.querySelector("[data-slot='preview-progress']");
const indicator = () => chip()?.querySelector("[data-slot='progress-indicator']");

/** Lets the delay run out, and the catalogue's answer land. */
const linger = async () => {
    await act(async () => {
        await vi.advanceTimersByTimeAsync(PREPARING_NOTICE_DELAY_MS);
    });
};

beforeEach(() => {
    vi.useFakeTimers();
    forgetCatalogue();
});

afterEach(() => {
    vi.useRealTimers();
});

describe("a session being opened", () => {
    it("reads as the activity it is about to become until the delay has passed", () => {
        // A compiled model opens in about a second; a notice that it can take minutes would be flashed for nothing.
        render(<ProgressBar report={preparing("CoreML")} fraction={0.25} operations={CHAIN} />);

        expect(chip()).toHaveTextContent("Upscaling");
        expect(chip()).not.toHaveTextContent("Optimizing");
        expect(indicator()).toHaveStyle({ transform: "translateX(-75%)" });
    });

    it("names the model being optimized once it has taken long enough, and says it happens once", async () => {
        render(<ProgressBar report={preparing("CoreML")} fraction={0.25} operations={CHAIN} />);

        await linger();

        expect(chip()).toHaveTextContent("Optimizing Kyoto model");
        expect(chip()).not.toHaveTextContent("Mac");
        expect(screen.getByText("First use only, may take a few minutes")).toBeInTheDocument();
        // Indeterminate: a compile has nothing to measure, and a bar standing still for minutes reads as a hang.
        expect(indicator()?.getAttribute("style") ?? "").not.toContain("translateX");
        expect(indicator()).toHaveClass("animate-progress-sweep");
    });

    it("says the same for TensorRT, without naming the GPU", async () => {
        render(<ProgressBar report={preparing("TensorRT")} fraction={0.25} operations={CHAIN} />);

        await linger();

        expect(chip()).toHaveTextContent("Optimizing Kyoto model");
        expect(chip()).not.toHaveTextContent("GPU");
        expect(screen.getByText("First use only, may take a few minutes")).toBeInTheDocument();
    });

    it("promises nothing about a first use for a provider that does not compile", async () => {
        // A slow CPU load is still worth naming, but nothing about it happens only once.
        render(<ProgressBar report={preparing("CPU")} fraction={0.25} operations={CHAIN} />);

        await linger();

        expect(chip()).toHaveTextContent("Loading Kyoto model");
        expect(chip()).not.toHaveTextContent("Optimizing");
        expect(screen.queryByText("First use only, may take a few minutes")).not.toBeInTheDocument();
    });

    it("names the enhancement where the chain holds no model of the report's family", async () => {
        // A detection, which no menu offers: the face recovery that asked for it is what the user chose.
        render(<ProgressBar report={preparing("CoreML", { family: "detection" })} fraction={0} operations={CHAIN} />);

        await linger();

        expect(chip()).toHaveTextContent("Optimizing Face Recovery model");
    });

    it("waits out the delay again for the next session rather than inheriting the first one's", async () => {
        // A provider that would not open the model is followed by the CPU: a new wait, which may well be short.
        const { rerender } = render(<ProgressBar report={preparing("CoreML")} fraction={0.25} operations={CHAIN} />);
        await linger();
        expect(chip()).toHaveTextContent("Optimizing Kyoto model");

        rerender(<ProgressBar report={preparing("CPU")} fraction={0.25} operations={CHAIN} />);

        expect(chip()).toHaveTextContent("Upscaling");
    });

    it("goes back to the activity and the real position once the session is open", async () => {
        const { rerender } = render(<ProgressBar report={preparing("CoreML")} fraction={0.25} operations={CHAIN} />);
        await linger();

        rerender(
            <ProgressBar report={{ ...preparing(undefined), stage: "running" }} fraction={0.3} operations={CHAIN} />,
        );

        expect(chip()).toHaveTextContent("Upscaling");
        expect(chip()).not.toHaveTextContent("First use only");
        expect(indicator()).toHaveStyle({ transform: "translateX(-70%)" });
    });
});
