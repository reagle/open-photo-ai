import { type RefObject, useEffect, useState } from "react";
import type { PhysicalPosition } from "@tauri-apps/api/dpi";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { isWindows } from "@/ipc/os";
import { openPaths } from "@/lib/openPaths";
import { report } from "@/lib/report";

/**
 * What a drop's reported position has to be divided by to be the CSS pixels the DOM measures in.
 *
 * Tauri types the position as a `PhysicalPosition` on every platform, and on **Windows alone** it
 * actually is one: wry takes it from `ScreenToClient`, whose client coordinates are device pixels.
 * macOS reads it from `draggingLocation()`, which AppKit gives in points, and GTK reports logical
 * units as well - and `tauri-runtime-wry` relabels the pair as physical either way without scaling
 * it. So the type is right about one platform in three, and dividing by `devicePixelRatio` on a
 * Retina Mac halves every coordinate, which quietly pulls the sidebar and the drawer into whatever
 * rectangle is being tested.
 *
 * Read from wry 0.55's three `drag_drop.rs` implementations rather than from the type. Verified on
 * macOS, which is the only platform this series builds on; the Windows branch is what its source
 * says and stays unverified, as R6 records.
 */
const positionScale = () => (isWindows() ? window.devicePixelRatio : 1);

/**
 * Whether the pointer was over `element` when the files were let go of.
 *
 * The position is relative to the webview and the rectangle is relative to the viewport, which are
 * the same origin: `titleBarStyle: "Overlay"` in `tauri.conf.json` puts the frontend under the title
 * bar rather than below it, so the webview fills the window. A platform that inset the webview inside
 * the window would need that offset subtracted here.
 *
 * The right and bottom edges are exclusive, as a rectangle's own `contains` would have it: a drop at
 * `rect.bottom` is the first pixel of whatever is drawn under the canvas, not the last of the canvas.
 */
const isOver = (element: HTMLElement, position: PhysicalPosition) => {
    const { x, y } = position.toLogical(positionScale());
    const rect = element.getBoundingClientRect();

    return x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom;
};

/**
 * Opens the images dropped on `target`, and names the files it will not open.
 *
 * **On `target`, not anywhere on the window.** Tauri delivers a drop as a window event carrying a
 * position rather than to an element, so the region that accepts one is a rule this hook enforces
 * itself: the position is converted into the DOM's own pixels and hit-tested against the element
 * the caller hands it. The caller hands it the region the preview draws its drop mark on - the surface
 * that says images can be dropped on it - so what the window offers and what it accepts are the same
 * rectangle, one element rather than two that could come to disagree. A drop over the navbar, the
 * sidebar or the drawer is ignored, exactly as the reference ignores it.
 *
 * **The drawer's strip is ignored whether the drawer is folded or not**, which is the question this
 * hook used to park here for a later slice. The preview canvas is inset by the folded drawer's header
 * permanently, and unfolding slides the body up *over* the canvas rather than resizing it - so the
 * canvas's own bottom edge sits inside an open drawer, and hit-testing against it would accept a drop
 * let go of on the thumbnails. The caller's answer is to hand over a region bottomed at the drawer's
 * top rather than the canvas's, so the thumbnails are outside the rectangle by construction. See
 * `Preview.tsx`, and design.md D7 of `add-gui-drop-beam`.
 *
 * **What is dropped is opened by `openPaths`**, the route files the platform asks the application to
 * open take too, so the two refuse, describe and count alike. Its comment says why the partition is
 * by extension and why the refusal is named here rather than in Rust.
 *
 * **It also reports whether a drag is currently over `target`**, which is the second answer the same
 * channel already holds - the returned boolean is the `isOver` call the drop path makes, evaluated
 * while the files are still in the air. A separate hook with its own `onDragDropEvent` would need
 * its own copy of `positionScale` and `isOver`, and two copies are how the region that is drawn and
 * the region that accepts a drop come to disagree. Opening dropped images is still what this hook is
 * for; the hover is a second return from one listener, not a second responsibility.
 */
export const useDroppedImages = (target: RefObject<HTMLElement | null>) => {
    const [over, setOver] = useState(false);

    useEffect(() => {
        const listening = getCurrentWebview().onDragDropEvent((event) => {
            const payload = event.payload;
            const element = target.current;

            // `leave` carries no position - it is the drag crossing back out of the window - so it
            // clears rather than being hit-tested.
            if (payload.type === "leave") {
                setOver(false);
                return;
            }

            // Recomputed from the position on both, never keyed on `enter` alone: `enter` is a
            // *window* event, fired once wherever the drag crossed in. A drag that arrives over the
            // navbar and slides down onto the canvas emits one `enter` outside it and then only
            // `over`, so an `enter`-only mark never lights up for it - and a mark that lit up on
            // `enter` regardless of position would light up for a drag that never reaches the
            // canvas. Recomputing makes both edges the same rule rather than two special cases.
            if (payload.type === "enter" || payload.type === "over") {
                setOver(element !== null && isOver(element, payload.position));
                return;
            }

            // Unconditionally, and before the hit-test below: the mark goes out when the files are
            // let go of, whether or not this window was the one that took them.
            setOver(false);

            if (payload.type !== "drop") return;

            // Before the extension list is even asked for: a drop outside the canvas is not this
            // window's to open, and nothing is told to the user about it - the file was let go of
            // over a region that never offered to take it.
            if (!element || !isOver(element, payload.position)) return;

            openPaths(payload.paths, "drop").catch((error: unknown) =>
                // Either the extension list or the describe, both of which mean the IPC bridge is
                // gone. Rust has written whatever it knows to the log, and `report` records what it
                // could not know.
                report("Failed to open the dropped files", error),
            );
        });

        return () => {
            // The listener is registered asynchronously, so the unregistration has to wait for it -
            // otherwise a component unmounted before the promise settles leaves one behind, and a
            // reloaded webview in development would accumulate a handler per reload.
            listening.then((unlisten) => unlisten());
        };
    }, [target]);

    return over;
};
