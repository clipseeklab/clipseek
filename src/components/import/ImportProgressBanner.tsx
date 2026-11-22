import { Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Progress } from "@/components/ui/progress";
import { importOverallFraction } from "@/hooks/useImportEta";
import type { ImportProgress } from "@/types";
export default function ImportProgressBanner({
  progress,
  etaSeconds,
}: {
  progress: ImportProgress;
  etaSeconds: number | null;
}) {
  const { t } = useTranslation();
  return (
    <div className="shrink-0 border-b bg-background px-6 py-2">
      <div className="flex items-center gap-3 text-sm">
        <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate text-muted-foreground">
          {progress.stage === "embed"
            ? t("assets.embedding", { done: progress.done, total: progress.total })
            : t("assets.importing", { done: progress.done, total: progress.total })}
          {progress.currentFile ? ` · ${progress.currentFile}` : ""}
        </span>
        <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
          {progress.done >= progress.total
            ? t("assets.finishingImport")
            : etaSeconds == null
              ? t("assets.etaCalculating")
              : t("assets.etaRemaining", { time: formatEta(etaSeconds, t) })}
        </span>
        <Progress value={importOverallFraction(progress) * 100} className="h-1.5 w-48" />
      </div>
    </div>
  );
}
function formatEta(seconds: number, t: ReturnType<typeof useTranslation>["t"]) {
  if (seconds < 60) return t("assets.etaLessThanMinute");
  const minutes = Math.ceil(seconds / 60);
  if (minutes < 60) return t("assets.etaMinutes", { count: minutes });
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  return remainingMinutes > 0
    ? t("assets.etaHoursMinutes", { hours, minutes: remainingMinutes })
    : t("assets.etaHours", { count: hours });
}
