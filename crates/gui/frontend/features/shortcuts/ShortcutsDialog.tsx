import { useId } from "react";
import type { ParseKeys } from "i18next";
import { useTranslation } from "react-i18next";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { DialogTitleBar } from "@/components/ui/dialog-title-bar";
import { useShortcutBindings } from "@/hooks/useShortcuts";
import { isMacOs } from "@/ipc/os";
import { formatBinding, SHORTCUT_ACTIONS } from "@/lib/shortcuts";
import { useShortcutsListStore } from "@/stores/shortcutsList";

/**
 * The keyboard shortcuts list: every action with the keys it is bound to now, as this platform writes
 * them, opened from the keyboard (`?` by default) and mounted once by the shell.
 *
 * Read-only on purpose. Changing a binding is a draft with Save and Cancel, which the settings dialog
 * already is; a second place to edit them would be a second draft to keep in step. The line under the
 * list says where that is. While it is open the window's shortcuts are off - it is a dialog - so the
 * key that opened it does not close it; Escape, the close box and a click outside do.
 */
export const ShortcutsDialog = () => {
    const { t } = useTranslation();
    const open = useShortcutsListStore((state) => state.open);
    const setOpen = useShortcutsListStore((state) => state.setOpen);
    const bindings = useShortcutBindings();
    const mac = isMacOs();
    const descriptionId = useId();

    return (
        <Dialog open={open} onOpenChange={setOpen}>
            <DialogContent
                showCloseButton={false}
                aria-describedby={descriptionId}
                // `sm:max-w-none` as well as `max-w-none`, for the reason `SettingsDialog` gives.
                className="w-md max-w-none gap-0 overflow-hidden rounded-xl bg-card p-0 shadow-[0_24px_60px_rgb(0_0_0/0.7)] sm:max-w-none"
            >
                <DialogTitleBar title={t("shortcuts.title")} />

                <div className="flex flex-col gap-4 px-6 pt-3 pb-6">
                    <p id={descriptionId} className="m-0 text-[13px] text-muted-foreground">
                        {t("shortcuts.description")}
                    </p>

                    <dl className="m-0 flex flex-col divide-y divide-border rounded-lg border border-border">
                        {SHORTCUT_ACTIONS.map((action) => (
                            <div key={action} className="flex items-center justify-between gap-6 px-4 py-2">
                                <dt className="text-sm">{t(`settings.keyboard.actions.${action}` as ParseKeys)}</dt>
                                <dd className="m-0 flex flex-wrap justify-end gap-1.5">
                                    {bindings[action].length === 0 ? (
                                        <span className="text-[13px] text-muted-foreground">
                                            {t("settings.keyboard.none")}
                                        </span>
                                    ) : (
                                        bindings[action].map((binding) => (
                                            <kbd
                                                key={binding}
                                                className="rounded-md border border-border bg-background px-2 py-0.5 font-mono text-[12px]"
                                            >
                                                {formatBinding(binding, mac, t("settings.keyboard.space"))}
                                            </kbd>
                                        ))
                                    )}
                                </dd>
                            </div>
                        ))}
                    </dl>

                    <p className="m-0 text-[12px] text-muted-foreground">{t("shortcuts.hint")}</p>
                </div>
            </DialogContent>
        </Dialog>
    );
};
