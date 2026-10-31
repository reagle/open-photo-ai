import { type ReactNode, useEffect, useRef } from "react";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { PopoverClose, PopoverContent } from "@/components/ui/popover";
import { ADD_MENU_GAP, ENHANCEMENT_ROW_HEIGHT } from "@/lib/constants";
import { acceptOnReturn, BLOCKING_LAYER, keepsLiveSurface, LIVE_SURFACE } from "@/lib/layers";

// The design puts the panel 16px clear of the sidebar's leading edge (screen 11), which is the same gap
// the add menu keeps - and unlike that menu, this one needs no correction for the panel's own padding:
// a row spans the sidebar's full width, so its box *is* the sidebar's edge and `sideOffset`, which
// Radix measures from the trigger, is the gap itself.
/** How far the panel sits from the row it opens from. */
const PANEL_GAP = ADD_MENU_GAP;

// `align="start"` puts the two top edges together and this walks the panel down. Well inside the
// row's own height, which is what Radix clamps a cross-axis offset at - floating-ui's `limitShift`
// refuses to let a panel float free of the thing that anchors it.
/** How far down the row the panel's top edge starts: the row's vertical centre. */
const PANEL_ALIGN = ENHANCEMENT_ROW_HEIGHT / 2;

/**
 * Return as Done, for as long as the panel is up - this is mounted inside its content, which Radix mounts only then.
 *
 * On the window rather than on the panel, because the focus is often not in it: a press on the preview to pan, or on
 * the drawer's zoom, leaves the panel open and takes the focus with it. Not while a dialog, a menu or a listbox is
 * open over it (a model tray's list has its own Return), and not where Return operates the focused control.
 */
const DoneOnReturn = ({ onDone }: { onDone: () => void }) => {
    const done = useRef(onDone);
    done.current = onDone;

    useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            if (document.querySelector(BLOCKING_LAYER)) return;

            acceptOnReturn(event, () => done.current());
        };

        window.addEventListener("keydown", onKeyDown);

        return () => window.removeEventListener("keydown", onKeyDown);
    }, []);

    return null;
};

type OptionsPanelProps = {
    /** The enhancement's own name, which is what the panel is titled with. */
    title: string;
    /** That family's controls. */
    children: ReactNode;
    /** Closes the panel: Return's Done, as Escape and the close box are Radix's own. */
    onDone: () => void;
};

/**
 * The frame an enhancement's options open in: a titled panel anchored to its row.
 *
 * **A live surface** (`lib/layers.ts`): every change it makes is made as it is made, so Escape and Return both close
 * it, keeping them, and the photograph stays operable beside it - the view's keys pass through (`LIVE_SURFACE`), and a
 * press on the preview or the drawer's zoom leaves it open (`KEEPS_LIVE_SURFACE`), so the user can zoom and pan to see
 * what a slider did. A press anywhere else - the sidebar, the toolbar - closes it, as Radix's outside press does.
 */
export const OptionsPanel = ({ title, children, onDone }: OptionsPanelProps) => {
    // One frame for every family, which is what keeps each family's panel from rediscovering the
    // anchoring: every family differs in what it puts *inside* this, and in nothing else. The
    // reference reaches the same conclusion with its own shared `OptionsPopover`.
    const { t } = useTranslation();

    return (
        <PopoverContent
            side="left"
            align="start"
            sideOffset={PANEL_GAP}
            alignOffset={PANEL_ALIGN}
            // `p-0` over the generated primitive's `p-4`, because this panel is not uniformly padded:
            // the design draws a 40px header flush to the panel's edges with a rule under it, and the
            // body padded inside that. The 296px width and the 10px radius are the design's too, where
            // the primitive is generated at 288px and the application's own 8px.
            className="w-[296px] overflow-hidden rounded-[10px] p-0"
            data-slot="enhancement-options"
            {...{ [LIVE_SURFACE]: "" }}
            // Radix asks this for an outside press and for the focus moving outside alike: the drawer's zoom buttons
            // take the focus as they are clicked.
            onInteractOutside={(event) => {
                if (keepsLiveSurface(event.target)) event.preventDefault();
            }}
        >
            <DoneOnReturn onDone={onDone} />

            <div className="flex h-10 items-center justify-between border-b border-border pr-1.5 pl-3">
                <span className="font-medium text-muted-foreground text-xs">{title}</span>

                <PopoverClose
                    className="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-hidden"
                    aria-label={t("common.close")}
                >
                    {/* 1.75 rather than the application-wide 1.5, as every close box in this design is. */}
                    <X className="size-4" strokeWidth={1.75} />
                </PopoverClose>
            </div>

            <div className="flex flex-col gap-4 px-3 py-3.5">{children}</div>
        </PopoverContent>
    );
};
