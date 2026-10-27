import { useEffect, useRef, useState } from "react";
import { type CropInfo, cropKey } from "@/ipc/crop";
import { type LensInfo, lensInfo } from "@/ipc/lens";
import { report } from "@/lib/report";

/**
 * The lens of the photograph with identity `identity` as it is framed by `crop`: `undefined` while the question is out -
 * and for a photograph with no identity, which has no pixels for anything to be asked about - and `null` once it has
 * failed.
 *
 * **Per photograph and framing, for as long as the photograph is open**: `lensInfo` keeps each answer, so a panel opened
 * again, or opened over another photograph and back, does not ask again. An answer landing after the question changed is
 * dropped rather than set, as `useOnce` drops one for an unmounted consumer. A failure is reported once, and the
 * caller falls back as the library does for a photograph it knows nothing of.
 */
export const useLensInfo = (identity: string | undefined, crop?: CropInfo): LensInfo | null | undefined => {
    const [answered, setAnswered] = useState<{ asked: string; info: LensInfo | null }>();

    // The question by value, so a framing written again with the same fields is not a new one. The framing itself is
    // read through a ref for the same reason: the effect is keyed on what was asked, not on the object that asked it.
    const asked = `${identity}|${cropKey(crop)}`;
    const framing = useRef(crop);
    framing.current = crop;

    useEffect(() => {
        if (identity === undefined) return;

        let live = true;

        lensInfo(identity, framing.current).then(
            (info) => live && setAnswered({ asked, info }),
            (error: unknown) => {
                report("asking for the photograph's lens failed", error);
                if (live) setAnswered({ asked, info: null });
            },
        );

        return () => {
            live = false;
        };
    }, [identity, asked]);

    // Only an answer to *this* question: the one held while a new question is out belongs to the previous one.
    return identity !== undefined && answered?.asked === asked ? answered.info : undefined;
};
