import { useEffect, useState } from "react";
import type { CropInfo } from "@/ipc/crop";
import { type ImageRecord, renditionFor } from "@/ipc/images";
import { screenBound, THUMBNAIL_BOUND } from "@/lib/constants";
import { logTiming } from "@/lib/timing";

/** The three renditions the canvas steps through, smallest first. */
export type Stage = "thumbnail" | "screen" | "full";

/** One photograph's renditions, by stage, at one framing. */
const renditionsOf = (file: ImageRecord | undefined, crop: CropInfo | undefined) => ({
    // The strip's own URL where nothing is framed, so the stand-in is the bitmap the webview has already decoded for
    // the thumbnail; a framed photograph needs its framing's, which is a small request of its own.
    thumbnail: renditionFor(file, THUMBNAIL_BOUND, crop),
    screen: renditionFor(file, screenBound(), crop),
    full: renditionFor(file, 0, crop),
});

/** Decodes `url`, resolving either way: a rendition that will not decode is drawn as broken, not waited for. */
const decoded = (url: string) => {
    const preload = new Image();
    preload.src = url;

    // `decode()` resolves once the bitmap can be painted. Absent in jsdom and in an old webview: there the next stage is
    // drawn at once, and the browser paints it when it can.
    if (typeof preload.decode !== "function") return Promise.resolve();

    return preload.decode().catch(() => {});
};

/**
 * The URL the canvas draws `file` from: **the strip's thumbnail at once**, then a rendition the size of the screen, and
 * the photograph at its own size only once it is magnified - each drawn when it has decoded, never one smaller than
 * the last.
 *
 * What it replaces held the whole canvas - the previous photograph *and its enhanced result* - on screen until the new
 * photograph's full-resolution original had been decoded, re-encoded as a JPEG and decoded again by the webview. On a
 * machine busy with a lens blur that was seconds in which a click seemed to do nothing, and the run for the new
 * photograph was not even asked for until it ended. Now the record changes on the click, so the run is asked for at
 * once and the previous result is never drawn over the new photograph: a result belongs to the identity it was made
 * from, and that identity is no longer on screen.
 *
 * **The box is the record's, not the bitmap's**, so a 384-pixel stand-in is drawn at the photograph's fitted size and
 * shape, and a sharper stage replacing it moves nothing. That is what made the wait necessary before - a bitmap of one
 * photograph in the box of another - and it no longer arises: every stage is this photograph, at this framing. A file
 * whose header gave no dimensions has no box until its pixels arrive, so it is drawn from its full rendition as before.
 *
 * Each stage's decode, timed from the moment the photograph changed, is written to the log at `info`: the click to the
 * first frame is the number this exists for.
 */
export const useStandIn = (
    file: ImageRecord | undefined,
    crop: CropInfo | undefined,
    magnified: boolean,
): string | undefined => {
    const renditions = renditionsOf(file, crop);
    const known = file?.width !== undefined && file.height !== undefined;
    const wanted: Stage = magnified || !known ? "full" : "screen";

    // Keyed by the photograph's framed full URL, which names both the identity and the crop: what was decoded for
    // another photograph, or another framing, is never drawn for this one.
    const key = renditions.full;
    const first: Stage = known ? "thumbnail" : "full";
    const [shown, setShown] = useState<{ key: string | undefined; stage: Stage }>({ key: undefined, stage: first });
    const fresh = shown.key !== key;
    const stage: Stage = fresh ? first : shown.stage;

    // `stage` and `fresh` are read for where to start, not listened to: the loop is what moves them.
    // biome-ignore lint/correctness/useExhaustiveDependencies: see above
    useEffect(() => {
        if (key === undefined) return;

        const asked = performance.now();
        const name = file?.path.split(/[\\/]/).at(-1) ?? "";
        let live = true;

        const stages: Stage[] = wanted === "full" ? ["thumbnail", "screen", "full"] : ["thumbnail", "screen"];
        const at = stages.indexOf(stage);

        (async () => {
            // For a photograph just shown, the stage already drawn is decoded too, so its time is logged as what the
            // click cost; it is in the webview's memory and settles at once. A zoom starts from the stage after.
            for (const step of stages.slice(Math.max(fresh ? at : at + 1, 0))) {
                const url = renditions[step];
                if (url === undefined) return;

                await decoded(url);
                if (!live) return;

                logTiming(`photograph drawn: ${step} of ${name} after ${Math.round(performance.now() - asked)} ms`);
                setShown((previous) =>
                    previous.key === key && stages.indexOf(previous.stage) >= stages.indexOf(step)
                        ? previous
                        : { key, stage: step },
                );
            }
        })();

        return () => {
            live = false;
        };
    }, [key, wanted]);

    return renditions[stage];
};
