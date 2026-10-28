import { Plus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { HintTooltip } from "@/components/HintTooltip";
import { Button } from "@/components/ui/button";
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuGroup,
    DropdownMenuItem,
    DropdownMenuLabel,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { familyEntry, useCatalogue } from "@/hooks/useCatalogue";
import { useShortcutHint } from "@/hooks/useShortcuts";
import type { Operation } from "@/ipc/enhance";
import { ADD_MENU_GAP, SIDEBAR_PADDING } from "@/lib/constants";
import {
    applyOrder,
    ENHANCEMENT_GROUPS,
    ENHANCEMENTS,
    isPainted,
    newOperation,
    startingFNumber,
    startingScale,
    suspendedBecause,
} from "@/lib/enhancements";
import { track } from "@/lib/faro";
import { useAddEnhancementMenuStore } from "@/stores/addEnhancementMenu";
import { useImageCrop } from "@/stores/crop";
import { useEnhancementStore, useFileEnhancements } from "@/stores/enhancements";
import { useCurrentFile } from "@/stores/files";
import { useMaskStore } from "@/stores/mask";
import { useSettingsStore } from "@/stores/settings";

// Not the design's own 16px. The design puts the menu 16px clear of the **sidebar's** leading edge
// (screen 10). `sideOffset` is measured from the *trigger's* box, and the trigger is the full-width
// button inside a panel padded by `SIDEBAR_PADDING` - so one gap's worth of offset would put the menu
// that far from the button and still under the panel's padding. The sum is the distance from the
// button to where the design draws the menu's trailing edge, and it is written as a sum so the
// dependency is visible.
//
// It comes to the reference's own number: its MUI menu anchors to the button's leading edge and then
// shifts itself `-2.5rem`, which is this 40px.
/** How far the menu sits from the control that opens it. */
const MENU_OFFSET = SIDEBAR_PADDING + ADD_MENU_GAP;

/** A heading of the menu: smaller and quieter than the entries it names, so it reads as a label, not a choice. */
const HEADING = "px-2.5 pt-1.5 pb-0.5 text-[11px] font-medium tracking-wide text-muted-foreground uppercase";

/** How far down the trigger the menu's top edge starts: its vertical centre, as the design draws it. */
const MENU_ALIGN = 20;

/**
 * The Add enhancement button, and the menu of what this application can run.
 *
 * **An enhancement already in the stack is drawn unavailable rather than removed.**
 *
 * The button is unavailable with no image open, because there is nothing for an enhancement to be
 * added to. Its key (`E` by default) opens the same menu, with the focus on its first entry so the
 * arrows and Enter pick one - see `useAddEnhancementMenuStore`.
 */
export const AddEnhancement = () => {
    const { t } = useTranslation();
    const open = useAddEnhancementMenuStore((state) => state.open);
    const setOpen = useAddEnhancementMenuStore((state) => state.setOpen);
    const hint = useShortcutHint("addEnhancement", t("enhancements.add"));

    const file = useCurrentFile();
    const families = useCatalogue();
    const models = useSettingsStore((state) => state.models);
    const stack = useFileEnhancements(file?.path);
    const crop = useImageCrop(file?.identity);
    const addEnhancement = useEnhancementStore((state) => state.addEnhancement);

    const add = async (family: Operation["family"]) => {
        setOpen(false);

        if (!file) return;

        // Asked of the backend, for the photograph as it is framed now: the ladder is `opai`'s. Only an
        // upscale reads it, so nothing else waits on the bridge.
        const scale = family === "upscale" ? await startingScale(file, crop) : undefined;
        // Likewise a lens blur's aperture, which starts where it adds no blur: the photograph's own, as it is framed.
        const fNumber = family === "lens_blur" ? await startingFNumber(file, crop) : undefined;

        /*
         * Built here rather than in the store, because what a *new* enhancement carries is a
         * question about the user's settings and the photograph on screen - the stored default model
         * for that family, and a scale suited to the image - and neither is something a store of
         * per-file stacks knows. `newOperation` answers nothing for a family the catalogue publishes
         * no model for, which is the render before the catalogue has arrived: the press adds
         * nothing rather than adding an operation naming no model.
         */
        const operation = newOperation(family, familyEntry(families, family), models[family], scale, fNumber);

        if (!operation) return;

        addEnhancement(file.path, operation, applyOrder(families));
        track("enhancement_added", { family, source: "manual" });

        // A painted enhancement does nothing until something is painted, so adding it is asking for the brush.
        if (isPainted(family)) useMaskStore.getState().startEditing(file.path, "removal");
    };

    return (
        // Open only with an image to add to: the key cannot open it without one, and closing the image
        // while it is open takes it down rather than leaving it hanging off a disabled button.
        <DropdownMenu open={open && file !== undefined} onOpenChange={setOpen} modal={false}>
            <HintTooltip tip={hint.tooltip}>
                <DropdownMenuTrigger asChild>
                    <Button
                        variant="outline"
                        disabled={!file}
                        aria-keyshortcuts={hint.ariaKeyShortcuts}
                        className="h-10 w-full"
                        data-slot="add-enhancement"
                    >
                        <Plus className="size-4.5" />
                        {t("enhancements.add")}
                    </Button>
                </DropdownMenuTrigger>
            </HintTooltip>

            {/*
             * `side="left"` with the menu's top edge at the button's vertical centre, which is the
             * reference's anchoring (`anchorOrigin` centre/left against `transformOrigin` top/right)
             * and what the design draws. `align="start"` puts the two top edges together and
             * `alignOffset` walks the menu down half the button; 20 is well inside the 40px the
             * trigger's own height allows, which is what Radix clamps a cross-axis offset at.
             */}
            <DropdownMenuContent
                side="left"
                align="start"
                sideOffset={MENU_OFFSET}
                alignOffset={MENU_ALIGN}
                className="w-[230px] p-1"
                data-slot="add-enhancement-menu"
            >
                {/*
                 * Under headings that name each run of the chain's order (see `ENHANCEMENT_GROUPS`), so the
                 * order reads as the sequence it is rather than as arbitrary. The headings are labels, not
                 * entries: the arrows step over them.
                 */}
                {ENHANCEMENT_GROUPS.map(({ group, labelKey }, index) => (
                    <DropdownMenuGroup key={group}>
                        {index > 0 && <DropdownMenuSeparator />}
                        <DropdownMenuLabel className={HEADING}>{t(labelKey)}</DropdownMenuLabel>
                        {ENHANCEMENTS.filter((entry) => entry.group === group).map(
                            ({ family, nameKey, icon: Icon }) => (
                                <DropdownMenuItem
                                    key={family}
                                    // The design's 44px row with its 20px glyph, where the generated item is a
                                    // 32px row with a 16px one: this menu names enhancements rather than actions.
                                    className="h-11 gap-3 px-2.5 text-[13px] [&_svg]:size-5 [&_svg]:text-foreground"
                                    // Unavailable rather than removed: adding Upscale twice would put two upscales
                                    // in one chain, and a menu that changed shape as a stack was built up would
                                    // move the entries under the pointer.
                                    // And unavailable while it would be held out of the chain on arrival: an
                                    // upscale, while the restore enlarges.
                                    disabled={
                                        stack.some((operation) => operation.family === family) ||
                                        suspendedBecause(stack, family) !== undefined
                                    }
                                    onSelect={() => void add(family)}
                                >
                                    <Icon />
                                    {t(nameKey)}
                                </DropdownMenuItem>
                            ),
                        )}
                    </DropdownMenuGroup>
                ))}
            </DropdownMenuContent>
        </DropdownMenu>
    );
};
