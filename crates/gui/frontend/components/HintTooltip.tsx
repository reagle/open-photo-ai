import type { ReactElement } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

/**
 * A tooltip on one control, for the hint a shortcut hook returns - `useShortcutHint` or
 * `useAppShortcutHint` - so every control that names its key does it the same way: on hover and on
 * focus, with the app's own tooltip and its delay.
 *
 * `children` is the control itself, a single element that takes a ref and the trigger's handlers.
 */
export const HintTooltip = ({ tip, children }: { tip: string; children: ReactElement }) => (
    <Tooltip>
        <TooltipTrigger asChild>{children}</TooltipTrigger>

        <TooltipContent>{tip}</TooltipContent>
    </Tooltip>
);
