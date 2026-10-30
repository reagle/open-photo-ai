import { log } from "@/ipc/log";

/**
 * Writes one of the window's own timings to the log at `info`: what only the window can measure, such as how long a
 * click took to put the photograph on screen.
 *
 * To the file alone, never to Faro: a timing is not a failure. Dropped if the bridge is gone, as a failed record is.
 */
export const logTiming = (message: string) => {
    Promise.resolve()
        .then(() => log({ level: "info", message }))
        .catch(() => {});
};
