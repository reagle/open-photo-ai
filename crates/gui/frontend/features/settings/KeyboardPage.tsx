import { type KeyboardEvent, type Ref, useId, useMemo, useRef, useState } from "react";
import type { ParseKeys } from "i18next";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { isMacOs } from "@/ipc/os";
import {
    bindingFromEvent,
    formatBinding,
    MAX_BINDINGS,
    overlaps,
    type Refusal,
    refusal,
    resolveShortcuts,
    SHORTCUT_ACTIONS,
    type ShortcutAction,
    type StoredShortcuts,
} from "@/lib/shortcuts";
import { useSettingsDraft } from "./draft";
import { SettingsCard } from "./rows.tsx";

/** The attribute the recorder carries while it is recording - and only then. */
const RECORDER = "data-shortcut-recorder";

/**
 * Whether a key event landed on a recorder that is recording: the settings dialog's question before
 * it lets `Escape` close it, since Radix asks in the capture phase, before the recorder hears the key.
 */
export const isRecording = (target: EventTarget | null) =>
    target instanceof Element && target.closest(`[${RECORDER}]`) !== null;

/** What a row says after a recording that added nothing: a refused key, or one another action holds. */
type Notice =
    | { kind: "refused"; action: ShortcutAction; binding: string; reason: Refusal }
    | { kind: "conflict"; action: ShortcutAction; binding: string; holder: ShortcutAction };

/** The name of an action, as the page and its messages print it. */
const actionKey = (action: ShortcutAction) => `settings.keyboard.actions.${action}` as ParseKeys;

/**
 * Add shortcut, which becomes the recorder when pressed: it keeps the focus and takes the next key
 * combination that is not a modifier on its own.
 *
 * `Escape` on its own ends the recording with nothing added (the dialog is told not to close - see
 * `isRecording`); `Tab` ends it and is let through, so the focus moves as it always does; so does
 * anything else that takes the focus away. Every other key is the recording's, default prevented, so
 * `Enter` does not press the button it lands on - and the `keyup` of a recorded `Space`, which is
 * when a button activates on it, is swallowed for the same reason.
 */
const Recorder = ({
    action,
    name,
    active,
    disabled,
    describedBy,
    buttonRef,
    onStart,
    onStop,
    onRecord,
}: {
    action: ShortcutAction;
    name: string;
    active: boolean;
    disabled: boolean;
    describedBy: string;
    buttonRef: Ref<HTMLButtonElement>;
    onStart: () => void;
    onStop: () => void;
    onRecord: (binding: string) => void;
}) => {
    const { t } = useTranslation();
    const mac = isMacOs();
    const swallowKeyUp = useRef(false);

    const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
        if (!active) return;
        if (event.key === "Tab") {
            onStop();
            return;
        }

        event.preventDefault();
        event.stopPropagation();

        const plainEscape =
            event.key === "Escape" && !event.shiftKey && !event.altKey && !event.ctrlKey && !event.metaKey;
        if (plainEscape) {
            onStop();
            return;
        }

        // A modifier on its own names no key yet: keep listening for the rest of the combination.
        const binding = bindingFromEvent(event.nativeEvent, mac);
        if (!binding) return;

        swallowKeyUp.current = true;
        onRecord(binding);
    };

    return (
        <Button
            ref={buttonRef}
            type="button"
            variant="outline"
            size="sm"
            disabled={disabled}
            data-action={action}
            {...{ [RECORDER]: active ? "" : undefined }}
            // The visible words are the start of the name, so a voice user can say what they see; the
            // action makes the four buttons tell apart. While recording, the words themselves are the name.
            aria-label={active ? undefined : t("settings.keyboard.addLabel", { action: name })}
            aria-describedby={describedBy}
            className="h-7 bg-transparent px-2.5 font-normal text-[12px] dark:bg-transparent"
            onClick={() => !active && onStart()}
            onKeyDown={onKeyDown}
            onKeyUp={(event) => {
                if (!swallowKeyUp.current) return;

                swallowKeyUp.current = false;
                event.preventDefault();
            }}
            onBlur={() => active && onStop()}
        >
            {t(active ? "settings.keyboard.recording" : "settings.keyboard.add")}
        </Button>
    );
};

/**
 * The Keyboard page: every action the keyboard performs, with the keys it is bound to.
 *
 * **Edits the draft like every other page**, so nothing changes until Save, Cancel discards, and the
 * footer's Reset to defaults - which writes `shortcuts: {}` - puts every action back on its defaults.
 * What it shows is what `resolveShortcuts` puts in force, so a page and the window cannot disagree.
 *
 * Changing one action stores that action's whole list; an action never touched stays absent and keeps
 * following the defaults of whatever build is running.
 */
export const KeyboardPage = () => {
    const { t } = useTranslation();
    const { values, update } = useSettingsDraft();
    const mac = isMacOs();
    const baseId = useId();
    const [recording, setRecording] = useState<ShortcutAction>();
    const [notice, setNotice] = useState<Notice>();
    // Each row's Add shortcut, where the focus goes when the control that had it is removed - a chip's
    // remove button, or Use here instead - rather than falling back to the document.
    const addButtons = useRef(new Map<ShortcutAction, HTMLButtonElement>());
    const refocus = (action: ShortcutAction) => setTimeout(() => addButtons.current.get(action)?.focus());

    const table = useMemo(() => resolveShortcuts(values.shortcuts, mac), [values.shortcuts, mac]);
    const show = (binding: string) => formatBinding(binding, mac, t("settings.keyboard.space"));
    const write = (patch: StoredShortcuts) => update({ shortcuts: { ...values.shortcuts, ...patch } });

    const record = (action: ShortcutAction, binding: string) => {
        setRecording(undefined);
        setNotice(undefined);

        const reason = refusal(binding, mac);
        if (reason) {
            setNotice({ kind: "refused", action, binding, reason });
            return;
        }

        // Already this action's: nothing to add, and nothing worth saying.
        if (table[action].some((held) => overlaps(held, binding, mac))) return;

        const holder = SHORTCUT_ACTIONS.find(
            (other) => other !== action && table[other].some((held) => overlaps(held, binding, mac)),
        );
        if (holder) {
            setNotice({ kind: "conflict", action, binding, holder });
            return;
        }

        write({ [action]: [...table[action], binding] });
    };

    // Both halves in one write, so no draft exists in which the binding is on both actions or on neither.
    const move = (action: ShortcutAction, binding: string, holder: ShortcutAction) => {
        setNotice(undefined);
        refocus(action);
        write({
            [holder]: table[holder].filter((held) => !overlaps(held, binding, mac)),
            [action]: [...table[action], binding],
        });
    };

    const remove = (action: ShortcutAction, binding: string) => {
        setNotice(undefined);
        refocus(action);
        write({ [action]: table[action].filter((held) => held !== binding) });
    };

    const opensList = table.showShortcuts[0];

    return (
        <>
            <SettingsCard>
                {SHORTCUT_ACTIONS.map((action) => {
                    const name = t(actionKey(action));
                    const bindings = table[action];
                    const statusId = `${baseId}-${action}-status`;
                    const ownNotice = notice?.action === action ? notice : undefined;

                    return (
                        <div key={action} data-slot="shortcut-row" className="flex flex-col px-4 py-3">
                            <div className="flex items-center justify-between gap-6">
                                <span className="text-sm">{name}</span>

                                <div className="flex flex-wrap items-center justify-end gap-1.5">
                                    {bindings.length === 0 ? (
                                        <span className="text-[13px] text-muted-foreground">
                                            {t("settings.keyboard.none")}
                                        </span>
                                    ) : (
                                        bindings.map((binding) => (
                                            <span
                                                key={binding}
                                                className="inline-flex h-7 items-center gap-1 rounded-md border border-border bg-background pr-1 pl-2"
                                            >
                                                <kbd className="font-mono text-[12px]">{show(binding)}</kbd>
                                                <button
                                                    type="button"
                                                    aria-label={t("settings.keyboard.remove", {
                                                        binding: show(binding),
                                                        action: name,
                                                    })}
                                                    onClick={() => remove(action, binding)}
                                                    className="flex size-5 items-center justify-center rounded-sm text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
                                                >
                                                    <X className="size-3.5" aria-hidden />
                                                </button>
                                            </span>
                                        ))
                                    )}

                                    <Recorder
                                        action={action}
                                        name={name}
                                        active={recording === action}
                                        disabled={bindings.length >= MAX_BINDINGS}
                                        describedBy={statusId}
                                        buttonRef={(element) => {
                                            if (element) addButtons.current.set(action, element);
                                            else addButtons.current.delete(action);
                                        }}
                                        onStart={() => {
                                            setNotice(undefined);
                                            setRecording(action);
                                        }}
                                        onStop={() => setRecording(undefined)}
                                        onRecord={(binding) => record(action, binding)}
                                    />
                                </div>
                            </div>

                            {/*
                             * Always in the tree, empty when there is nothing to say, so a screen reader
                             * has a live region to hear the prompt and the outcome through - one that
                             * appeared with its text would not be announced.
                             */}
                            <div
                                id={statusId}
                                role="status"
                                className="mt-2 flex flex-wrap items-center justify-end gap-x-2 text-right text-[12px] text-muted-foreground empty:mt-0"
                            >
                                {recording === action && t("settings.keyboard.prompt", { action: name })}
                                {ownNotice?.kind === "refused" &&
                                    t(`settings.keyboard.refused.${ownNotice.reason}`, {
                                        binding: show(ownNotice.binding),
                                    })}
                                {ownNotice?.kind === "conflict" && (
                                    <>
                                        <span>
                                            {t("settings.keyboard.conflict", {
                                                binding: show(ownNotice.binding),
                                                action: t(actionKey(ownNotice.holder)),
                                            })}
                                        </span>
                                        <Button
                                            variant="link"
                                            className="h-auto p-0 text-[12px]"
                                            onClick={() => move(action, ownNotice.binding, ownNotice.holder)}
                                        >
                                            {t("settings.keyboard.useHere")}
                                        </Button>
                                    </>
                                )}
                            </div>
                        </div>
                    );
                })}
            </SettingsCard>

            {/* How to see these from the window, which is the one place nothing else says it. */}
            {opensList !== undefined && (
                <p className="m-0 text-[12px] text-muted-foreground">
                    {t("shortcuts.settingsHint", { key: show(opensList) })}
                </p>
            )}
        </>
    );
};
