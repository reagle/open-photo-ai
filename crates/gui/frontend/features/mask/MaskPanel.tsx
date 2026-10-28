import type { MouseEvent, ReactNode } from "react";
import { Brush, Contrast, Eraser, Redo2, Trash2, Undo2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { isEmpty, SLIDER_MAX, sizeToSlider, sliderToSize, withModifier } from "@/lib/mask";
import { toPercent } from "@/lib/utils";
import { type Target, useFileMask, useMaskStore } from "@/stores/mask";
import { useMaskKeys } from "./useMaskKeys";

/** A brush's size as the panel reads it: a percent of the longer side, to a tenth below one percent. */
const sizeText = (size: number) => `${size < 0.01 ? (size * 100).toFixed(1) : toPercent(size)}%`;

/**
 * An icon button with its name for assistive technology and, in a tooltip, its name and its key.
 *
 * The key is untranslated: `[`, `Space` and `Option` are what is printed on the keyboard.
 */
const Tool = ({
    label,
    shortcut,
    disabled,
    onClick,
    children,
}: {
    label: string;
    shortcut?: string;
    disabled: boolean;
    onClick: () => void;
    children: ReactNode;
}) => {
    const { t } = useTranslation();

    return (
        <Tooltip>
            <TooltipTrigger asChild>
                <Button
                    variant="secondary"
                    size="icon-sm"
                    aria-label={label}
                    disabled={disabled}
                    onClick={onClick}
                    className="flex-1"
                >
                    {children}
                </Button>
            </TooltipTrigger>

            <TooltipContent>
                {shortcut ? t("mask.panel.shortcut", { action: label, key: shortcut }) : label}
            </TooltipContent>
        </Tooltip>
    );
};

/** A slider with its name and its value over it. */
const Setting = ({
    label,
    text,
    position,
    step,
    onChange,
}: {
    label: string;
    text: string;
    position: number;
    step: number;
    onChange: (position: number) => void;
}) => (
    <div className="flex flex-col gap-2.5">
        <div className="flex items-center justify-between">
            <span className="text-[13px] text-foreground">{label}</span>
            <span className="font-mono text-[13px] text-muted-foreground">{text}</span>
        </div>

        <Slider
            min={0}
            max={SLIDER_MAX}
            step={step}
            value={[position]}
            thumbLabel={label}
            onValueChange={([next]) => next !== undefined && onChange(next)}
        />
    </div>
);

/**
 * The tools of editing a mask: paint or erase, the brush's size and softness, undo and redo, invert and clear, the
 * tint, and the way out.
 *
 * **One panel for both of a photograph's regions**, the limiting mask and the area Remove takes out (`target`): the
 * same brush, the same keys, the same undo. The removal's is titled "Paint area to remove", says that a stroke is
 * removed when it ends, and has no Invert - a removal turned over would take out everything but what was painted.
 *
 * Drawn in the sidebar, in the place of the section's buttons, and not in a popover: the user's attention is on the
 * preview, and a popover would close on the first click in it. It is mounted exactly while the preview is painting on
 * `path`, which is also what keeps its keys - see {@link useMaskKeys} - alive only that long.
 *
 * **A control the mouse has pressed lets go of the focus.** A focused button is pressed again by Space, and Space is
 * how the photograph is panned while painting. Keyboard activation (a click with no pointer behind it) keeps its focus.
 */
export const MaskPanel = ({ path, target = "limit" }: { path: string; target?: Target }) => {
    const { t } = useTranslation();
    useMaskKeys(path, target);

    const tool = useMaskStore((state) => state.tool);
    const entry = useFileMask(path, target);
    const removal = target === "removal";
    const title = removal ? t("mask.removal.title") : t("mask.panel.title");
    const { setMode, setSize, setSoftness, setOverlay, undo, redo, invert, clear, stopEditing } =
        useMaskStore.getState();

    const painted = !isEmpty(entry);

    const releaseFocus = (event: MouseEvent<HTMLFieldSetElement>) => {
        if (event.detail === 0 || !(event.target instanceof HTMLElement)) return;
        if (event.target.closest("button, [role='switch'], [role='radio']"))
            (document.activeElement as HTMLElement | null)?.blur();
    };

    return (
        // A click bubbling here is a click on one of the controls, which is all this contains.
        // biome-ignore lint/a11y/useKeyWithClickEvents: not a control; it only hands the focus back
        <fieldset
            aria-label={title}
            onClick={releaseFocus}
            className="m-0 flex min-w-0 flex-col gap-4 border-0 p-0"
            data-slot="mask-panel"
            data-target={target}
        >
            {/* Named on screen for the removal, which opens on adding Remove and has to say what it is for. */}
            {removal && (
                <span className="flex items-center gap-2 text-[13px] font-semibold text-foreground">
                    <span aria-hidden className="size-2.5 flex-none rounded-full bg-removal" />
                    {title}
                </span>
            )}

            <ToggleGroup
                type="single"
                variant="outline"
                value={tool.mode}
                // Pressing the one that is on turns it off, which is no tool: the brush is always one or the other.
                onValueChange={(mode) => (mode === "paint" || mode === "erase") && setMode(mode)}
                aria-label={t("mask.panel.tool")}
                className="w-full"
            >
                <ToggleGroupItem
                    value="paint"
                    className="flex-1"
                    title={t("mask.panel.shortcut", { action: t("mask.panel.paint"), key: "B" })}
                >
                    <Brush />
                    {t("mask.panel.paint")}
                </ToggleGroupItem>
                <ToggleGroupItem
                    value="erase"
                    className="flex-1"
                    title={t("mask.panel.shortcut", { action: t("mask.panel.erase"), key: "E" })}
                >
                    <Eraser />
                    {t("mask.panel.erase")}
                </ToggleGroupItem>
            </ToggleGroup>

            <Setting
                label={t("mask.panel.size")}
                text={sizeText(tool.size)}
                position={sizeToSlider(tool.size)}
                step={1}
                onChange={(position) => setSize(sliderToSize(position))}
            />

            <Setting
                label={t("mask.panel.softness")}
                text={`${toPercent(tool.softness)}%`}
                position={tool.softness * 100}
                step={5}
                onChange={(position) => setSoftness(position / 100)}
            />

            <div className="flex gap-1.5">
                <Tool
                    label={t("mask.panel.undo")}
                    shortcut={withModifier("Z")}
                    disabled={(entry?.undo.length ?? 0) === 0}
                    onClick={() => undo(path, target)}
                >
                    <Undo2 />
                </Tool>
                <Tool
                    label={t("mask.panel.redo")}
                    shortcut={withModifier("Z", true)}
                    disabled={(entry?.redo.length ?? 0) === 0}
                    onClick={() => redo(path, target)}
                >
                    <Redo2 />
                </Tool>
                {/* Nothing to turn over until something is painted; and nothing a removal can turn over at all. */}
                {!removal && (
                    <Tool label={t("mask.panel.invert")} disabled={!painted} onClick={() => invert(path)}>
                        <Contrast />
                    </Tool>
                )}
                <Tool label={t("mask.panel.clear")} disabled={!painted} onClick={() => clear(path, target)}>
                    <Trash2 />
                </Tool>
            </div>

            <label htmlFor="mask-overlay" className="flex items-center justify-between">
                <span className="text-[13px] text-foreground">{t("mask.panel.overlay")}</span>
                <Switch
                    id="mask-overlay"
                    checked={tool.overlay}
                    onCheckedChange={setOverlay}
                    title={t("mask.panel.shortcut", { action: t("mask.panel.overlay"), key: "O" })}
                    className="data-[state=unchecked]:bg-input"
                />
            </label>

            <p className="text-xs leading-relaxed text-muted-foreground text-pretty">
                {removal ? t("mask.removal.hint") : t("mask.panel.hint")}
            </p>

            <Button
                onClick={stopEditing}
                title={t("mask.panel.shortcut", { action: t("mask.panel.done"), key: "Esc" })}
                className="h-9 w-full font-normal"
            >
                {t("mask.panel.done")}
            </Button>
        </fieldset>
    );
};
