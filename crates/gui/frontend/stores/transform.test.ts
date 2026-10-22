import { act } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { ZOOM_BUTTON_STEP, ZOOM_MAX, ZOOM_MIN } from "@/lib/constants";
import { HOLIDAY, openFiles, resetFileStore, SUNSET } from "@/test/support";
import { FITTED, fitCurrent, stepZoom, useTransformStore } from "./transform";

const identity = (record: { identity?: string }) => record.identity ?? "";

const set = (record: { identity?: string }, transform: Parameters<typeof store.setTransform>[1]) =>
    act(() => store.setTransform(identity(record), transform));

const store = useTransformStore.getState();
const transforms = () => useTransformStore.getState().transforms;

beforeEach(() => {
    resetFileStore();
    useTransformStore.setState(useTransformStore.getInitialState(), true);
});

describe("the transform store", () => {
    it("keeps one photograph's view separate from another's", () => {
        set(HOLIDAY, { scale: 4, x: -100, y: -50 });
        set(SUNSET, { scale: 2, x: -10, y: -20 });

        expect(transforms().get(identity(HOLIDAY))).toEqual({ scale: 4, x: -100, y: -50 });
        expect(transforms().get(identity(SUNSET))).toEqual({ scale: 2, x: -10, y: -20 });
    });

    it("replaces the map on every write rather than mutating it", () => {
        const before = transforms();

        set(HOLIDAY, { scale: 2, x: 0, y: 0 });

        // The standing rule, argued in `setTransform`.
        expect(transforms()).not.toBe(before);
        expect(before.size).toBe(0);
    });

    it("holds the scale inside the range whatever it is handed", () => {
        set(HOLIDAY, { scale: 0.25, x: 0, y: 0 });
        expect(transforms().get(identity(HOLIDAY))?.scale).toBe(ZOOM_MIN);

        set(HOLIDAY, { scale: 40, x: 0, y: 0 });
        expect(transforms().get(identity(HOLIDAY))?.scale).toBe(ZOOM_MAX);
    });

    it("keeps the anchor a writer handed it, and leaves it out when none was", () => {
        set(HOLIDAY, { scale: 2, x: 0, y: 0, anchor: { x: 0.25, y: 0.75 } });
        expect(transforms().get(identity(HOLIDAY))?.anchor).toEqual({ x: 0.25, y: 0.75 });

        set(HOLIDAY, { scale: 3, x: 0, y: 0 });
        expect(transforms().get(identity(HOLIDAY))).not.toHaveProperty("anchor");
    });
});

describe("stepZoom and fitCurrent", () => {
    const current = () => transforms().get(identity(HOLIDAY));

    it("steps the current photograph by the drawer buttons' step, keeping its position and no anchor", () => {
        act(() => openFiles(HOLIDAY, SUNSET));
        set(HOLIDAY, { scale: 2, x: -40, y: -30, anchor: { x: 0.1, y: 0.9 } });

        act(() => stepZoom(1));
        expect(current()).toEqual({ scale: 2 + ZOOM_BUTTON_STEP, x: -40, y: -30 });

        act(() => stepZoom(-1));
        expect(current()).toEqual({ scale: 2, x: -40, y: -30 });

        // Only the current photograph moves.
        expect(transforms().has(identity(SUNSET))).toBe(false);
    });

    it("starts a never-magnified photograph from fitted", () => {
        act(() => openFiles(HOLIDAY));

        act(() => stepZoom(1));

        expect(current()).toEqual({ scale: ZOOM_MIN + ZOOM_BUTTON_STEP, x: 0, y: 0 });
    });

    it("holds at either end of the range", () => {
        act(() => openFiles(HOLIDAY));

        act(() => stepZoom(-1));
        expect(current()?.scale).toBe(ZOOM_MIN);

        set(HOLIDAY, { scale: ZOOM_MAX, x: 0, y: 0 });
        act(() => stepZoom(1));
        expect(current()?.scale).toBe(ZOOM_MAX);
    });

    it("fits a magnified, panned photograph whole and centred", () => {
        act(() => openFiles(HOLIDAY));
        set(HOLIDAY, { scale: 5, x: -300, y: -120 });

        act(() => fitCurrent());

        expect(current()).toEqual(FITTED);
    });

    it("does nothing with no photograph open", () => {
        const before = transforms();

        act(() => stepZoom(1));
        act(() => fitCurrent());

        expect(transforms()).toBe(before);
        expect(transforms().size).toBe(0);
    });

    it("does nothing for a current file whose bytes could not be read", () => {
        const { identity: _unread, ...unreadable } = HOLIDAY;
        act(() => openFiles(unreadable));

        act(() => stepZoom(1));
        act(() => fitCurrent());

        expect(transforms().size).toBe(0);
    });
});
