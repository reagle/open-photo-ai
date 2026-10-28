import { toast } from "sonner";
import i18n from "@/i18n";
import type { ImageRecord } from "@/ipc/images";
import { checkRecent, noteRecent, onRecentChanged, type RecentImage, recentImages } from "@/ipc/recent";
import { openPaths } from "@/lib/openPaths";
import { fileName } from "@/lib/paths";
import { report } from "@/lib/report";
import { useFileStore } from "@/stores/files";
import { useRecentStore } from "@/stores/recent";

/** What Rust is told of a photograph that entered the gallery. */
const recentOf = ({ path, identity }: ImageRecord): RecentImage =>
    identity === undefined ? { path } : { path, identity };

/** Hands Rust what entered the gallery, reporting and dropping a rejection: only the list is stale. */
const note = (images: RecentImage[]) => {
    noteRecent(images).catch((error: unknown) => report("Failed to note the images opened", error));
};

// **What enters the gallery, rather than what each route opens**, so the picker, a drop, Open With, `--gui` and Open
// Recent itself count alike, and a route added later counts without being told to. A batch that adds nothing new -
// the same folder opened again - changes no paths, so it notes nothing.
//
// In the store's own order, which is the batch's: the first of a batch is the one the window makes current, so it is
// the one listed newest.
/**
 * Keeps the Open Recent list: tells Rust which photographs enter the gallery, and keeps `stores/recent.ts` in step
 * with the list Rust publishes, from now and on every change.
 *
 * Returns the unsubscribe.
 */
export const watchRecent = (): (() => void) => {
    recentImages()
        .then((paths) => useRecentStore.getState().setPaths(paths))
        // A browser without Tauri underneath it - `vite` opened directly - rejects every call.
        .catch((error: unknown) => report("Failed to read the recent images", error));

    const listening = onRecentChanged((paths) => useRecentStore.getState().setPaths(paths));

    const stop = useFileStore.subscribe((state, previous) => {
        if (state.files === previous.files) return;

        const before = new Set(previous.files.map((file) => file.path));
        const entered = state.files.filter((file) => !before.has(file.path));
        if (entered.length > 0) note(entered.map(recentOf));
    });

    return () => {
        stop();
        listening
            .then((unlisten) => unlisten())
            .catch((error: unknown) => report("Failed to stop listening to the recent images", error));
    };
};

/**
 * Opens the recent photograph at `path`, from the macOS menu or the Windows and Linux button.
 *
 * ```
 *   open already       -> made current, and noted again so it moves to the top
 *   no longer there    -> a notice; Rust has dropped it from the list
 *   otherwise          -> opened as a drop is (`openPaths`), which makes it current
 * ```
 *
 * Rejects when the IPC bridge is gone.
 */
export const openRecent = async (path: string) => {
    const { files, setCurrentIndex } = useFileStore.getState();
    const index = files.findIndex((file) => file.path === path);
    const open = files[index];

    if (open) {
        setCurrentIndex(index);
        note([recentOf(open)]);
        return;
    }

    if (!(await checkRecent(path))) {
        toast.warning(i18n.t("toasts.recentMissing", { name: fileName(path) }));
        return;
    }

    await openPaths([path], "recent");
};
