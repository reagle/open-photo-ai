import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { usePhotoInfo } from "@/hooks/usePhotoInfo";
import type { ImageRecord } from "@/ipc/images";
import type { PhotoInfo } from "@/ipc/photoInfo";

/**
 * What the badge's tooltip says the file's HDR is, and what an export does with it - which is what
 * `opai::export::loss::assess` decides: a gain map is kept only by a JPEG written with it, and PQ and
 * HLG are not written as HDR by any format yet. Worded in step with the export dialog's loss warnings,
 * which say the same thing about one format at a time.
 */
export const hdrTip = (t: TFunction, hdr: NonNullable<PhotoInfo["hdr"]>): string => {
    switch (hdr) {
        case "gainMap":
            return t("navbar.hdr.gainMap");
        case "pq":
        case "hlg":
            return t("navbar.hdr.transfer", { transfer: hdr.toUpperCase() });
        default:
            return t("navbar.hdr.other");
    }
};

/**
 * A small "HDR" beside the dimensions where the current photograph's file declares HDR - an Ultra HDR,
 * ISO 21496-1 or Apple gain map, or a PQ or HLG transfer - with what an export keeps of it on hover.
 *
 * **The source, as shot.** What became of a gain map in the result is not something the window is told:
 * every enhancement but a lens blur and a removal leaves it as it is, those two rewrite it, and the one
 * case that drops it - a rewrite that failed - is warned of on the export's row ("it could not be
 * carried"). So the badge answers the file, as the information card's line does, and is absent with
 * nothing to say, while the question is out, and for a file that could not be read.
 */
export const NavbarHdr = ({ file }: { file: ImageRecord }) => {
    const { t } = useTranslation();
    const hdr = usePhotoInfo(file.identity)?.hdr;

    if (!hdr) return;

    return (
        <Tooltip>
            <TooltipTrigger asChild>
                {/*
                 * Focusable, so a keyboard reaches the tooltip as a pointer does; a `span` rather than a
                 * button, because there is nothing to press. The chip is the settings' "Recommended" one in
                 * shape, but outlined and muted: it is a fact about the file, not a call to look at it.
                 */}
                <span
                    // biome-ignore lint/a11y/noNoninteractiveTabindex: the tooltip is the badge's whole meaning, and focus is how a keyboard opens it.
                    tabIndex={0}
                    className="flex h-5 flex-none items-center rounded-full border border-border px-1.75 font-medium text-[11px] text-muted-foreground"
                    data-slot="navbar-hdr"
                >
                    {t("navbar.hdr.label")}
                </span>
            </TooltipTrigger>

            <TooltipContent side="bottom" sideOffset={8} className="max-w-72">
                {hdrTip(t, hdr)}
            </TooltipContent>
        </Tooltip>
    );
};
