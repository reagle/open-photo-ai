import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { DialogTitleBar } from "@/components/ui/dialog-title-bar";
import { answerPause } from "@/lib/batch";
import { useBatchStore } from "@/stores/batch";

/**
 * Asked when the user picks another photograph while the batch runs: *Pause processing and view this photo?* Pause &
 * view stops the run the batch is waiting on and goes to the photograph; Keep processing leaves everything as it was.
 *
 * **Don't ask again** is remembered with Pause & view alone - the answer a later click then gives without asking. With
 * Keep processing it would make every later click do nothing, which is no answer to remember.
 */
export const BatchPauseDialog = () => {
    const { t } = useTranslation();
    const asking = useBatchStore((state) => state.asking !== undefined);
    const [dontAsk, setDontAsk] = useState(false);

    const answer = (pause: boolean) => {
        answerPause(pause, dontAsk);
        setDontAsk(false);
    };

    return (
        <Dialog open={asking} onOpenChange={(open) => !open && answer(false)}>
            <DialogContent
                showCloseButton={false}
                aria-describedby={undefined}
                className="w-md max-w-none gap-0 overflow-hidden rounded-xl border-border bg-card p-0 sm:max-w-none"
                data-slot="batch-pause-dialog"
            >
                <DialogTitleBar title={t("batch.ask.title")} showCloseButton={false} />

                <div className="flex flex-col gap-5 p-6">
                    <p className="text-pretty text-[13px] text-muted-foreground leading-normal">
                        {t("batch.ask.body")}
                    </p>

                    <div className="flex items-center gap-2 text-[13px]">
                        <Checkbox
                            id="batch-dont-ask"
                            checked={dontAsk}
                            onCheckedChange={(checked) => setDontAsk(checked === true)}
                        />
                        <label htmlFor="batch-dont-ask">{t("batch.ask.dontAsk")}</label>
                    </div>

                    <div className="flex justify-end gap-3">
                        <Button variant="secondary" onClick={() => answer(false)}>
                            {t("batch.ask.keep")}
                        </Button>
                        <Button onClick={() => answer(true)}>{t("batch.ask.pause")}</Button>
                    </div>
                </div>
            </DialogContent>
        </Dialog>
    );
};
