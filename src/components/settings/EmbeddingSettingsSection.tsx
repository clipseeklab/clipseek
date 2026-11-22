import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ScanSearch, Loader2 } from "lucide-react";
import { queryKeys } from "@/lib/queryKeys";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { toast } from "@/components/Toaster";
import { explainError } from "@/lib/errors";
import type { ModelStatus } from "@/types";
export default function EmbeddingSettingsSection() {
  const { t, i18n } = useTranslation();
  const zh = i18n.language.startsWith("zh");
  const client = useQueryClient();
  const status = useQuery({ queryKey: queryKeys.embeddingModel, queryFn: api.modelStatus });
  const [precision, setPrecision] = useState<ModelStatus["precision"]>("q8");
  const [busy, setBusy] = useState(false);
  const directory = status.data?.modelDirectory;
  useEffect(() => {
    if (status.data) setPrecision(status.data.precision);
  }, [status.data]);
  const run = async () => {
    if (!directory) return;
    setBusy(true);
    try {
      const selected =
        precision !== status.data?.precision
          ? await api.modelSettings(precision, directory)
          : status.data;
      if (!selected?.ready) await api.modelDownload();
      toast.success(zh ? "模型设置已更新" : "Model settings updated");
    } catch (e) {
      toast.error(explainError(e));
    } finally {
      await client.invalidateQueries();
      setBusy(false);
    }
  };
  return (
    <section
      id="settings-model"
      className="scroll-mt-6 space-y-3 rounded-lg border bg-muted/20 p-4"
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="flex items-center gap-2 font-medium">
          <ScanSearch className="size-4 text-muted-foreground" />
          {t("settings.modelManagement")}
        </h2>
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span>EmbeddingGemma 2</span>
          {status.data?.execution && (
            <span
              className="rounded bg-muted px-1.5 py-0.5"
              title={status.data.execution.fallbackReason ?? undefined}
            >
              {
                { cpu: "CPU", metal: "Metal GPU", coreml: "CoreML GPU" }[
                  status.data.execution.backend
                ]
              }
            </span>
          )}
        </div>
      </div>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          <span className="text-xs text-muted-foreground">{zh ? "精度" : "Precision"}</span>
          <div
            role="group"
            aria-label={zh ? "模型精度" : "Precision"}
            className="inline-flex gap-0.5 rounded-md border bg-background p-0.5"
          >
            {(["q4", "q8", "fp32"] as const).map((value) => (
              <Button
                key={value}
                variant={precision === value ? "default" : "ghost"}
                size="sm"
                className="h-7 px-3 text-xs"
                aria-pressed={precision === value}
                disabled={busy || !directory}
                onClick={() => setPrecision(value)}
              >
                {value.toUpperCase()}
              </Button>
            ))}
          </div>
        </div>
        <Button
          size="sm"
          className="h-8 text-xs"
          disabled={busy || !directory}
          onClick={() => void run()}
        >
          {busy && <Loader2 className="size-3.5 animate-spin" />}
          {zh ? "应用模型" : "Apply model"}
        </Button>
      </div>
      {status.error && (
        <p role="alert" className="text-xs text-destructive">
          {explainError(status.error)}
        </p>
      )}
    </section>
  );
}
