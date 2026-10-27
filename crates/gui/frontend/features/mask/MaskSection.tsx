import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useFileEnhancements } from "@/stores/enhancements";
import { useCurrentFile } from "@/stores/files";
import { useHasMask, useIsEditing, useMaskStore } from "@/stores/mask";
import { MaskPanel } from "./MaskPanel";

/**
 * The mask of the current photograph, under its enhancements: whether there is one, and the way to paint it.
 *
 * Reads "No mask" with **Paint mask**, or "Mask" with **Edit**, **Invert** and **Clear**; and while the preview is
 * painting, the panel of tools in their place. Not shown for a photograph with nothing in its stack - there is nothing
 * for a mask to limit - though what is painted is kept, and is there again when an enhancement is.
 *
 * **It also owns when painting stops**, being the one thing mounted for as long as a file is open: when the photograph
 * changes, and when the stack empties. Escape and Done stop it from the panel's own keys and button; a closed file
 * stops it through the store's own `registerFileOwner`.
 */
export const MaskSection = () => {
    const { t } = useTranslation();
    const file = useCurrentFile();
    const path = file?.path;
    const stack = useFileEnhancements(path);
    const painted = useHasMask(path);
    const editing = useIsEditing(path);
    const startEditing = useMaskStore((state) => state.startEditing);
    const invert = useMaskStore((state) => state.invert);
    const clear = useMaskStore((state) => state.clear);
    const stacked = stack.length > 0;

    useEffect(() => {
        const { editing: painting, stopEditing } = useMaskStore.getState();

        // Painting on another photograph than the one open, or on one with nothing left to limit: nowhere to paint.
        if (painting && (painting.path !== path || !stacked)) stopEditing();
    }, [path, stacked]);

    if (path === undefined || !stacked) return;

    return (
        <div className="flex flex-col gap-3 border-t border-border px-6 py-4" data-slot="mask-section">
            {editing ? (
                <MaskPanel path={path} />
            ) : (
                <>
                    <span className="text-[13px] font-semibold text-foreground">
                        {painted ? t("mask.section.some") : t("mask.section.none")}
                    </span>

                    {painted ? (
                        <div className="flex gap-1.5">
                            <Button
                                variant="secondary"
                                size="sm"
                                className="flex-1 font-normal"
                                onClick={() => startEditing(path)}
                            >
                                {t("mask.section.edit")}
                            </Button>
                            <Button
                                variant="secondary"
                                size="sm"
                                className="flex-1 font-normal"
                                onClick={() => invert(path)}
                            >
                                {t("mask.section.invert")}
                            </Button>
                            <Button
                                variant="secondary"
                                size="sm"
                                className="flex-1 font-normal"
                                onClick={() => clear(path)}
                            >
                                {t("mask.section.clear")}
                            </Button>
                        </div>
                    ) : (
                        <Button
                            variant="secondary"
                            size="sm"
                            className="font-normal"
                            onClick={() => startEditing(path)}
                        >
                            {t("mask.section.paint")}
                        </Button>
                    )}
                </>
            )}
        </div>
    );
};
