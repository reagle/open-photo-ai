import { useEffect, useState } from "react";
import type { ParseKeys } from "i18next";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { HintTooltip } from "@/components/HintTooltip";
import { Button } from "@/components/ui/button";
import { Popover, PopoverTrigger } from "@/components/ui/popover";
import { SelectFacesDialog } from "@/features/faces/SelectFacesDialog";
import { useLensInfo } from "@/hooks/useLensInfo";
import { useShortcutHint } from "@/hooks/useShortcuts";
import type { FamilyEntry } from "@/ipc/catalogue";
import type { Operation } from "@/ipc/enhance";
import { detectFaces } from "@/ipc/faces";
import { ASSUMED_LENS, shownAmount } from "@/lib/blurAmount";
import { type Enhancement, modelValue, optionFor, parameterOf, qualityLabel } from "@/lib/enhancements";
import { enabledFaces } from "@/lib/faces";
import { report } from "@/lib/report";
import type { ShortcutAction } from "@/lib/shortcuts";
import { cn, toPercent } from "@/lib/utils";
import { useImageCrop } from "@/stores/crop";
import { useFaceChoice, useFacesStore, useImageFaces } from "@/stores/faces";
import { useHasMask, useIsEditing, useMaskStore } from "@/stores/mask";
import { useOptionsPanelStore } from "@/stores/optionsPanel";
import { useSettingsStore } from "@/stores/settings";
import { ColorBalanceOptions } from "./ColorBalanceOptions";
import { ColorizationOptions } from "./ColorizationOptions";
import { DenoiseOptions } from "./DenoiseOptions";
import { FaceRecoveryOptions } from "./FaceRecoveryOptions";
import { LensBlurOptions } from "./LensBlurOptions";
import { LightAdjustmentOptions } from "./LightAdjustmentOptions";
import { OptionsPanel } from "./OptionsPanel";
import { RestoreOptions } from "./RestoreOptions";
import { SharpenOptions } from "./SharpenOptions";
import { UpscaleOptions } from "./UpscaleOptions";

type EnhancementRowProps = {
    /** The enhancement this row is of, which is what its icon and its name come from. */
    enhancement: Enhancement;
    /** What that enhancement is set to do, which is what the line beneath the name reports. */
    operation: Operation;
    /** What the catalogue publishes for this family, or `undefined` before it has arrived. */
    entry: FamilyEntry | undefined;
    /**
     * The photograph this row's enhancement is set to run over, which is what its faces are keyed by.
     *
     * Absent for a file whose bytes could not be read - which has no pixels to detect in, so its row
     * reports no count and can only ever report none.
     */
    identity?: string;
    /**
     * Why this enhancement is held out of the chain while it stays on the stack, as the key of the sentence that
     * says so - an upscale while the restore enlarges - or `undefined` where it runs. See `suspendedBecause`.
     */
    suspended?: ParseKeys | undefined;
    /**
     * This row's place in the list, counting from 1, which is what the digit keys open the options by. Absent
     * for a row drawn outside the list, which then names no key.
     */
    place?: number;
    /** The photograph's path, which a removal's strokes are kept by. */
    path: string;
    /** Takes this enhancement off the image's stack. */
    onRemove: () => void;
    /** Puts a changed version of this enhancement back on the stack, which re-runs the preview. */
    onChange: (operation: Operation) => void;
};

/** The actions that open a row's options, by the row's place in the list: `1` to `8`. */
const OPEN_OPTIONS: readonly ShortcutAction[] = [
    "openOptions1",
    "openOptions2",
    "openOptions3",
    "openOptions4",
    "openOptions5",
    "openOptions6",
    "openOptions7",
    "openOptions8",
];

// The reference's own `toFixed(3)`, and it is there to hide float noise rather than to round: a
// value typed as 1.5 and carried through a parse can arrive as 1.4999999999999998, and three places
// is past anything the control can produce. `parseFloat` then drops the zeros a whole number
// gains, so a doubling reads `2x` rather than `2.000x`.
/** How many decimal places a scale is reported to. */
const SCALE_PLACES = 3;

/**
 * One enhancement on the current image's stack: what it is, what it is set to do, and the way to
 * take it off.
 *
 * **The row itself opens the enhancement's options**: the whole row is the control, and the remove
 * button sits over its trailing edge rather than beside it.
 *
 * **The remove control is revealed on hover and on focus.**
 */
export const EnhancementRow = ({
    enhancement,
    operation,
    entry,
    identity,
    suspended,
    place,
    path,
    onRemove,
    onChange,
}: EnhancementRowProps) => {
    const { t } = useTranslation();
    const Icon = enhancement.icon;

    /*
     * A removal's region, which is all a Remove row reports and all its control opens: whether anything is painted,
     * and whether the brush is on it now.
     */
    const removalPainted = useHasMask(path, "removal");
    const removalEditing = useIsEditing(path, "removal");

    /*
     * The faces found in this photograph as it is framed, which is what a face-recovery row counts.
     * The framing is read here rather than passed in for the reason the canvas reads it: it is a
     * store keyed by the same identity, and threading it through the list would be a second copy of
     * the same lookup.
     *
     * `undefined` means *not known yet*, which is a different thing from an empty array, and the two
     * have to read differently - see `info`.
     */
    const crop = useImageCrop(identity);
    const faces = useImageFaces(identity, crop);
    // Only a lens blur's row asks: the amount it reads is against the photograph's own lens, as it is framed.
    const lens = useLensInfo(operation.family === "lens_blur" ? identity : undefined, crop);

    /*
     * The choice made among them, which is what turns `2 Faces` into `1/2 Faces` and what the button in
     * the options panel counts down from - each face's own default, `restorable`, where the user said
     * nothing. Not compared against the framing, unlike the faces: a choice names faces by key, so one
     * recorded at another framing matches nothing.
     */
    const choice = useFaceChoice(identity);
    const chosen = enabledFaces(faces ?? [], choice).length;

    /*
     * Whether the Select faces dialog is up, held here rather than in `FaceRecoveryOptions` so the
     * dialog's lifetime is the row's rather than the panel's. Radix dismisses a popover when a modal
     * takes focus outside its content, so a dialog mounted inside the panel would unmount itself
     * mid-open along with the panel that opened it.
     */
    const [choosingFaces, setChoosingFaces] = useState(false);

    /*
     * The panel is closed deliberately as the dialog opens, rather than left to Radix: the design
     * draws one thing on screen at a time, not a panel peeking out from behind a modal.
     */
    //
    // A store's rather than this row's, so the window's keys (`1` to `8`) can open a row's panel by its
    // place in the list.
    const panelOpen = useOptionsPanelStore((state) => state.family === operation.family);
    const setPanelOpen = (open: boolean) => {
        const { openPanel, closePanel } = useOptionsPanelStore.getState();

        if (open) openPanel(operation.family);
        else closePanel(operation.family);
    };

    // Each control's key, from the user's bindings: the row's is its place in the list (`1` to `8`), and the
    // remove button's is Delete - `⌫` on a Mac - which acts on the row with the focus.
    const rowHint = useShortcutHint(place === undefined ? undefined : OPEN_OPTIONS[place - 1], t(enhancement.nameKey));
    const removeHint = useShortcutHint("removeEnhancement", t("enhancements.remove"));

    /*
     * **The picker's own detection**: a face recovery's options opened over a photograph whose faces are
     * not known at this framing yet ask `detect_faces` for them, so Select faces has something to offer
     * without waiting for the chain. The chain finds the same faces for itself inside its own run - one
     * detection, `Detection::for_face_recovery`, served from `opai`'s run store to whichever asks second -
     * and records them when it answers; this is only for the panel a person is looking at now.
     *
     * Written only while the photograph is still open, as every late answer is: a photograph that was
     * closed has had every owner told to forget it. A failure is reported and nothing more - the chain's
     * own answer says whether the faces could be found, and it is the one that tells the user.
     */
    const lookingForFaces = panelOpen && operation.family === "face_recovery" && faces === undefined;

    useEffect(() => {
        if (!lookingForFaces || !identity) return;

        const { processor } = useSettingsStore.getState();

        detectFaces(identity, processor, crop).done.then(
            (found) => useFacesStore.getState().recordFaces(identity, crop, found),
            (error: unknown) => report("detecting the faces for the picker failed", error),
        );
    }, [lookingForFaces, identity, crop]);

    /*
     * What the model is called and which quality it is being run at, read off the catalogue rather
     * than composed from the codename: title-casing `saopaulo` gives "Saopaulo", and the label is
     * already published beside it.
     */
    const option = optionFor(entry, modelValue(operation));

    /*
     * A model the catalogue no longer publishes - or one described before the catalogue arrived -
     * is reported by its own codename and its own precision. That is the same class of drift
     * `lib/enhancements.ts` already answers for a family the catalogue stops publishing, and the
     * same answer: say something honest rather than throw.
     */
    const name = option?.label ?? operation.codename;
    const quality = qualityLabel(t, option?.quality) ?? operation.precision;

    /*
     * Which controls the panel holds, chosen against the operation's own family for the reason the
     * info line is: every family offers something different inside one shared frame, and the reference
     * keeps the same record of family to options component.
     */
    const options = () => {
        switch (operation.family) {
            // Painted, not set: the row opens the brush instead of a panel - see the trigger below.
            case "remove":
                return null;
            case "denoise":
                return (
                    <DenoiseOptions enhancement={enhancement} operation={operation} entry={entry} onChange={onChange} />
                );
            case "face_recovery":
                return (
                    <FaceRecoveryOptions
                        enhancement={enhancement}
                        operation={operation}
                        entry={entry}
                        found={faces?.length ?? 0}
                        chosen={chosen}
                        onSelectFaces={() => {
                            setPanelOpen(false);
                            setChoosingFaces(true);
                        }}
                        onChange={onChange}
                    />
                );
            case "light_adjustment":
                return (
                    <LightAdjustmentOptions
                        enhancement={enhancement}
                        operation={operation}
                        entry={entry}
                        onChange={onChange}
                    />
                );
            case "color_balance":
                return (
                    <ColorBalanceOptions
                        enhancement={enhancement}
                        operation={operation}
                        entry={entry}
                        onChange={onChange}
                    />
                );
            case "sharpen":
                return (
                    <SharpenOptions enhancement={enhancement} operation={operation} entry={entry} onChange={onChange} />
                );
            case "colorization":
                return (
                    <ColorizationOptions
                        enhancement={enhancement}
                        operation={operation}
                        entry={entry}
                        onChange={onChange}
                    />
                );
            case "upscale":
                return (
                    <UpscaleOptions enhancement={enhancement} operation={operation} entry={entry} onChange={onChange} />
                );
            case "restore":
                return (
                    <RestoreOptions enhancement={enhancement} operation={operation} entry={entry} onChange={onChange} />
                );
            case "lens_blur":
                return (
                    <LensBlurOptions
                        enhancement={enhancement}
                        operation={operation}
                        entry={entry}
                        {...(identity !== undefined && { identity })}
                        onChange={onChange}
                    />
                );
        }
    };

    // Zero where the amount is known to nobody - the operation carries none and the catalogue has not
    // arrived - which reads as the model's effect withheld rather than as a number that was never set.
    const amount = (name: string) => parameterOf(operation, entry, name) ?? 0;

    /*
     * Which sentence the line is depends on what the enhancement has to report - a scale, an
     * intensity, a face count, or nothing but the model - so it is chosen against the operation's
     * own family rather than declared beside the enhancement's name. `noImplicitReturns` is what
     * makes a new family arriving without a sentence a compile error rather than a row whose line
     * reads "NaN%".
     */
    const info = () => {
        switch (operation.family) {
            // Whether there is anything to remove yet: a Remove with nothing painted does nothing at all.
            case "remove":
                return removalPainted
                    ? t("enhancements.removal.painted", { name })
                    : t("enhancements.removal.unpainted", { name });
            case "face_recovery":
                /*
                 * **The count is the faces actually found in the photograph as it is framed**, so
                 * framing a face out of the picture changes what this reads.
                 *
                 * Between adding the enhancement and the detection landing there is no count to
                 * report, and what this must not do is read `0 Faces` and look like an answer. So a
                 * photograph whose faces are not known yet reports the model and the quality alone,
                 * and one where none were found reports none - the two are different sentences
                 * because they are different facts. Only a known set can be counted, and only a
                 * known set can be chosen from.
                 *
                 * **Where some have been skipped it reports the ratio**, so a row cannot say `2
                 * Faces` while the run restores one.
                 */
                return faces === undefined
                    ? t("enhancements.infoModel", { name, quality })
                    : t("enhancements.infoFaces", {
                          name,
                          quality,
                          /*
                           * The reference's `facesLabel` exactly: i18next composes the two suffixes
                           * as `key_context_plural`, so `faces_partial_one` and `faces_partial_other`
                           * are selected without a second call site knowing they exist. **Pluralised
                           * on the total rather than on the chosen count**, which is what makes
                           * `0/2 Faces` read correctly - and a language with more plural forms adds
                           * keys to its own catalogue with no code change here.
                           */
                          faces: t("enhancements.faces", {
                              count: faces.length,
                              enabled: chosen,
                              ...(chosen !== faces.length && { context: "partial" }),
                          }),
                      });
            case "denoise":
            case "sharpen":
                // Shared by the two families that carry a strength, and kept apart from the bias cases below:
                // the field they read differs.
                return t("enhancements.info", { name, quality, intensity: toPercent(amount("strength")) });
            case "light_adjustment":
            case "color_balance":
                // A negative bias keeps its sign: the direction is what it changes.
                return t("enhancements.info", { name, quality, intensity: toPercent(amount("bias")) });
            case "colorization":
                // The model and the quality alone: there is no amount to report. Its own case rather than a
                // fall-through onto face recovery, whose case counts faces.
                return t("enhancements.infoModel", { name, quality });
            case "upscale":
                return t("enhancements.infoScale", {
                    name,
                    quality,
                    // 1x where the scale is known to nobody: a factor that changes nothing.
                    scale: Number.parseFloat((parameterOf(operation, entry, "scale") ?? 1).toFixed(SCALE_PLACES)),
                });
            case "restore":
                // Both of what a restore carries: the size, read as the upscale's is, and the strength.
                return t("enhancements.infoRestore", {
                    name,
                    quality,
                    scale: Number.parseFloat((parameterOf(operation, entry, "scale") ?? 1).toFixed(SCALE_PLACES)),
                    intensity: toPercent(amount("strength")),
                });
            case "lens_blur": {
                // The model and the blur amount, with no quality: the model is published at one precision, which says
                // nothing. The amount is the slider's, read against the photograph's own aperture.
                const f = parameterOf(operation, entry, "f_number");

                // The model alone where the operation and the catalogue know no aperture, or the photograph's own lens
                // is still on its way (a number now would change as it landed): nothing to report yet.
                return f === undefined || lens === undefined
                    ? t("enhancements.infoModel", { name, quality })
                    : t("enhancements.infoLensBlur", {
                          name,
                          amount: shownAmount(f, (lens ?? ASSUMED_LENS).ownFNumber),
                      });
            }
        }
    };

    const label = (
        <>
            <Icon className="size-5 flex-none" />

            <span className={cn("flex min-w-0 flex-col gap-px", suspended && "opacity-60")}>
                <span className="text-[13px]">{t(enhancement.nameKey)}</span>
                <span className="text-xs text-foreground-dim italic" data-slot="enhancement-info">
                    {/* What it is set to do says nothing while it does not run; why it does not, does. */}
                    {suspended ? t(suspended) : info()}
                </span>
            </span>
        </>
    );

    /*
     * **A painted enhancement's row opens the brush, not a popover.** Its whole setting is what is painted over the
     * preview, and a popover would close on the first press there (Radix's outside click). The row toggles the brush
     * panel in the sidebar, under the list, and reads as pressed while it is up - the open row's paint, as a popover's
     * row has it.
     */
    const brush = enhancement.painted === true;

    return (
        <div
            className="group relative border-b border-border"
            data-slot="enhancement-row"
            data-family={operation.family}
        >
            {brush ? (
                <HintTooltip tip={rowHint.tooltip}>
                    <button
                        type="button"
                        data-slot="enhancement-trigger"
                        aria-pressed={removalEditing}
                        aria-keyshortcuts={rowHint.ariaKeyShortcuts}
                        onClick={(event) => {
                            const store = useMaskStore.getState();

                            if (removalEditing) store.stopEditing();
                            else store.startEditing(path, "removal");

                            // A mouse press lets go of the focus, as the panel's controls do: Space is the brush's
                            // pan, and a focused row would take it as a press that closes the brush again.
                            if (event.detail !== 0) event.currentTarget.blur();
                        }}
                        className="flex min-h-13 w-full items-center gap-3 py-2 pr-10 pl-4 text-left hover:bg-secondary aria-pressed:bg-secondary"
                    >
                        {label}
                    </button>
                </HintTooltip>
            ) : (
                <Popover open={panelOpen} onOpenChange={setPanelOpen}>
                    <HintTooltip tip={rowHint.tooltip}>
                        <PopoverTrigger asChild>
                            {/*
                             * The whole row is the trigger, which is the reference's arrangement and the
                             * design's (screen 14).
                             *
                             * `min-h` rather than a fixed height: the two lines are 13px and 12px in every
                             * language, but a model name long enough to wrap has to be able to. The panel is
                             * anchored against `ENHANCEMENT_ROW_HEIGHT`, which is this `min-h-13`.
                             *
                             * `aria-expanded:bg-secondary` beside the hover, because the design paints an
                             * open row exactly as it paints a hovered one (screen 11) - a row whose panel is
                             * up with nothing marking it is a panel with no visible source. Not
                             * `data-[state=open]`: the tooltip around the trigger writes its own `data-state`
                             * over the popover's.
                             */}
                            <button
                                type="button"
                                data-slot="enhancement-trigger"
                                aria-keyshortcuts={rowHint.ariaKeyShortcuts}
                                className="flex min-h-13 w-full items-center gap-3 py-2 pr-10 pl-4 text-left hover:bg-secondary aria-expanded:bg-secondary"
                            >
                                {label}
                            </button>
                        </PopoverTrigger>
                    </HintTooltip>

                    <OptionsPanel title={t(enhancement.nameKey)}>
                        {suspended ? (
                            <>
                                <p className="text-muted-foreground text-xs" data-slot="enhancement-suspended">
                                    {t(suspended)}
                                </p>
                                {/*
                                 * Drawn as it is set, so the user sees what will run again once the reason goes, and
                                 * unavailable: a disabled fieldset disables every control inside it, the model
                                 * tray's and the scale's alike, with no control having to know.
                                 */}
                                <fieldset disabled className="flex min-w-0 flex-col gap-4 opacity-50">
                                    {options()}
                                </fieldset>
                            </>
                        ) : (
                            options()
                        )}
                    </OptionsPanel>
                </Popover>
            )}

            {/*
             * A sibling of the popover rather than a child of it, so closing the panel above does not
             * take the dialog down with it. Mounted only for the family that offers it, which is the
             * same carve-out the info line and the options panel already make.
             */}
            {operation.family === "face_recovery" && (
                <SelectFacesDialog open={choosingFaces} onClose={() => setChoosingFaces(false)} />
            )}

            <HintTooltip tip={removeHint.tooltip}>
                <Button
                    variant="secondary"
                    size="icon-sm"
                    aria-label={t("enhancements.remove")}
                    aria-keyshortcuts={removeHint.ariaKeyShortcuts}
                    onClick={onRemove}
                    // Hover alone is the design's rule and the reference's, and it is what keeps a list of
                    // several reading as a list rather than as a column of buttons - but hover is not
                    // reachable from a keyboard, so the control is in the tab order and shows itself when
                    // it is focused. Drawn at zero opacity rather than not drawn at all, which is what
                    // keeps it focusable in the first place.
                    className="absolute top-1/2 right-2 size-6.5 -translate-y-1/2 bg-input opacity-0 focus-visible:opacity-100 group-hover:opacity-100"
                >
                    <X className="size-4" />
                </Button>
            </HintTooltip>
        </div>
    );
};
