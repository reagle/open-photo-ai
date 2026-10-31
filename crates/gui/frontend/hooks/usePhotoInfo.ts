import { useEffect, useState } from "react";
import { type PhotoInfo, photoInfo } from "@/ipc/photoInfo";
import { report } from "@/lib/report";

/**
 * What the file of the photograph with identity `identity` says about how it was taken: `undefined` while the question
 * is out, for a photograph with no identity, and once it has failed - whoever reads it then simply draws nothing.
 *
 * **Asked once per photograph**, whoever asks: `photoInfo` keeps each answer while the photograph is open, so the
 * navbar's HDR badge and the information card share one read of the file.
 */
export const usePhotoInfo = (identity: string | undefined): PhotoInfo | undefined => {
    const [answered, setAnswered] = useState<{ identity: string; info: PhotoInfo }>();

    useEffect(() => {
        if (identity === undefined) return;

        let live = true;
        photoInfo(identity).then(
            (info) => live && setAnswered({ identity, info }),
            (error: unknown) => report("asking for the photograph's information failed", error),
        );

        return () => {
            live = false;
        };
    }, [identity]);

    // Only an answer about *this* photograph: the one held while the next is asked about belongs to the last.
    return identity !== undefined && answered?.identity === identity ? answered.info : undefined;
};
