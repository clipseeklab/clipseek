import { useAppStore } from "@/stores/app";
import { queryKeys } from "@/lib/queryKeys";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { Database, HardDrive, Loader2 } from "lucide-react";
import { api } from "@/lib/api";
import { Progress } from "@/components/ui/progress";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";
function formatBytes(bytes: number) {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const unit = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** unit).toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}
export default function StorageStatsButton() {
  const { t } = useTranslation();
  const libraryId = useAppStore((state) => state.current?.id);
  const { data, isLoading, isStale, refetch } = useQuery({
    queryKey: queryKeys.storageStats.library(libraryId),
    queryFn: api.assetStorageStats,
    enabled: false,
    staleTime: 10 * 60 * 1000,
    gcTime: 30 * 60 * 1000,
    refetchOnWindowFocus: false,
  });
  const remainingRatio = data?.diskTotalBytes ? data.diskAvailableBytes / data.diskTotalBytes : 1;
  const color =
    remainingRatio < 0.1
      ? "text-red-500"
      : remainingRatio <= 0.2
        ? "text-yellow-500"
        : "text-emerald-500";
  const progressColor =
    remainingRatio < 0.1
      ? "[&_[data-slot=progress-indicator]]:bg-red-500"
      : remainingRatio <= 0.2
        ? "[&_[data-slot=progress-indicator]]:bg-yellow-500"
        : "[&_[data-slot=progress-indicator]]:bg-emerald-500";
  return (
    <Popover
      onOpenChange={(open) => {
        if (open && (!data || isStale)) void refetch();
      }}
    >
      <PopoverTrigger asChild>
        <button
          type="button"
          className="rounded-md p-1.5 transition-colors hover:bg-accent"
          aria-label={t("assets.storage.title")}
          title={t("assets.storage.title")}
        >
          <HardDrive className={cn("h-4 w-4", color)} />
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-72">
        <div className="mb-3 flex items-center gap-2 font-medium">
          <Database className="h-4 w-4" />
          {t("assets.storage.title")}
        </div>
        {isLoading || !data ? (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t("common.loading")}
          </div>
        ) : (
          <div className="space-y-4">
            <div className="space-y-1.5">
              <div className="flex items-center justify-between text-xs">
                <span className="text-muted-foreground">{t("assets.storage.diskRemaining")}</span>
                <span className={cn("font-medium tabular-nums", color)}>
                  {(remainingRatio * 100).toFixed(1)}%
                </span>
              </div>
              <Progress
                value={(1 - remainingRatio) * 100}
                className={cn("h-2.5 bg-muted", progressColor)}
              />
            </div>
            <dl className="space-y-2 text-sm">
              <div className="flex justify-between gap-4">
                <dt className="text-muted-foreground">{t("assets.storage.assetCount")}</dt>
                <dd>{data.assetCount}</dd>
              </div>
              <div className="flex justify-between gap-4">
                <dt className="text-muted-foreground">{t("assets.storage.librarySize")}</dt>
                <dd>{formatBytes(data.libraryBytes)}</dd>
              </div>
              <div className="flex justify-between gap-4">
                <dt className="text-muted-foreground">{t("assets.storage.diskRemaining")}</dt>
                <dd>{formatBytes(data.diskAvailableBytes)}</dd>
              </div>
              <div className="flex justify-between gap-4">
                <dt className="text-muted-foreground">{t("assets.storage.diskTotal")}</dt>
                <dd>{formatBytes(data.diskTotalBytes)}</dd>
              </div>
            </dl>
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}
