import { act, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "@/i18n";
import { forgetCatalogue } from "@/ipc/catalogue";
import type { Operation, RunProgress } from "@/ipc/enhance";
import { CATALOGUE, render } from "@/test/support";
import {
    ESTIMATE_TICK_MS,
    estimatedFraction,
    estimatedMinutes,
    PREPARING_NOTICE_DELAY_MS,
    ProgressBar,
} from "./ProgressBar";

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
        expect(chip()).not.toHaveTextContent("Compiling");
        expect(indicator()).toHaveStyle({ transform: "translateX(-75%)" });
    });

    it("names the model being compiled once it has taken long enough, and says it happens once", async () => {
        render(<ProgressBar report={preparing("CoreML")} fraction={0.25} operations={CHAIN} />);

        await linger();

        expect(chip()).toHaveTextContent("Compiling Kyoto model");
        expect(chip()).not.toHaveTextContent("Mac");
        expect(screen.getByText("First use only, may take a few minutes")).toBeInTheDocument();
        // Indeterminate: a compile has nothing to measure, and a bar standing still for minutes reads as a hang.
        expect(indicator()?.getAttribute("style") ?? "").not.toContain("translateX");
        expect(indicator()).toHaveClass("animate-progress-sweep");
    });

    it("says the same for TensorRT, without naming the GPU", async () => {
        render(<ProgressBar report={preparing("TensorRT")} fraction={0.25} operations={CHAIN} />);

        await linger();

        expect(chip()).toHaveTextContent("Compiling Kyoto model");
        expect(chip()).not.toHaveTextContent("GPU");
        expect(screen.getByText("First use only, may take a few minutes")).toBeInTheDocument();
    });

    it("promises nothing about a first use for a provider that does not compile", async () => {
        // A slow CPU load is still worth naming, but nothing about it happens only once.
        render(<ProgressBar report={preparing("CPU")} fraction={0.25} operations={CHAIN} />);

        await linger();

        expect(chip()).toHaveTextContent("Loading Kyoto model");
        expect(chip()).not.toHaveTextContent("Compiling");
        expect(screen.queryByText("First use only, may take a few minutes")).not.toBeInTheDocument();
    });

    it("names the enhancement where the chain holds no model of the report's family", async () => {
        // A detection, which no menu offers: the face recovery that asked for it is what the user chose.
        render(<ProgressBar report={preparing("CoreML", { family: "detection" })} fraction={0} operations={CHAIN} />);

        await linger();

        expect(chip()).toHaveTextContent("Compiling Face Recovery model");
    });

    it("waits out the delay again for the next session rather than inheriting the first one's", async () => {
        // A provider that would not open the model is followed by the CPU: a new wait, which may well be short.
        const { rerender } = render(<ProgressBar report={preparing("CoreML")} fraction={0.25} operations={CHAIN} />);
        await linger();
        expect(chip()).toHaveTextContent("Compiling Kyoto model");

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

describe("a session whose last compile is remembered", () => {
    /** Three minutes and six seconds, as Kazan's first CoreML compile took. */
    const LAST_TIME = 186_000;

    /** How far the chip's bar is filled, in percent, read off the indicator's offset. */
    const filled = () => {
        const transform = (indicator() as HTMLElement | null)?.style.transform ?? "";
        const offset = /translateX\(-([\d.]+)%\)/.exec(transform);

        return offset ? 100 - Number(offset[1]) : undefined;
    };

    /** Lets `ms` more pass, a tick at a time. */
    const pass = async (ms: number) => {
        await act(async () => {
            await vi.advanceTimersByTimeAsync(ms);
        });
    };

    it("says about how many minutes it took last time", async () => {
        render(
            <ProgressBar report={preparing("CoreML", { expectedMs: LAST_TIME })} fraction={0.25} operations={CHAIN} />,
        );

        await linger();

        expect(chip()).toHaveTextContent("Compiling Kyoto model");
        expect(screen.getByText("First use only, may take about 3 minutes")).toBeInTheDocument();
        expect(screen.queryByText("First use only, may take a few minutes")).not.toBeInTheDocument();
    });

    it("says one minute in the singular, and never fewer than one", async () => {
        render(<ProgressBar report={preparing("TensorRT", { expectedMs: 20_000 })} fraction={0} operations={CHAIN} />);

        await linger();

        expect(screen.getByText("First use only, may take about 1 minute")).toBeInTheDocument();
    });

    it("fills the bar against the time it took rather than sweeping", async () => {
        render(
            <ProgressBar report={preparing("CoreML", { expectedMs: LAST_TIME })} fraction={0.25} operations={CHAIN} />,
        );

        await linger();
        expect(indicator()).not.toHaveClass("animate-progress-sweep");
        const early = filled();
        expect(early).toBeDefined();
        expect(early).toBeLessThan(5);

        // It moves on its own, with no report arriving: a compile sends none.
        await pass(ESTIMATE_TICK_MS * 10);
        expect(filled()).toBeGreaterThan(early ?? 0);

        // Halfway through the last compile's time, the bar is near halfway through the part it fills linearly.
        await pass(LAST_TIME / 2 - PREPARING_NOTICE_DELAY_MS - ESTIMATE_TICK_MS * 10);
        expect(filled()).toBeGreaterThanOrEqual(43);
        expect(filled()).toBeLessThanOrEqual(46);
    });

    it("keeps creeping past the estimate but never reaches the end before the session opens", async () => {
        render(
            <ProgressBar report={preparing("CoreML", { expectedMs: LAST_TIME })} fraction={0.25} operations={CHAIN} />,
        );

        await pass(LAST_TIME);
        const atEstimate = filled() ?? 0;
        expect(atEstimate).toBeGreaterThanOrEqual(89);

        await pass(LAST_TIME / 2);
        const later = filled() ?? 0;
        expect(later).toBeGreaterThan(atEstimate);

        await pass(LAST_TIME * 10);
        expect(filled()).toBeLessThanOrEqual(95);
    });

    it("goes back to the run's own position once the session is open", async () => {
        const report = preparing("CoreML", { expectedMs: LAST_TIME });
        const { rerender } = render(<ProgressBar report={report} fraction={0.25} operations={CHAIN} />);
        await pass(LAST_TIME / 2);

        rerender(<ProgressBar report={{ ...report, stage: "running" }} fraction={0.3} operations={CHAIN} />);

        expect(chip()).not.toHaveTextContent("First use only");
        expect(indicator()).toHaveStyle({ transform: "translateX(-70%)" });
    });
});

describe("filling a bar against an estimate", () => {
    it("is linear up to nine tenths at the estimate", () => {
        expect(estimatedFraction(0, 1000)).toBe(0);
        expect(estimatedFraction(500, 1000)).toBeCloseTo(0.45);
        expect(estimatedFraction(1000, 1000)).toBeCloseTo(0.9);
    });

    it("approaches but never reaches 95% past it, always still moving", () => {
        const past = [1500, 2000, 4000, 20_000].map((elapsed) => estimatedFraction(elapsed, 1000));

        expect(past[0]).toBeCloseTo(0.925);
        for (let index = 1; index < past.length; index++) {
            expect(past[index]).toBeGreaterThan(past[index - 1] ?? 0);
        }
        expect(Math.max(...past)).toBeLessThan(0.95);
    });

    it("starts at the ceiling for an estimate that has already run out", () => {
        expect(estimatedFraction(0, 0)).toBeCloseTo(0.95);
    });

    it("rounds to whole minutes, and says at least one", () => {
        expect(estimatedMinutes(186_000)).toBe(3);
        expect(estimatedMinutes(150_000)).toBe(3);
        expect(estimatedMinutes(149_000)).toBe(2);
        expect(estimatedMinutes(5_000)).toBe(1);
    });
});
