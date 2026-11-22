import { queryKeys } from "@/lib/queryKeys";
import { isWindows } from "@/lib/platform";
/** Trash masonry shows retention badges, expiry highlighting, restoration, and confirmed
 * permanent deletion for individual or all assets.
 */
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { convertFileSrc } from "@tauri-apps/api/core";
import { assetPosterPath } from "@/lib/assetPreview";
import { Loader2, RotateCcw, Trash2 } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/components/Toaster";
import { useAppStore } from "@/stores/app";
import { cn } from "@/lib/utils";
import { useAssetPages } from "@/hooks/useAssetPages";
import MasonryGrid from "@/components/assets/MasonryGrid";
import { Button } from "@/components/ui/button";
import { explainError } from "@/lib/errors";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
export default function TrashPage() {
  const { t } = useTranslation();
  const libraryId = useAppStore((state) => state.current?.id);
  const queryClient = useQueryClient();
  const retention = useAppStore((s) => s.settings.trashRetentionDays);
  const { items, total, isLoading, isFetchingNextPage, fetchNextPage, hasNextPage } = useAssetPages(
    { inTrash: true },
  );
  const [purgeAllOpen, setPurgeAllOpen] = useState(false);
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
    void queryClient.invalidateQueries({ queryKey: queryKeys.folders.library(libraryId) });
  };
  const restore = async (ids: number[]) => {
    try {
      await api.assetRestore(ids);
      refresh();
      toast.success(t("common.operationSuccess"));
    } catch (e) {
      toast.error(explainError(e));
    }
  };
  const purge = async (ids: number[]) => {
    try {
      await api.assetDeletePermanent(ids);
    } catch (e) {
      toast.error(explainError(e));
    } finally {
      refresh();
    }
  };
  const purgeAll = async () => {
    try {
      await api.trashPurgeAll();
      setPurgeAllOpen(false);
      toast.success(t("common.operationSuccess"));
    } catch (e) {
      toast.error(explainError(e));
    } finally {
      refresh();
    }
  };
  /** Days until automatic deletion; negative means expired and awaiting cleanup. */
  const daysLeft = (deletedAt: number) => {
    const elapsedDays = Math.floor((Date.now() / 1000 - deletedAt) / 86400);
    return retention - elapsedDays;
  };
  return (
    <div className="flex h-full flex-col">
      <header
        data-tauri-drag-region={isWindows ? undefined : true}
        data-page-toolbar
        className="flex items-center gap-3 border-b px-6 py-3"
      >
        <h1 data-tauri-drag-region={isWindows ? undefined : true} className="text-lg font-semibold">
          {t("trash.title")}
        </h1>
        {total > 0 && (
          <span className="text-sm text-muted-foreground">
            {t("assets.total", { count: total })}
          </span>
        )}
        <div data-tauri-drag-region={isWindows ? undefined : true} className="h-8 flex-1" />
        {total > 0 && (
          <>
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                void restore(items.map((a) => a.id));
              }}
            >
              <RotateCcw className="h-4 w-4" />
              {t("trash.restoreAll")}
            </Button>
            <Button
              variant="outline"
              size="sm"
              className="text-destructive"
              onClick={() => setPurgeAllOpen(true)}
            >
              <Trash2 className="h-4 w-4" />
              {t("trash.purgeAll")}
            </Button>
          </>
        )}
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        {isLoading ? (
          <div className="flex h-40 items-center justify-center text-muted-foreground">
            <Loader2 className="mr-2 h-5 w-5 animate-spin" />
            {t("common.loading")}
          </div>
        ) : total === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 text-center">
            <div className="flex h-20 w-20 items-center justify-center rounded-full bg-muted">
              <Trash2 className="h-9 w-9 text-muted-foreground" />
            </div>
            <div>
              <div className="font-medium">{t("trash.empty")}</div>
              <div className="mt-1 text-sm text-muted-foreground">{t("trash.emptyDesc")}</div>
            </div>
          </div>
        ) : (
          <MasonryGrid
            items={items}
            onLoadMore={() => {
              void fetchNextPage();
            }}
            hasMore={hasNextPage}
            isLoadingMore={isFetchingNextPage}
            renderItem={(asset) => {
              const left = asset.deletedAt ? daysLeft(asset.deletedAt) : retention;
              const posterPath = assetPosterPath(asset);
              return (
                <div className="group relative overflow-hidden rounded-lg border bg-muted opacity-80">
                  {posterPath ? (
                    <img
                      src={convertFileSrc(posterPath)}
                      alt={asset.displayName ?? asset.fileName}
                      loading="eager"
                      className="block w-full"
                      draggable={false}
                    />
                  ) : (
                    <div
                      className="flex aspect-video items-center justify-center text-muted-foreground"
                      aria-label={asset.displayName ?? asset.fileName}
                    >
                      {asset.fileName}
                    </div>
                  )}
                  {/* Days-remaining badge. */}
                  <div
                    className={cn(
                      "absolute left-1.5 top-1.5 rounded px-1.5 py-0.5 text-[10px] font-medium text-white",
                      left <= 3 ? "bg-destructive" : "bg-black/60",
                    )}
                  >
                    {left <= 3 ? t("trash.expiringSoon") : t("trash.autoDeleteIn", { days: left })}
                  </div>
                  <div className="px-2 py-1.5">
                    <div className="truncate text-xs text-muted-foreground">
                      {asset.displayName ?? asset.fileName}
                    </div>
                  </div>
                  {/* Hover actions. */}
                  <div className="absolute inset-x-0 bottom-0 hidden justify-center gap-2 bg-gradient-to-t from-black/60 to-transparent p-2 group-hover:flex">
                    <Button
                      variant="secondary"
                      size="sm"
                      className="h-7 text-xs"
                      onClick={() => {
                        void restore([asset.id]);
                      }}
                    >
                      <RotateCcw className="h-3.5 w-3.5" />
                      {t("trash.restore")}
                    </Button>
                    <Button
                      variant="destructive"
                      size="sm"
                      className="h-7 text-xs"
                      onClick={() => {
                        void purge([asset.id]);
                      }}
                    >
                      {t("trash.purge")}
                    </Button>
                  </div>
                </div>
              );
            }}
          />
        )}
      </div>

      <AlertDialog open={purgeAllOpen} onOpenChange={setPurgeAllOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("trash.purgeAll")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("trash.purgeConfirm", { count: total })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
              onClick={() => void purgeAll()}
            >
              {t("common.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
