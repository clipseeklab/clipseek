import { useTranslation } from "react-i18next";
import { Progress } from "@/components/ui/progress";
import type { OperationProgress } from "@/types";
export function formatOperationBytes(bytes: number) {
  if (!bytes) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** index).toFixed(index > 2 ? 1 : 0)} ${units[index]}`;
}
/** Shared progress UI keeps merging, relocation, and cross-library transfer feedback consistent. */
export function OperationStatus({ progress }: { progress: OperationProgress }) {
  const { t } = useTranslation();
  const percent = progress.totalBytes
    ? (progress.completedBytes / progress.totalBytes) * 100
    : progress.totalItems
      ? (progress.completedItems / progress.totalItems) * 100
      : 0;
  return (
    <div className="space-y-2 rounded-md border bg-muted/30 p-3 text-sm">
      <div className="flex justify-between gap-3">
        <span>{t(`library.operationPhase.${progress.phase}`)}</span>
        <span>{Math.round(percent)}%</span>
      </div>
      <Progress value={percent} className="h-1.5" />
      {progress.totalBytes > 0 && (
        <div className="text-xs text-muted-foreground">
          {formatOperationBytes(progress.completedBytes)} /{" "}
          {formatOperationBytes(progress.totalBytes)}
        </div>
      )}
      {progress.totalItems > 0 && (
        <div className="text-xs text-muted-foreground">
          {t("library.operationItems", {
            done: progress.completedItems,
            total: progress.totalItems,
            skipped: progress.skipped,
            failed: progress.failed,
          })}
        </div>
      )}
      {progress.currentItem && (
        <div className="truncate text-xs text-muted-foreground">{progress.currentItem}</div>
      )}
      {progress.message && <div className="text-xs text-destructive">{progress.message}</div>}
    </div>
  );
}
