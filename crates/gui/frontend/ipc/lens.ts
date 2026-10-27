import { registerFileOwner } from "@/stores/files";
import { type CropInfo, cropKey } from "./crop";
import { call } from "./invoke";

// Written a second time in `crates/gui/src/lens.rs`, as the `#[tauri::command]` function's name. A rename on one
// side alone is not a type error but an `invoke` that rejects at runtime: `lens.test.ts` pins this half, and
// `generate_handler![]` the Rust one by refusing to compile against a function that does not exist.
const LENS_INFO_COMMAND = "lens_info";

/**
 * Where a photograph's own lens numbers came from, as Rust's `LensOrigin` spells it.
 *
 * `lens_info` answers `exif` or `assumed` alone: `estimated` is the depth model's refinement of a run, which this
 * command never waits for.
 */
export type LensOrigin = "exif" | "estimated" | "assumed";

/**
 * The lens a photograph was taken with, in 35 mm full-frame-equivalent terms, as Rust's `LensInfo` serializes it.
 *
 * `ownFNumber` is the f-number the photograph already has - **as it is framed**, so a crop of it is wider-numbered
 * than the whole frame - and where the Lens Blur slider ends: an f-number at or above it adds no blur. `focal35mm` is
 * absent (`null` on the wire) where only the depth model could say.
 *
 * `cropFactor` is the photograph's **as it is framed** - a crop of it has a larger one - where the EXIF gives one to
 * measure, and `null` where it does not. The real f-number, which is what the lens's barrel says and the EXIF records,
 * is the equivalent one over it; with none, no real f-number is claimed.
 */
export type LensInfo = { ownFNumber: number; focal35mm: number | null; cropFactor: number | null; origin: LensOrigin };

/**
 * Why the `lens_info` command rejected.
 *
 * `kind` is the `#[serde(tag = "kind")]` on Rust's `LensError`. A photograph with no EXIF is not among these: it
 * answers the assumed f/8.
 */
export type LensError = { kind: "unknownSource"; identity: string } | { kind: "read"; message: string };

// Kept by identity and framing until the photograph is closed, because an identity names one file's bytes and so its
// EXIF cannot change under it, and the framing is the one other thing the answer is of. The promise is kept, as `once`
// keeps one, so the panel and the add menu asking together are one `invoke`; a rejection is dropped so a failed question
// can be asked again.
const asked = new Map<string, Promise<LensInfo>>();

/**
 * The lens of the open photograph with identity `source`, as it is framed by `crop` (the whole of it where absent),
 * from its EXIF alone: instant, and it never runs the depth model.
 *
 * **Asked once per photograph and framing**, while the photograph is open. Rejects with the serialized
 * {@link LensError}, typed as `unknown` because that is what an `invoke` rejection is.
 */
export const lensInfo = (source: string, crop?: CropInfo): Promise<LensInfo> => {
    const key = `${source}|${cropKey(crop)}`;
    let answer = asked.get(key);

    if (!answer) {
        // `crop` is sent as `undefined` rather than omitted, as `enhance` sends it.
        answer = call<LensInfo>(LENS_INFO_COMMAND, { source, crop }).catch((error: unknown) => {
            asked.delete(key);

            throw error;
        });
        asked.set(key, answer);
    }

    return answer;
};

/** Forgets every answer, which is what closing every photograph leaves behind. */
export const forgetLensInfo = () => asked.clear();

/** Forgets the answers for each framing of the photograph with identity `identity`, which is what closing it means. */
const forgetLensInfoOf = (identity: string) => {
    for (const key of asked.keys()) {
        if (key.startsWith(`${identity}|`)) asked.delete(key);
    }
};

// A per-file owner keyed by identity, as the crop store's is: an answer for every framing a photograph has been looked
// at in would otherwise outlive it, one entry per crop. A photograph with no identity was never asked about.
registerFileOwner({
    forget: (_path, identity) => {
        if (identity !== undefined) forgetLensInfoOf(identity);
    },
    forgetAll: forgetLensInfo,
});
