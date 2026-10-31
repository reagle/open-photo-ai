import { useEffect, useMemo, useState } from "react";
import type { ImageRecord } from "@/ipc/images";
import { type Badge, badgeOf, batchRows, progressOf, unprocessedCount } from "@/lib/batch";
import { useBatchStore } from "@/stores/batch";
import { useCropStore } from "@/stores/crop";
import { useEnhancementStore } from "@/stores/enhancements";
import { useFacesStore } from "@/stores/faces";
import { useFileStore } from "@/stores/files";
import { useMaskStore } from "@/stores/mask";
import { useSettingsStore } from "@/stores/settings";

/** Every store a photograph's request, and so its place in the batch, is read from. */
const READ = [
    useBatchStore,
    useFileStore,
    useEnhancementStore,
    useCropStore,
    useFacesStore,
    useMaskStore,
    useSettingsStore,
];

/**
 * `look()`, kept current: answered again whenever a store it reads changes, and re-rendering only when the answer
 * does. Subscribed rather than rendered from because the answer is a word or a few counts, and a stroke on one
 * photograph's mask then costs each thumbnail one comparison.
 */
const useLooked = <T>(look: () => T, same: (a: T, b: T) => boolean = Object.is) => {
    const [value, setValue] = useState(look);

    // `same` is the caller's constant; `look` stands for the photograph or the counts it reads.
    // biome-ignore lint/correctness/useExhaustiveDependencies: see above
    useEffect(() => {
        const refresh = () => setValue((previous) => (same(previous, look()) ? previous : look()));
        const unsubscribes = READ.map((store) => (store.subscribe as (listener: () => void) => () => void)(refresh));

        refresh();

        return () => {
            for (const unsubscribe of unsubscribes) unsubscribe();
        };
    }, [look]);

    return value;
};

/** The strip's badge for `file`, as {@link badgeOf} answers it. */
export const useBatchBadge = (file: ImageRecord): Badge | undefined => {
    const look = useMemo(() => () => badgeOf(file), [file]);

    return useLooked(look);
};

type BatchCounts = ReturnType<typeof progressOf> & { unprocessed: number };

const counts = (): BatchCounts => ({ ...progressOf(), unprocessed: unprocessedCount() });
const sameCounts = (a: BatchCounts, b: BatchCounts) =>
    a.done === b.done && a.failed === b.failed && a.left === b.left && a.unprocessed === b.unprocessed;

/** How far the batch has got, and how many open photographs a "Process all" would go through. */
export const useBatchCounts = () => useLooked(counts, sameCounts);

const sameRows = (a: ReturnType<typeof batchRows>, b: ReturnType<typeof batchRows>) =>
    a.length === b.length &&
    a.every((row, at) => row.file === b[at]?.file && row.index === b[at]?.index && row.state === b[at]?.state);

/** The batch's photographs in the strip's order with where each stands, as {@link batchRows} answers them. */
export const useBatchRows = () => useLooked(batchRows, sameRows);
