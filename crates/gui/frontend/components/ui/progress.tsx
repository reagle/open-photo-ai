import type { ComponentProps } from "react";
import { Progress as ProgressPrimitive } from "radix-ui";
import { cn } from "@/lib/utils";

/**
 * A progress bar, or - where `value` is `null` - an indeterminate one: a third of the track sweeping across it, for
 * work that is happening and cannot say how far it has got. `null` is Radix's own spelling of that, and what the root
 * reports to a screen reader as no value.
 */
const Progress = ({ className, value, ...props }: ComponentProps<typeof ProgressPrimitive.Root>) => {
    const indeterminate = value === null;

    return (
        <ProgressPrimitive.Root
            data-slot="progress"
            className={cn("relative h-2 w-full overflow-hidden rounded-full bg-primary/20", className)}
            {...props}
        >
            <ProgressPrimitive.Indicator
                data-slot="progress-indicator"
                className={cn(
                    "h-full w-full flex-1 bg-primary transition-all",
                    indeterminate && "w-1/3 animate-progress-sweep transition-none motion-reduce:animate-none",
                )}
                {...(!indeterminate && { style: { transform: `translateX(-${100 - (value || 0)}%)` } })}
            />
        </ProgressPrimitive.Root>
    );
};

export { Progress };
