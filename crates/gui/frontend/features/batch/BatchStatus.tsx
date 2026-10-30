import { Pause, Play } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useBatchCounts } from "@/hooks/useBatch";
import { pauseBatch, processAll, resumeBatch } from "@/lib/batch";
import { useBatchStore } from "@/stores/batch";

/** The least a "Process all" is offered for: one photograph is processed by looking at it. */
const PROCESS_ALL_FROM = 2;

/**
 * The batch, in the navbar: *Processing 7 of 21* with Pause while it runs, *Resume (14 left)* while it is paused, and
 * *Process all (5)* where two or more open photographs have enhancements the window has not run yet and no batch is
 * under way. Nothing otherwise.
 *
 * In the navbar rather than over the canvas: the canvas's corners are the panes' chips and the run's own bar, which
 * say what is happening to the photograph on screen; this says what is happening to all of them.
 */
export const BatchStatus = () => {
    const { t } = useTranslation();
    const phase = useBatchStore((state) => state.phase);
    const { done, failed, left, total, unprocessed } = useBatchCounts();

    if (phase === "running") {
        return (
            <div className="flex items-center gap-2 text-xs" data-slot="batch-status" role="status">
                <span className="text-muted-foreground">
                    {t("batch.processing", { current: Math.min(done + failed + 1, total), total })}
                </span>
                <Button variant="ghost" size="xs" onClick={pauseBatch}>
                    <Pause aria-hidden />
                    {t("batch.pause")}
                </Button>
            </div>
        );
    }

    if (phase === "paused" && left > 0) {
        return (
            <Button variant="secondary" size="xs" onClick={resumeBatch} data-slot="batch-status">
                <Play aria-hidden />
                {t("batch.resume", { count: left })}
            </Button>
        );
    }

    if (unprocessed >= PROCESS_ALL_FROM) {
        return (
            <Button variant="secondary" size="xs" onClick={processAll} data-slot="batch-status">
                <Play aria-hidden />
                {t("batch.processAll", { count: unprocessed })}
            </Button>
        );
    }

    return null;
};
