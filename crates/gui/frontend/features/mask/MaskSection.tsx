import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { isPainted } from "@/lib/enhancements";
import { useFileEnhancements } from "@/stores/enhancements";
import { useCurrentFile } from "@/stores/files";
import { useEditingTarget, useHasMask, useMaskStore } from "@/stores/mask";
import { MaskPanel } from "./MaskPanel";

/**
 * The mask of the current photograph, under its enhancements: whether there is one, and the way to paint it.
 *
 * Reads "No mask" with **Paint mask**, or "Mask" with **Edit**, **Invert** and **Clear**; and while the preview is
 * painting, the panel of tools in their place. Not shown for a photograph with nothing in its stack - there is nothing
 * for a mask to limit - though what is painted is kept, and is there again when an enhancement is.
 *
 * **The removal's brush is drawn here too**, in the same place: while the area Remove takes out is being painted, the
 * panel for it - titled "Paint area to remove", with no Invert - stands in for the whole section. One region is edited
 * at a time, and the limiting mask's buttons come back when Done is pressed.
 *
 * **So is the Lens Blur's Refine brush**, opened from its panel: while it is being painted its panel - titled "Refine
 * lens blur", with Focus, Blur and Erase for its tools - stands in for the section likewise, and Done brings the
 * buttons back.
 *
 * **A mask limits the enhancements that are not painted**: a stack holding only Remove has nothing for one to limit -
 * the limiting mask is blended in after the removals and never brings back what was removed - so the section is not
 * offered for it.
 *
 * **It also owns when painting stops**, being the one thing mounted for as long as a file is open: when the photograph
 * changes, and when the stack no longer holds what the region being painted is for. Escape and Done stop it from the
 * panel's own keys and button; a closed file stops it through the store's own `registerFileOwner`, and taking Remove
 * off the stack through `forgetTarget`.
 */
export const MaskSection = () => {
    const { t } = useTranslation();
    const file = useCurrentFile();
    const path = file?.path;
    const stack = useFileEnhancements(path);
    const painted = useHasMask(path);
    const editing = useEditingTarget(path);
    const startEditing = useMaskStore((state) => state.startEditing);
    const invert = useMaskStore((state) => state.invert);
    const clear = useMaskStore((state) => state.clear);
    const stacked = stack.some((operation) => !isPainted(operation.family));
    const removing = stack.some((operation) => isPainted(operation.family));
    const blurring = stack.some((operation) => operation.family === "lens_blur");

    useEffect(() => {
        const { editing: painting, stopEditing } = useMaskStore.getState();
        if (!painting) return;

        // Painting on another photograph than the one open, or a region the stack has nothing left for: nowhere to
        // paint.
        const wanted = { limit: stacked, removal: removing, refine: blurring }[painting.target];
        if (painting.path !== path || !wanted) stopEditing();
    }, [path, stacked, removing, blurring]);

    if (path === undefined) return;

    if (editing === "removal" && removing)
        return (
            <div className="flex flex-col gap-3 border-t border-border px-6 py-4" data-slot="mask-section">
                <MaskPanel path={path} target="removal" />
            </div>
        );

    if (editing === "refine" && blurring)
        return (
            <div className="flex flex-col gap-3 border-t border-border px-6 py-4" data-slot="mask-section">
                <MaskPanel path={path} target="refine" />
            </div>
        );

    if (!stacked) return;

    return (
        <div className="flex flex-col gap-3 border-t border-border px-6 py-4" data-slot="mask-section">
            {editing === "limit" ? (
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
