import { useId } from "react";
import { useTranslation } from "react-i18next";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { DialogTitleBar } from "@/components/ui/dialog-title-bar";
import { useAppShortcuts } from "@/hooks/useAppShortcuts";
import { useShortcutBindings } from "@/hooks/useShortcuts";
import { isMacOs } from "@/ipc/os";
import { useShortcutsListStore } from "@/stores/shortcutsList";
import { LIST_SECTIONS, rowKeys } from "./rows.ts";

/**
 * The keyboard shortcuts list: every action with the keys it is bound to now, as this platform writes
 * them, opened from the keyboard (`?` by default) and mounted once by the shell.
 *
 * Read-only on purpose. Changing a binding is a draft with Save and Cancel, which the settings dialog
 * already is; a second place to edit them would be a second draft to keep in step. The line under the
 * list says where that is. While it is open the window's shortcuts are off - it is a dialog - so the
 * key that opened it does not close it; Escape, the close box and a click outside do. The
 * application's own keys (Open, Export, Settings, Close Image, Close All Images, Quit) head the list, fixed.
 * The rest sit under View, Images and Enhancements, with alike actions on one row - see `rows.ts`.
 */
export const ShortcutsDialog = () => {
    const { t } = useTranslation();
    const open = useShortcutsListStore((state) => state.open);
    const setOpen = useShortcutsListStore((state) => state.setOpen);
    const bindings = useShortcutBindings();
    const mac = isMacOs();
    const descriptionId = useId();
    const applicationId = useId();
    const sectionId = useId();
    const fixed = useAppShortcuts();

    return (
        <Dialog open={open} onOpenChange={setOpen}>
            <DialogContent
                showCloseButton={false}
                aria-describedby={descriptionId}
                // `sm:max-w-none` as well as `max-w-none`, for the reason `SettingsDialog` gives.
                className="w-md max-w-none gap-0 overflow-hidden rounded-xl bg-card p-0 shadow-[0_24px_60px_rgb(0_0_0/0.7)] sm:max-w-none"
            >
                <DialogTitleBar title={t("shortcuts.title")} />

                {/* Scrolled rather than grown: the list outgrows a small window now that the editor has keys. */}
                <div className="flex max-h-[75vh] flex-col gap-4 overflow-y-auto px-6 pt-3 pb-6">
                    <p id={descriptionId} className="m-0 text-[13px] text-muted-foreground">
                        {t("shortcuts.description")}
                    </p>

                    {/* The application's own keys first, fixed: the same group the Keyboard page shows. */}
                    <section aria-labelledby={applicationId} className="flex flex-col gap-2">
                        <h3 id={applicationId} className="m-0 font-medium text-[13px] text-muted-foreground">
                            {t("settings.keyboard.application")}
                        </h3>
                        <dl className="m-0 flex flex-col divide-y divide-border rounded-lg border border-border">
                            {fixed.map(({ action, name, keys }) => (
                                <div key={action} className="flex items-center justify-between gap-6 px-4 py-2">
                                    <dt className="text-sm">{name}</dt>
                                    <dd className="m-0 flex flex-wrap justify-end gap-1.5">
                                        {keys.map((shown) => (
                                            <kbd
                                                key={shown}
                                                className="rounded-md border border-border bg-background px-2 py-0.5 font-mono text-[12px]"
                                            >
                                                {shown}
                                            </kbd>
                                        ))}
                                    </dd>
                                </div>
                            ))}
                        </dl>
                    </section>

                    {LIST_SECTIONS.map((section) => (
                        <section
                            key={section.id}
                            aria-labelledby={`${sectionId}-${section.id}`}
                            className="flex flex-col gap-2"
                        >
                            <h3
                                id={`${sectionId}-${section.id}`}
                                className="m-0 font-medium text-[13px] text-muted-foreground"
                            >
                                {t(`shortcuts.sections.${section.id}`)}
                            </h3>
                            <dl className="m-0 flex flex-col divide-y divide-border rounded-lg border border-border">
                                {section.rows.map((row) => {
                                    const keys = rowKeys(
                                        row.actions.map((action) => bindings[action]),
                                        mac,
                                        t("settings.keyboard.space"),
                                    );

                                    return (
                                        <div key={row.id} className="flex items-center justify-between gap-6 px-4 py-2">
                                            <dt className="text-sm">{t(row.label)}</dt>
                                            <dd className="m-0 flex flex-wrap justify-end gap-1.5">
                                                {keys.length === 0 ? (
                                                    <span className="text-[13px] text-muted-foreground">
                                                        {t("settings.keyboard.none")}
                                                    </span>
                                                ) : (
                                                    keys.map((shown) => (
                                                        <kbd
                                                            key={shown}
                                                            className="rounded-md border border-border bg-background px-2 py-0.5 font-mono text-[12px]"
                                                        >
                                                            {shown}
                                                        </kbd>
                                                    ))
                                                )}
                                            </dd>
                                        </div>
                                    );
                                })}
                            </dl>
                        </section>
                    ))}

                    <p className="m-0 text-[12px] text-muted-foreground">{t("shortcuts.hint")}</p>
                </div>
            </DialogContent>
        </Dialog>
    );
};
