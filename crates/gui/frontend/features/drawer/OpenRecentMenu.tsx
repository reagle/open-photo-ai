import { History } from "lucide-react";
import { useTranslation } from "react-i18next";
import { HintTooltip } from "@/components/HintTooltip";
import { Button } from "@/components/ui/button";
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { isMacOs } from "@/ipc/os";
import { clearRecent } from "@/ipc/recent";
import { fileName } from "@/lib/paths";
import { openRecent } from "@/lib/recent";
import { report } from "@/lib/report";
import { useRecentStore } from "@/stores/recent";

/**
 * Open Recent on Windows and Linux, beside the drawer's Add images: the photographs opened most recently, newest first,
 * and Clear Menu - what File > Open Recent holds on macOS, where the menu bar has it and this is not drawn.
 *
 * Unavailable while the list is empty. An entry is named by its file, with the whole path as its title; choosing one
 * opens it through `openRecent`, as the macOS menu's entry does.
 */
export const OpenRecentMenu = () => {
    const { t } = useTranslation();
    const paths = useRecentStore((state) => state.paths);

    // The menu bar's on macOS; a second Open Recent here would be one place too many.
    if (isMacOs()) return null;

    const open = (path: string) => {
        openRecent(path).catch((error: unknown) => report("Failed to open a recent image", error));
    };
    const clear = () => {
        clearRecent().catch((error: unknown) => report("Failed to clear the recent images", error));
    };

    return (
        <DropdownMenu modal={false}>
            <HintTooltip tip={t("drawer.openRecent")}>
                <DropdownMenuTrigger asChild>
                    <Button
                        variant="ghost"
                        size="icon-sm"
                        disabled={paths.length === 0}
                        aria-label={t("drawer.openRecent")}
                    >
                        <History className="size-4.5" />
                    </Button>
                </DropdownMenuTrigger>
            </HintTooltip>

            <DropdownMenuContent side="top" align="start">
                {paths.map((path) => (
                    <DropdownMenuItem key={path} title={path} onSelect={() => open(path)}>
                        {fileName(path)}
                    </DropdownMenuItem>
                ))}
                <DropdownMenuSeparator />
                <DropdownMenuItem onSelect={clear}>{t("menu.bar.clearRecent")}</DropdownMenuItem>
            </DropdownMenuContent>
        </DropdownMenu>
    );
};
