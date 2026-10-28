import { act, waitFor } from "@testing-library/react";
import { toast as sonner } from "sonner";
import { afterEach, beforeEach, describe, expect, it, type Mock, vi } from "vitest";
// Initialised here, not by `AppProviders`: the run's notice is composed through a `t` held in a ref
// and raised outside the tree, so a toast would otherwise read back as its own key.
import "@/i18n";
import type { CropInfo } from "@/ipc/crop";
import type { Enhancement, Operation, RunProgress } from "@/ipc/enhance";
import { cancelEnhance, enhance, onEnhanceProgress } from "@/ipc/enhance";
import { detectFaces, type Face } from "@/ipc/faces";
import { useEnhancementStore } from "@/stores/enhancements";
import { useFacesStore } from "@/stores/faces";
import { useFileStore } from "@/stores/files";
import { useMaskStore } from "@/stores/mask";
import { useSettingsStore } from "@/stores/settings";
import { FRAMING, HOLIDAY, openFiles, render, resetFileStore, SUNSET } from "@/test/support";
import { type EnhancementRun, useEnhancementRun } from "./useEnhancementRun";

// The seams themselves are pinned by `ipc/enhance.test.ts` and `ipc/faces.test.ts` against the wire;
// here they are mocked so this file is about what the hook asks for and what it does with the answers.
vi.mock("@/ipc/enhance", () => ({
    enhance: vi.fn(),
    cancelEnhance: vi.fn(() => Promise.resolve()),
    onEnhanceProgress: vi.fn(() => Promise.resolve(() => {})),
    // Not this hook's, but the enhancement store's: closing a photograph tells every owner to forget
    // it, and that one hands the result it was holding back to Rust.
    releaseEnhanced: vi.fn(() => Promise.resolve()),
}));

// Mocked only to say it is never reached: the run finds its own faces.
vi.mock("@/ipc/faces", () => ({ detectFaces: vi.fn() }));

const asked = enhance as unknown as Mock;
const stopped = cancelEnhance as unknown as Mock;
const subscribed = onEnhanceProgress as unknown as Mock;
const detected = detectFaces as unknown as Mock;

/** One run the test settles by hand, in the shape `enhance` answers with. */
type Pending = { run: string; settle: (outcome: Enhancement) => void; reject: (error: unknown) => void };

const pending: Pending[] = [];

/** Delivers a report to whatever the hook subscribed with. */
const deliver = (report: RunProgress) => {
    for (const [handler] of subscribed.mock.calls) act(() => (handler as (r: RunProgress) => void)(report));
};

const upscale = (scale: number): Operation => ({
    family: "upscale",
    codename: "kyoto",
    precision: "fp32",
    parameters: { scale },
});

/** A face recovery as the add menu creates one: a model, and no faces until the run path puts them in. */
const recovery = (): Operation => ({
    family: "face_recovery",
    codename: "athens",
    precision: "fp32",
    parameters: {},
});

const face = (left: number): Face => ({
    bounding_box: { min: { x: left, y: 4 }, max: { x: left + 3, y: 7 } },
    landmarks: [
        { x: left + 1, y: 5 },
        { x: left + 2, y: 5 },
        { x: left + 1.5, y: 6 },
        { x: left + 1, y: 6.5 },
        { x: left + 2, y: 6.5 },
    ],
    confidence: 0.9,
    restorable: true,
    key: `${left},4,${left + 3},7`,
});

const progress = (run: string, extra: Partial<RunProgress> = {}): RunProgress => ({
    run,
    operation: "Kyoto 2x (FP32)",
    family: "upscale",
    stage: "running",
    chainFraction: 0.4,
    ...extra,
});

/** What the hook answers, kept current by a probe component mounted the way the canvas mounts it. */
let answer: EnhancementRun;

const Probe = ({ file = HOLIDAY, crop }: { file?: typeof HOLIDAY; crop?: CropInfo }) => {
    answer = useEnhancementRun(file, crop);

    return null;
};

const mount = (file = HOLIDAY) => render(<Probe file={file} />);

/** A second framing, so a change of crop is a change of value rather than of one of its fields. */
const REFRAMED: CropInfo = { ...FRAMING, left: FRAMING.left + 200 };

/** Puts a stack on one image, which is what starts a run. */
const stack = (path: string, ...operations: Operation[]) =>
    act(() => useEnhancementStore.setState({ enhancements: new Map([[path, operations]]) }));

beforeEach(() => {
    vi.clearAllMocks();
    pending.length = 0;
    sonner.dismiss();

    let minted = 0;
    asked.mockImplementation(() => {
        const run = `run-${++minted}`;
        const entry: Pending = { run, settle: () => {}, reject: () => {} };

        entry.settle = () => {};
        const done = new Promise<Enhancement>((resolve, reject) => {
            entry.settle = (outcome) => act(() => resolve(outcome));
            entry.reject = (error) => act(() => reject(error));
        });
        pending.push(entry);

        return { run, done };
    });

    useEnhancementStore.setState({ autopilot: true, enhancements: new Map() });
    useFacesStore.setState(useFacesStore.getInitialState(), true);
    useSettingsStore.setState({ processor: "auto" });

    // Both photographs are open, as they are wherever this hook is given one: the canvas hands it a
    // record out of the file store, and what a detection answers is held only while that record is
    // still there.
    resetFileStore();
    openFiles(HOLIDAY, SUNSET);
});

describe("running the current image's enhancements", () => {
    it("asks for nothing while the image has none", () => {
        mount();

        // The seam accepts an empty chain and answers with the source, but that is a round trip for
        // an answer the window holds: the pane is pointed at the source directly.
        expect(asked).not.toHaveBeenCalled();
        expect(answer.enhanced).toBeUndefined();
    });

    it("runs the stack over the image and answers what it produced", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        expect(asked).toHaveBeenCalledWith(HOLIDAY.identity, [upscale(2)], "auto", undefined, undefined);

        pending[0]?.settle({ outcome: "enhanced", identity: "abcdef0123456789", width: 6000, height: 4000 });

        await waitFor(() =>
            expect(answer.enhanced).toEqual({ identity: "abcdef0123456789", width: 6000, height: 4000 }),
        );
    });

    it("runs again when the stack changes, and stops the run it replaces", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        stack(HOLIDAY.path, upscale(4));

        await waitFor(() => expect(asked).toHaveBeenCalledTimes(2));
        expect(stopped).toHaveBeenCalledWith("run-1");
        expect(asked).toHaveBeenLastCalledWith(HOLIDAY.identity, [upscale(4)], "auto", undefined, undefined);
    });

    it("runs again when the processor changes", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        act(() => useSettingsStore.setState({ processor: "cpu" }));

        await waitFor(() =>
            expect(asked).toHaveBeenLastCalledWith(HOLIDAY.identity, [upscale(2)], "cpu", undefined, undefined),
        );
    });

    it("runs again when the framing changes, and stops the run it replaces", async () => {
        stack(HOLIDAY.path, upscale(2));
        const view = mount();

        view.rerender(<Probe file={HOLIDAY} crop={FRAMING} />);

        // The same rule the stack, the image and the processor already follow: the run in flight is
        // producing pixels for a framing nobody is looking at any more.
        await waitFor(() => expect(asked).toHaveBeenCalledTimes(2));
        expect(stopped).toHaveBeenCalledWith("run-1");
        expect(asked).toHaveBeenLastCalledWith(HOLIDAY.identity, [upscale(2)], "auto", FRAMING, undefined);
    });

    it("lets go of the result the previous framing produced", async () => {
        stack(HOLIDAY.path, upscale(2));
        const view = render(<Probe file={HOLIDAY} crop={FRAMING} />);

        pending[0]?.settle({ outcome: "enhanced", identity: "abcdef0123456789", width: 2400, height: 3200 });
        await waitFor(() => expect(answer.enhanced).toBeDefined());

        view.rerender(<Probe file={HOLIDAY} crop={REFRAMED} />);

        // The image-change behaviour rather than the enhancement-change behaviour, and a deliberate
        // divergence from the reference, which keeps the previous result on screen. The canvas sizes
        // its box from the framing's own dimensions, so a result made from other pixels would be
        // drawn to the wrong proportions until the re-run landed. D10.
        await waitFor(() => expect(answer.enhanced).toBeUndefined());

        // And the pixels themselves go: asking for a run is what displaces whatever the backend was
        // holding, enforced there rather than trusted to this side - see `ipc/enhance.ts`.
        expect(asked).toHaveBeenCalledTimes(2);
    });

    it("keeps the result when an enhancement changes, where it drops it for a framing", async () => {
        stack(HOLIDAY.path, upscale(2));
        render(<Probe file={HOLIDAY} crop={FRAMING} />);

        pending[0]?.settle({ outcome: "enhanced", identity: "abcdef0123456789", width: 2400, height: 3200 });
        await waitFor(() => expect(answer.enhanced).toBeDefined());

        stack(HOLIDAY.path, upscale(4));

        // The other half of D10, and why the two are not one rule: an enhancement does not change the
        // shape of what is drawn, so the previous result stays on screen until a newer one lands -
        // which is what stops the scale field flashing the source between keystrokes.
        await waitFor(() => expect(asked).toHaveBeenCalledTimes(2));
        expect(answer.enhanced).toEqual({ identity: "abcdef0123456789", width: 2400, height: 3200 });
    });

    it("asks for no run when the framing of an image with no enhancements changes", async () => {
        const view = mount();

        view.rerender(<Probe file={HOLIDAY} crop={FRAMING} />);

        // An empty stack asks for nothing whatever else changes: the seam would answer with the
        // source, and that is a round trip for an answer the window already holds. The canvas draws
        // the newly framed source, which it gets from the rendition URL rather than from a run.
        await act(async () => {});
        expect(asked).not.toHaveBeenCalled();
        expect(answer.enhanced).toBeUndefined();
    });

    it("stops a run nothing is waiting for any more", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        // The last enhancement removed: no new run displaces this one, so the explicit stop is the
        // only thing that ends it.
        stack(HOLIDAY.path);

        await waitFor(() => expect(stopped).toHaveBeenCalledWith("run-1"));
        expect(asked).toHaveBeenCalledTimes(1);
        expect(answer.enhanced).toBeUndefined();
    });

    it("drops the result when the image changes", async () => {
        stack(HOLIDAY.path, upscale(2));
        const view = mount();

        pending[0]?.settle({ outcome: "enhanced", identity: "abcdef0123456789", width: 6000, height: 4000 });
        await waitFor(() => expect(answer.enhanced).toBeDefined());

        view.rerender(<Probe file={SUNSET} />);

        // A result belongs to the photograph it was made from; drawing it over another one is the
        // one thing the second pane is not for.
        await waitFor(() => expect(answer.enhanced).toBeUndefined());
    });

    it("writes nothing for a run that was stopped", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        pending[0]?.settle({ outcome: "stopped" });

        // A displaced run answers `stopped` whatever it returned, so the view goes on showing what
        // it was showing rather than a partly-enhanced image.
        await waitFor(() => expect(answer.enhanced).toBeUndefined());
    });

    it("holds the last result while the run that replaces it works", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        pending[0]?.settle({ outcome: "enhanced", identity: "abcdef0123456789", width: 6000, height: 4000 });
        await waitFor(() => expect(answer.enhanced).toBeDefined());

        stack(HOLIDAY.path, upscale(4));
        await waitFor(() => expect(asked).toHaveBeenCalledTimes(2));
        pending[0]?.settle({ outcome: "stopped" });

        // Typing in the scale field re-runs on every keystroke; flashing the source between them is
        // what holding the last result avoids.
        await waitFor(() => expect(answer.enhanced?.identity).toBe("abcdef0123456789"));
    });
});

describe("what the canvas is told about a run", () => {
    it("says a run is in flight before it has reported anything", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        // Which is what the indicator is drawn on: a run loads a model before it reports, and an
        // indicator that waited for the first report appeared long after the enhancement was added.
        await waitFor(() => expect(answer.running).toBe(true));
        expect(answer.report).toBeUndefined();
    });

    it("stops saying so once the run has ended", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        await waitFor(() => expect(answer.running).toBe(true));

        pending[0]?.settle({ outcome: "enhanced", identity: "abcdef0123456789", width: 6000, height: 4000 });

        await waitFor(() => expect(answer.running).toBe(false));
    });

    it("says nothing is in flight once the last enhancement is removed", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        await waitFor(() => expect(answer.running).toBe(true));

        stack(HOLIDAY.path);

        await waitFor(() => expect(answer.running).toBe(false));
    });

    it("keeps the reports naming its own run", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        deliver(progress("run-1"));
        await waitFor(() => expect(answer.report?.chainFraction).toBe(0.4));
    });

    it("discards a report from a run it has abandoned", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        stack(HOLIDAY.path, upscale(4));
        await waitFor(() => expect(asked).toHaveBeenCalledTimes(2));

        // A displaced run goes on reporting until it notices it has been stopped, which is what
        // makes those reports discardable rather than confusing.
        deliver(progress("run-1", { chainFraction: 0.9 }));

        expect(answer.report).toBeUndefined();
    });

    it("stops reporting once the run has ended", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        deliver(progress("run-1"));
        await waitFor(() => expect(answer.report).toBeDefined());

        pending[0]?.settle({ outcome: "enhanced", identity: "abcdef0123456789", width: 6000, height: 4000 });

        await waitFor(() => expect(answer.report).toBeUndefined());
    });

    it("reports nothing for a run whose every operation was already known", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        // Such a chain produces no report at all - each operation was neither fetched nor run - so
        // the bar is drawn empty and generically labelled for as long as the chain takes, and never
        // says which enhancement it is on.
        pending[0]?.settle({ outcome: "enhanced", identity: "abcdef0123456789", width: 6000, height: 4000 });

        await waitFor(() => expect(answer.enhanced).toBeDefined());
        expect(answer.report).toBeUndefined();
    });
});

describe("a run that does not finish", () => {
    const toast = () => document.querySelector("[data-sonner-toast]");

    it("tells the user when a run fails", async () => {
        const logged = vi.spyOn(console, "error").mockImplementation(() => {});
        const failure = { kind: "enhance", message: "no execution provider could be built" };
        stack(HOLIDAY.path, upscale(2));
        mount();

        pending[0]?.reject(failure);

        await waitFor(() => expect(toast()).toHaveTextContent("Failed to enhance image"));
        expect(answer.enhanced).toBeUndefined();
        expect(logged).toHaveBeenCalledWith("enhancing the image failed", failure);
    });

    it("says nothing about a run that was stopped", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        pending[0]?.settle({ outcome: "stopped" });

        // The user asked for it to stop. Reporting that as a failure would tell them their
        // enhancement broke when they had merely changed their mind.
        await waitFor(() => expect(answer.enhanced).toBeUndefined());
        expect(toast()).toBeNull();
    });
});

describe("a chain that restores faces", () => {
    const toast = () => document.querySelector("[data-sonner-toast]");

    const faces = () => useFacesStore.getState().faces;

    const enhanced = (extra: Partial<Extract<Enhancement, { outcome: "enhanced" }>> = {}): Enhancement => ({
        outcome: "enhanced",
        identity: "abcdef0123456789",
        width: 6000,
        height: 4000,
        ...extra,
    });

    it("asks for the chain at once, and never detects first", () => {
        // The run finds its own faces, inside the one request: one progress stream, one stop.
        stack(HOLIDAY.path, recovery(), upscale(2));
        mount();

        expect(detected).not.toHaveBeenCalled();
        expect(asked).toHaveBeenCalledExactlyOnceWith(
            HOLIDAY.identity,
            [recovery(), upscale(2)],
            "auto",
            undefined,
            undefined,
        );
    });

    it("sends the choice made among the photograph's faces, and leaves the stack without it", async () => {
        const choice = { skipped: [face(0).key], restored: [] };
        act(() => useFacesStore.getState().setFaceChoice(HOLIDAY.identity ?? "", choice));
        stack(HOLIDAY.path, recovery(), upscale(2));
        mount();

        expect(asked).toHaveBeenCalledWith(
            HOLIDAY.identity,
            [{ ...recovery(), faces: choice }, upscale(2)],
            "auto",
            undefined,
            undefined,
        );
        expect(useEnhancementStore.getState().enhancements.get(HOLIDAY.path)).toEqual([recovery(), upscale(2)]);
    });

    it("records the faces the run found, at the framing it ran at, and re-runs nothing for them", async () => {
        stack(HOLIDAY.path, recovery());
        mount();

        pending[0]?.settle(enhanced({ faces: [face(0), face(20)] }));

        await waitFor(() => expect(faces().get(HOLIDAY.identity ?? "")).toEqual({ faces: [face(0), face(20)] }));
        await act(async () => {});
        expect(asked).toHaveBeenCalledTimes(1);
    });

    it("starts exactly one run for an applied choice, and none for a render that wrote none", async () => {
        stack(HOLIDAY.path, recovery());
        const view = mount();

        view.rerender(<Probe />);
        expect(asked).toHaveBeenCalledTimes(1);

        act(() => useFacesStore.getState().setFaceChoice(HOLIDAY.identity ?? "", { skipped: ["a"], restored: [] }));

        await waitFor(() => expect(asked).toHaveBeenCalledTimes(2));
        expect(stopped).toHaveBeenCalledWith("run-1");
    });

    it("leaves the chain alone for a choice recorded against another photograph", async () => {
        stack(HOLIDAY.path, recovery());
        mount();

        act(() => useFacesStore.getState().setFaceChoice(SUNSET.identity ?? "", { skipped: ["a"], restored: [] }));
        await act(async () => {});

        expect(asked).toHaveBeenCalledTimes(1);
    });

    it("tells the user when the faces could not be found, and keeps the result the chain still made", async () => {
        const logged = vi.spyOn(console, "error").mockImplementation(() => {});
        stack(HOLIDAY.path, recovery(), upscale(2));
        mount();

        pending[0]?.settle(enhanced({ facesError: "no execution provider could be built" }));

        await waitFor(() => expect(toast()).toHaveTextContent("Failed to detect faces"));
        expect(logged).toHaveBeenCalledWith(
            "detecting the faces in the image failed",
            "no execution provider could be built",
        );
        // Recorded as none, so the row says so rather than waiting for an answer that is not coming.
        expect(faces().get(HOLIDAY.identity ?? "")).toEqual({ faces: [] });
        expect(answer.enhanced).toEqual({ identity: "abcdef0123456789", width: 6000, height: 4000 });
    });

    it("tells the user when the lens blur's subject could not be matted, and keeps the result", async () => {
        const logged = vi.spyOn(console, "error").mockImplementation(() => {});
        stack(HOLIDAY.path, upscale(2));
        mount();

        pending[0]?.settle(enhanced({ matteError: "Porto failed on tile 3" }));

        await waitFor(() => expect(toast()).toHaveTextContent("Failed to matte the subject"));
        expect(logged).toHaveBeenCalledWith("matting the lens blur's subject failed", "Porto failed on tile 3");
        expect(answer.enhanced).toEqual({ identity: "abcdef0123456789", width: 6000, height: 4000 });
    });

    it("holds nothing for a photograph that was closed before its run answered", async () => {
        stack(HOLIDAY.path, recovery());
        const view = mount();

        // Closing has already told every owner to forget this photograph. An answer landing after that
        // would put an entry back that nothing empties, since the close it belongs to is over.
        act(() => useFileStore.getState().closeFile(HOLIDAY.path));
        view.rerender(<Probe file={SUNSET} />);

        pending[0]?.settle(enhanced({ faces: [face(0)] }));
        await act(async () => {});

        expect(faces().size).toBe(0);
    });

    it("draws the run's own bar, the detection already on its head", async () => {
        // Rust maps the detection it makes into the first enhancement's slot of the bar, so the window
        // draws the report's own fraction and the bar moves forward once.
        stack(HOLIDAY.path, recovery());
        mount();

        deliver(progress("run-1", { family: "detection", operation: "New York (FP32)", chainFraction: 0.1 }));
        await waitFor(() => expect(answer.fraction).toBe(0.1));
        expect(answer.report?.family).toBe("detection");

        deliver(progress("run-1", { family: "face_recovery", operation: "Athens (FP32)", chainFraction: 0.6 }));
        await waitFor(() => expect(answer.fraction).toBe(0.6));
    });
});

describe("a mask painted over the photograph", () => {
    beforeEach(() => {
        useMaskStore.setState(useMaskStore.getInitialState(), true);
        vi.useFakeTimers();
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    /** One stroke, committed as the pointer coming up commits it. */
    const paint = (path: string, x = 0.4) =>
        act(() => {
            const { beginStroke, endStroke } = useMaskStore.getState();

            beginStroke(path, x, 0.5, false);
            endStroke(x, 0.5);
        });

    /** Lets a run land, and what is waiting on it run. */
    const land = async (index: number) => {
        pending[index]?.settle({ outcome: "enhanced", identity: "abcdef0123456789", width: 6000, height: 4000 });
        await act(async () => {});
    };

    /** Lets the time a mask change waits for another go by. */
    const wait = (ms = 300) => act(() => vi.advanceTimersByTime(ms));

    it("is sent with the run once a stroke is committed and a pause has gone by, and is re-run for each", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();
        expect(asked).toHaveBeenLastCalledWith(HOLIDAY.identity, [upscale(2)], "auto", undefined, undefined);
        await land(0);

        paint(HOLIDAY.path);
        expect(asked).toHaveBeenCalledTimes(1);
        wait();

        expect(asked).toHaveBeenCalledTimes(2);
        expect(stopped).toHaveBeenCalledWith("run-1");

        const [, , , , mask] = asked.mock.calls[1] as unknown[];
        expect(mask).toEqual({
            strokes: [{ erase: false, radius: 0.025, softness: 0.5, points: [0.4, 0.5] }],
            inverted: false,
        });

        paint(HOLIDAY.path, 0.6);
        wait();

        expect(asked).toHaveBeenCalledTimes(3);
    });

    it("costs one run for a flurry of strokes, from the pause after the last", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();
        await land(0);

        paint(HOLIDAY.path, 0.2);
        wait(200);
        paint(HOLIDAY.path, 0.4);
        wait(200);
        paint(HOLIDAY.path, 0.6);
        wait(299);
        expect(asked).toHaveBeenCalledTimes(1);

        wait(1);

        expect(asked).toHaveBeenCalledTimes(2);
        expect((asked.mock.calls[1] as unknown[])[4]).toMatchObject({ strokes: [{}, {}, {}] });
    });

    it("lets a run that is still computing its models finish, then runs once more with the latest mask", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        paint(HOLIDAY.path, 0.2);
        paint(HOLIDAY.path, 0.4);
        wait(1000);

        // Neither stroke stopped the first, uncached run, and neither started one of its own.
        expect(asked).toHaveBeenCalledTimes(1);
        expect(stopped).not.toHaveBeenCalled();

        await land(0);

        expect(asked).toHaveBeenCalledTimes(2);
        expect((asked.mock.calls[1] as unknown[])[4]).toMatchObject({ strokes: [{}, {}] });
        // The result that landed is on screen meanwhile, and nothing waits a further pause for the re-run.
        expect(answer.enhanced?.identity).toBe("abcdef0123456789");
    });

    it("does not re-run when the run that was computing had already been given the latest mask", async () => {
        stack(HOLIDAY.path, upscale(2));
        paint(HOLIDAY.path);
        mount();

        await land(0);
        wait(1000);

        expect(asked).toHaveBeenCalledTimes(1);
    });

    it("takes a stroke that arrives with a change of stack along with that run, rather than after it", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        act(() => {
            paint(HOLIDAY.path);
            stack(HOLIDAY.path, upscale(4));
        });
        wait(1000);

        expect(asked).toHaveBeenCalledTimes(2);
        expect((asked.mock.calls[1] as unknown[])[4]).toMatchObject({ strokes: [{}] });
        await land(1);
        expect(asked).toHaveBeenCalledTimes(2);
    });

    it("stops a cheap re-run that a newer stroke displaces, once the pause has gone by", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();
        await land(0);
        paint(HOLIDAY.path, 0.2);
        wait();
        expect(asked).toHaveBeenCalledTimes(2);

        // The re-run's models are cached, so it is the one a new stroke may displace.
        paint(HOLIDAY.path, 0.4);
        expect(stopped).toHaveBeenCalledTimes(1);
        wait();

        expect(asked).toHaveBeenCalledTimes(3);
        expect(stopped).toHaveBeenLastCalledWith("run-2");
    });

    it("does not start a run for a stroke that is still being drawn", () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        act(() => {
            const { beginStroke, extendStroke } = useMaskStore.getState();

            beginStroke(HOLIDAY.path, 0.1, 0.1, false);
            extendStroke(0.5, 0.5, 1.5);
            extendStroke(0.9, 0.9, 1.5);
        });
        wait(1000);

        expect(asked).toHaveBeenCalledTimes(1);
    });

    it("runs again for an undo and for an invert, and sends nothing once the mask paints nothing", async () => {
        stack(HOLIDAY.path, upscale(2));
        mount();
        await land(0);
        paint(HOLIDAY.path);
        wait();
        expect(asked).toHaveBeenCalledTimes(2);

        act(() => useMaskStore.getState().invert(HOLIDAY.path));
        wait();
        expect(asked).toHaveBeenCalledTimes(3);
        expect((asked.mock.calls[2] as unknown[])[4]).toMatchObject({ inverted: true });

        act(() => useMaskStore.getState().clear(HOLIDAY.path));
        wait();
        expect(asked).toHaveBeenCalledTimes(4);
        expect((asked.mock.calls[3] as unknown[])[4]).toBeUndefined();
    });

    it("runs again when a use is switched under a lens blur, sending only the one switched off", async () => {
        const blur: Operation = {
            family: "lens_blur",
            codename: "lisbon",
            precision: "fp16",
            parameters: { f_number: 1.4, focus_x: 0.5, focus_y: 0.5 },
        };
        stack(HOLIDAY.path, upscale(2), blur);
        mount();
        await land(0);
        paint(HOLIDAY.path);
        wait();
        expect(asked).toHaveBeenCalledTimes(2);
        expect((asked.mock.calls[1] as unknown[])[4]).not.toHaveProperty("keepsSharp");

        act(() => useMaskStore.getState().setUse(HOLIDAY.path, "keepsSharp", false));
        wait();
        expect(asked).toHaveBeenCalledTimes(3);
        expect((asked.mock.calls[2] as unknown[])[4]).toMatchObject({ keepsSharp: false });
        expect((asked.mock.calls[2] as unknown[])[4]).not.toHaveProperty("limitsEnhancements");

        // Switching it to what it already is changes nothing, and runs nothing.
        act(() => useMaskStore.getState().setUse(HOLIDAY.path, "keepsSharp", false));
        wait();
        expect(asked).toHaveBeenCalledTimes(3);
    });

    it("is another photograph's own: a stroke on one does not re-run the other", () => {
        stack(HOLIDAY.path, upscale(2));
        mount();

        paint(SUNSET.path);
        wait(1000);

        expect(asked).toHaveBeenCalledTimes(1);
    });
});

describe("an area painted for Remove", () => {
    beforeEach(() => {
        useMaskStore.setState(useMaskStore.getInitialState(), true);
        vi.useFakeTimers();
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    const remove: Operation = { family: "remove", codename: "lima", precision: "fp32", parameters: {} };

    /** One stroke on `path`'s removal, committed as the pointer coming up commits it. */
    const paint = (path: string, x = 0.4) =>
        act(() => {
            const { startEditing, beginStroke, endStroke } = useMaskStore.getState();

            startEditing(path, "removal");
            beginStroke(path, x, 0.5, false);
            endStroke(x, 0.5);
        });

    const chainOf = (call: number) => (asked.mock.calls[call] as unknown[])[1] as Operation[];
    const wait = (ms = 300) => act(() => vi.advanceTimersByTime(ms));

    it("is sent on the Remove before anything is painted, as a region with nothing in it", () => {
        stack(HOLIDAY.path, remove, upscale(2));
        mount();

        // Rust leaves the empty removal out of the chain; the limiting mask is still absent.
        expect(asked).toHaveBeenLastCalledWith(
            HOLIDAY.identity,
            [{ ...remove, mask: { strokes: [], inverted: false } }, upscale(2)],
            "auto",
            undefined,
            undefined,
        );
    });

    it("runs a stroke once a pause has gone by, stopping a run still computing, and leaves the stack without it", () => {
        stack(HOLIDAY.path, remove, upscale(2));
        mount();

        paint(HOLIDAY.path);
        wait(299);
        expect(asked).toHaveBeenCalledTimes(1);

        wait(1);

        // A new region is new work for the models, so the run in flight - computing the old one - is displaced.
        expect(asked).toHaveBeenCalledTimes(2);
        expect(stopped).toHaveBeenCalledWith("run-1");
        expect(chainOf(1)[0]?.mask).toEqual({
            strokes: [{ erase: false, radius: 0.025, softness: 0.5, points: [0.4, 0.5] }],
            inverted: false,
        });
        expect(chainOf(1)[1]).toEqual(upscale(2));
        expect((asked.mock.calls[1] as unknown[])[4]).toBeUndefined();
        expect(useEnhancementStore.getState().enhancements.get(HOLIDAY.path)?.[0]).toEqual(remove);
    });

    it("costs one run for a flurry of strokes", () => {
        stack(HOLIDAY.path, remove);
        mount();

        paint(HOLIDAY.path, 0.2);
        wait(200);
        paint(HOLIDAY.path, 0.4);
        wait(300);

        expect(asked).toHaveBeenCalledTimes(2);
        expect(chainOf(1)[0]?.mask?.strokes).toHaveLength(2);
    });

    it("runs a clear at once, with nothing left to compute", () => {
        stack(HOLIDAY.path, remove);
        mount();
        paint(HOLIDAY.path);
        wait();

        act(() => useMaskStore.getState().clear(HOLIDAY.path, "removal"));

        expect(asked).toHaveBeenCalledTimes(3);
        expect(chainOf(2)[0]?.mask).toEqual({ strokes: [], inverted: false });
    });

    it("is never sent with another photograph's pixels", () => {
        stack(HOLIDAY.path, remove);
        act(() =>
            useEnhancementStore.setState({
                enhancements: new Map([
                    [HOLIDAY.path, [remove]],
                    [SUNSET.path, [remove]],
                ]),
            }),
        );
        paint(HOLIDAY.path);
        const { rerender } = render(<Probe file={HOLIDAY} />);
        wait();

        rerender(<Probe file={SUNSET} />);

        // At once, with the second photograph's own (empty) region, not after a pause with the first one's.
        const last = asked.mock.calls.at(-1) as unknown[];
        expect(last[0]).toBe(SUNSET.identity);
        expect((last[1] as Operation[])[0]?.mask).toEqual({ strokes: [], inverted: false });
    });

    it("is independent of the limiting mask, which is still the run's own argument", async () => {
        stack(HOLIDAY.path, remove, upscale(2));
        mount();
        paint(HOLIDAY.path);
        wait();

        act(() => {
            const { startEditing, beginStroke, endStroke } = useMaskStore.getState();

            startEditing(HOLIDAY.path, "limit");
            beginStroke(HOLIDAY.path, 0.8, 0.5, false);
            endStroke(0.8, 0.5);
        });
        pending[1]?.settle({ outcome: "enhanced", identity: "abcdef0123456789", width: 6000, height: 4000 });
        await act(async () => {});
        wait();

        const last = asked.mock.calls.at(-1) as unknown[];
        expect((last[1] as Operation[])[0]?.mask?.strokes[0]?.points).toEqual([0.4, 0.5]);
        expect((last[4] as { strokes: { points: number[] }[] }).strokes[0]?.points).toEqual([0.8, 0.5]);
    });
});
