import { useEffect } from "react";
import { onFilesOpened, takeOpenedFiles } from "@/ipc/images";
import { openPaths } from "@/lib/openPaths";
import { report } from "@/lib/report";
import { useSetupStore } from "@/stores/setup";

/** Takes what the platform asked the application to open and opens it, as a drop is opened. */
const drain = () =>
    takeOpenedFiles()
        .then((paths) => (paths.length > 0 ? openPaths(paths, "open_with") : undefined))
        .catch((error: unknown) =>
            // The IPC bridge is gone. Rust has written whatever it knows to the log, and `report`
            // records what it could not know.
            report("Failed to open the files the platform asked to open", error),
        );

/**
 * Opens the files the platform asks the application to open - Open With, a drop on its icon, or a
 * file named on its command line - at launch and while it runs.
 *
 * Nothing is taken until setup is `ready`: before then nothing in the window can use an image, and
 * the files wait in Rust's inbox rather than in a component. From then on the inbox is drained once,
 * and again whenever Rust says it has something in it.
 *
 * **The first drain waits for the listener**, so nothing can fall between the two: a file that
 * arrives before the listener is registered is in the inbox when the first drain takes it, and one
 * that arrives after is announced to a listener that hears it. Every file is handed over once, so a
 * drain that finds the inbox already empty - StrictMode's second mount, or an event for files an
 * earlier drain took - opens nothing.
 */
export const useOpenedFiles = () => {
    const ready = useSetupStore((state) => state.status === "ready");

    useEffect(() => {
        if (!ready) return;

        const listening = onFilesOpened(drain);
        listening.then(drain, (error: unknown) => {
            report("Failed to listen for files the platform asks to open", error);
            drain();
        });

        return () => {
            // Registered asynchronously, so the unregistration has to wait for it, as in
            // `useDroppedImages`.
            listening.then((unlisten) => unlisten()).catch(() => {});
        };
    }, [ready]);
};
