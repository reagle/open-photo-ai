import { registerFileOwner } from "@/stores/files";
import { call } from "./invoke";

// Written a second time in `crates/gui/src/photo_info.rs`, as the `#[tauri::command]` function's name. `photoInfo.test.ts`
// pins this half, and `generate_handler![]` the Rust one.
const PHOTO_INFO_COMMAND = "photo_info";

/**
 * A photograph as it was taken, as Rust's `PhotoInfo` serializes it: every field `null` (or `false`, or empty) where
 * the file does not say. **As shot**: what the stack makes of these numbers is `lib/photoInfo.ts`'s to work out.
 *
 * `located` says only whether the file records a place, never the place. `renderedFNumber` and `rendered` are what an
 * earlier Open Photo AI export recorded in its `opai:` XMP: the photograph is already enhanced, perhaps already blurred.
 */
export type PhotoInfo = {
    make: string | null;
    model: string | null;
    lensModel: string | null;
    focalMm: number | null;
    focal35mm: number | null;
    fNumber: number | null;
    exposureSeconds: number | null;
    iso: number | null;
    /** `YYYY-MM-DD HH:MM:SS`, as the camera's clock read. */
    taken: string | null;
    /** `+02:00`, the offset the camera recorded beside it. */
    takenOffset: string | null;
    width: number | null;
    height: number | null;
    colorProfile: string | null;
    hdr: "gainMap" | "pq" | "hlg" | "other" | null;
    located: boolean;
    renderedFNumber: number | null;
    rendered: string[];
};

/** Why the `photo_info` command rejected: `kind` is the `#[serde(tag = "kind")]` on Rust's `PhotoInfoError`. */
export type PhotoInfoError = { kind: "unknownSource"; identity: string } | { kind: "read"; message: string };

// Kept by identity until the photograph is closed, as `lensInfo` keeps its answers: an identity names one file's bytes,
// so what it says cannot change under it. A rejection is dropped so a failed question can be asked again.
const asked = new Map<string, Promise<PhotoInfo>>();

/**
 * What the file of the open photograph with identity `source` says about how it was taken. Instant: it reads the file
 * and decodes nothing. **Asked once per photograph** while it is open.
 */
export const photoInfo = (source: string): Promise<PhotoInfo> => {
    let answer = asked.get(source);

    if (!answer) {
        answer = call<PhotoInfo>(PHOTO_INFO_COMMAND, { source }).catch((error: unknown) => {
            asked.delete(source);

            throw error;
        });
        asked.set(source, answer);
    }

    return answer;
};

/** Forgets every answer, which is what closing every photograph leaves behind. */
export const forgetPhotoInfo = () => asked.clear();

registerFileOwner({
    forget: (_path, identity) => {
        if (identity !== undefined) asked.delete(identity);
    },
    forgetAll: forgetPhotoInfo,
});
