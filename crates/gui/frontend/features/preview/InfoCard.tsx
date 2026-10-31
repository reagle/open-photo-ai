import { MapPin } from "lucide-react";
import { useTranslation } from "react-i18next";
import { usePhotoInfo } from "@/hooks/usePhotoInfo";
import { infoLines } from "@/lib/photoInfo";
import { useImageCrop } from "@/stores/crop";
import { useFileEnhancements } from "@/stores/enhancements";
import { useCurrentFile } from "@/stores/files";
import { useInfoCardStore } from "@/stores/infoCard";

/**
 * The card of what the current photograph was taken with, at the canvas's bottom left: camera and lens, the exposure,
 * when, the pixels, and the edits the stack applies - each, where an edit changes a number the file states, as shot
 * then as rendered (`f/8 → f/1.4 simulated`). Toggled by `I`; hidden until then, and remembered.
 *
 * See-through and `pointer-events-none`, as the focus hint is: it is read over the photograph, and must not take the
 * wheel-zoom and the drag-to-pan from the canvas under it. `bottom` is the drawer's top, which it would otherwise sit
 * under.
 */
export const InfoCard = ({ bottom }: { bottom: number }) => {
    const { t } = useTranslation();
    const visible = useInfoCardStore((state) => state.visible);
    const file = useCurrentFile();
    const crop = useImageCrop(file?.identity);
    const stack = useFileEnhancements(file?.path);
    const info = usePhotoInfo(visible ? file?.identity : undefined);

    if (!visible || !info) return null;

    const lines = infoLines(t, info, stack, crop);
    const rows = [lines.camera, lines.lens, lines.exposure, lines.taken, lines.pixels].filter(
        (line): line is string => line !== undefined,
    );

    return (
        <section
            data-slot="info-card"
            aria-label={t("preview.info.title")}
            className="pointer-events-none absolute left-3 z-[15] max-w-[min(30rem,calc(100%-1.5rem))] rounded-xl border border-border/60 bg-background/65 px-3 py-2 text-xs leading-relaxed text-foreground shadow-sm backdrop-blur-sm"
            style={{ bottom: bottom + 12 }}
        >
            {rows.map((row) => (
                <p key={row} className="truncate">
                    {row}
                </p>
            ))}
            {lines.edits !== undefined && (
                <p className="truncate text-muted-foreground">
                    {t("preview.info.edits")}: {lines.edits}
                </p>
            )}
            {lines.earlier !== undefined && <p className="truncate text-muted-foreground">{lines.earlier}</p>}
            {lines.located && (
                <p className="flex items-center gap-1 text-muted-foreground">
                    <MapPin aria-hidden="true" className="size-3" />
                    {t("preview.info.located")}
                </p>
            )}
        </section>
    );
};
