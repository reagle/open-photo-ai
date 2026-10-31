import { useEffect, useRef, useState } from "react";
import { type ExportLosses, exportLosses, type FormatChoice } from "@/ipc/export";
import { report } from "@/lib/report";

/** One question: the queue and the format choice it is about. */
type Question = { queue: readonly string[]; format: FormatChoice };

/**
 * What each queued file will lose in `format`, by path, as Rust predicts it from the file, with whether its source
 * declares an HDR gain map - `undefined` until the first answer arrives. A path with nothing to lose has an empty list.
 *
 * **Asked again only when the queue or the format choice changes**: what a file loses depends on the format it is
 * written as and nothing else the settings hold, and each question reads every queued file's header, which typing a
 * prefix must not repeat. **One question is in flight at a time**: one asked while another is still being answered
 * waits, and a later one replaces it, so stepping through the formats asks about the first and the last rather than
 * reading every queued file once per format passed. Only the latest question's answer is kept, as in
 * `useExportNames`; until it arrives the last answer stays.
 */
export const useExportLosses = (
    queue: readonly string[],
    format: FormatChoice,
): ReadonlyMap<string, ExportLosses> | undefined => {
    const [losses, setLosses] = useState<ReadonlyMap<string, ExportLosses>>();
    const asked = useRef(0);
    const busy = useRef(false);
    const waiting = useRef<Question | undefined>(undefined);

    useEffect(() => {
        asked.current += 1;

        const ask = ({ queue, format }: Question) => {
            busy.current = true;
            const question = asked.current;
            // The naming's prefix and suffix do not change what a file loses, and the folder neither; only the format
            // choice, which also decides whether a gain map is kept.
            exportLosses(queue, { prefix: "", suffix: "", format })
                .then(
                    (answer) => {
                        if (question !== asked.current) return;

                        setLosses(new Map(answer.map((predicted) => [predicted.path, predicted])));
                    },
                    // No warning is shown rather than a wrong one; each written row still says what it lost.
                    (error: unknown) => report("predicting the queued exports' losses failed", error),
                )
                .finally(() => {
                    busy.current = false;
                    const next = waiting.current;
                    waiting.current = undefined;
                    if (next) ask(next);
                });
        };

        if (busy.current) {
            waiting.current = { queue, format };
        } else {
            ask({ queue, format });
        }
    }, [queue, format]);

    // A question still waiting when the dialog closes is never asked.
    useEffect(
        () => () => {
            waiting.current = undefined;
        },
        [],
    );

    return losses;
};
