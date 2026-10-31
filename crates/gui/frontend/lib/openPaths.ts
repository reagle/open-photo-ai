import { toast } from "sonner";
import i18n from "@/i18n";
import { describeImages, type ImageRecord, inputExtensions } from "@/ipc/images";
import { formatsOf } from "@/lib/events";
import { track } from "@/lib/faro";
import { extensionOf, fileName } from "@/lib/paths";
import { useBatchStore } from "@/stores/batch";
import { useFileStore } from "@/stores/files";

/** How paths from outside the window reached it: a drop on the canvas, the platform asking (Open With), or Open Recent. */
export type PathSource = "drop" | "open_with" | "recent";

/**
 * Opens the images among `paths`, and names the files it will not open.
 *
 * The one route for paths that reach the window from outside it - a drop, and files the platform asks
 * the application to open - so both refuse, describe and count exactly alike.
 *
 * **The partition is by extension, as the reference partitions it**, against the list the decoder
 * itself reports. Judging a file by reading it instead would mean hashing every byte of whatever was
 * dropped - a video, an archive, a folder - before concluding it is not a photograph, and would
 * report a corrupt but genuinely supported photograph as a format the application does not open.
 *
 * What is refused is named to the user here rather than in Rust, which is also where the reference
 * puts it and for the reason its comment gives: the notice needs a plural form that only the i18n
 * catalogue gets right in languages with more than two. Refusing a file never costs the user the rest
 * of the batch - a folder of photographs with a stray document in it is a batch they assembled.
 *
 * Resolves with the records admitted. Rejects when the extension list or the describe does, both of which mean the
 * IPC bridge is gone.
 */
export const openPaths = async (paths: string[], source: PathSource): Promise<ImageRecord[]> => {
    const extensions = await inputExtensions();

    // One pass rather than two complementary filters: `extensionOf` allocates and the
    // `includes` is linear over the whole list, so a folder drop of several hundred paths
    // would evaluate both twice for every one of them to reach the same two arrays.
    const supported: string[] = [];
    const refused: string[] = [];

    for (const path of paths) {
        (extensions.includes(extensionOf(path)) ? supported : refused).push(path);
    }

    // Ahead of the describe, as the reference emits it ahead of the load: the notice is about
    // what was dropped, and it should not wait on however long hashing the rest takes.
    if (refused.length > 0) {
        toast.warning(
            i18n.t("toasts.unsupportedFiles", { count: refused.length, names: refused.map(fileName).join(", ") }),
        );
        // Which formats people want that this application does not open. Their types only.
        track("files_refused", { count: refused.length, formats: formatsOf(refused) });
    }

    // Not "describe nothing and add nothing", which would be the same answer at the cost of a
    // round trip: a batch of one document is a batch this application has nothing to open.
    if (supported.length === 0) return [];

    const admitted = await describeImages(supported);
    // While a batch pages through the photographs the view is the batch's: what is added joins the strip, and the
    // photograph on screen stays the one being worked on.
    useFileStore.getState().addFiles(admitted, { keepCurrent: useBatchStore.getState().phase === "running" });

    if (admitted.length > 0) {
        track("files_added", { count: admitted.length, source, formats: formatsOf(admitted.map((file) => file.path)) });
    }

    return admitted;
};
