import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Loader2, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { explainError } from "@/lib/errors";
import type { VectorRebuildPreflight, VectorRebuildProgress, VectorStatus } from "@/types";
import { toast } from "@/components/Toaster";
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
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
function formatBytes(value: number) {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let amount = value;
  let unit = 0;
  while (amount >= 1024 && unit < units.length - 1) {
    amount /= 1024;
    unit += 1;
  }
  return `${amount.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}
interface Props {
  libraryId?: string;
  libraryName?: string;
}
export default function SearchIndexSection({ libraryId, libraryName }: Props) {
  const { t } = useTranslation();
  const [status, setStatus] = useState<VectorStatus | null>(null);
  const [preflight, setPreflight] = useState<VectorRebuildPreflight | null>(null);
  const [progress, setProgress] = useState<VectorRebuildProgress | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    setPreflight(null);
    setProgress(null);
    if (!libraryId) {
      setStatus(null);
      return;
    }
    let disposed = false;
    const stops: Array<() => void> = [];
    void api
      .vectorStatus()
      .then((next) => {
        if (!disposed && next.libraryId === libraryId) setStatus(next);
      })
      .catch(() => {});
    void Promise.all([
      listen<VectorStatus>("vector://status", ({ payload }) => {
        if (payload.libraryId !== libraryId) return;
        setStatus(payload);
        if (payload.state !== "rebuilding") setProgress(null);
      }),
      listen<VectorRebuildProgress>("vector://rebuild-progress", ({ payload }) => {
        if (payload.libraryId === libraryId) setProgress(payload);
      }),
    ]).then((unlisteners) => {
      if (disposed) unlisteners.forEach((stop) => stop());
      else stops.push(...unlisteners);
    });
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
    };
  }, [libraryId]);
  const inspect = async () => {
    setBusy(true);
    try {
      setPreflight(await api.vectorRebuildPreflight());
    } catch (error) {
      toast.error(t("common.operationFailed", { message: explainError(error) }));
    } finally {
      setBusy(false);
    }
  };
  const confirm = async () => {
    setBusy(true);
    setProgress(null);
    try {
      await api.vectorRebuildStart(true);
      setPreflight(null);
      setStatus(await api.vectorStatus());
    } catch (error) {
      toast.error(t("common.operationFailed", { message: explainError(error) }));
    } finally {
      setBusy(false);
    }
  };
  const rebuilding = status?.state === "rebuilding";
  const percent = progress?.total ? Math.round((progress.completed * 100) / progress.total) : 0;
  return (
    <div className="space-y-2 border-t pt-4">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <Label>{t("settings.searchIndex")}</Label>
          <p className="text-xs leading-relaxed text-muted-foreground">
            {libraryId
              ? t("settings.searchIndexDesc", { library: libraryName })
              : t("settings.searchIndexNeedsLibrary")}
          </p>
        </div>
        {rebuilding ? (
          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              void api.vectorRebuildCancel().catch((error) => toast.error(explainError(error)))
            }
          >
            {t("common.cancel")}
          </Button>
        ) : (
          <Button
            variant="outline"
            size="sm"
            disabled={!libraryId || busy}
            title={libraryId ? undefined : t("settings.searchIndexNeedsLibrary")}
            onClick={() => void inspect()}
          >
            {busy ? <Loader2 className="size-4 animate-spin" /> : <RefreshCw className="size-4" />}
            {t("settings.searchIndexRebuild")}
          </Button>
        )}
      </div>

      {rebuilding && (
        <div className="space-y-1.5">
          <div className="h-1.5 overflow-hidden rounded-full bg-muted">
            <div className="h-full bg-primary transition-all" style={{ width: `${percent}%` }} />
          </div>
          <p className="text-xs tabular-nums text-muted-foreground">
            {t("vectorRecovery.rebuilding", {
              done: progress?.completed ?? 0,
              total: progress?.total ?? 0,
            })}
          </p>
        </div>
      )}

      <AlertDialog
        open={preflight !== null}
        onOpenChange={(next) => {
          if (!next) setPreflight(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.searchIndexConfirmTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("vectorRecovery.confirmDescription")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          {preflight && (
            <>
              <dl className="grid grid-cols-[max-content_minmax(0,1fr)] gap-2 rounded-md border p-3 text-sm">
                <dt className="whitespace-nowrap pr-4 text-muted-foreground">
                  {t("vectorRecovery.images")}
                </dt>
                <dd className="text-right tabular-nums">{preflight.frameCount}</dd>
                <dt className="text-muted-foreground">{t("settings.searchIndexCurrentSize")}</dt>
                <dd className="text-right tabular-nums">
                  {formatBytes(preflight.currentIndexBytes)}
                </dd>
                <dt className="text-muted-foreground">{t("vectorRecovery.required")}</dt>
                <dd className="text-right tabular-nums">
                  {formatBytes(preflight.estimatedRequiredBytes)}
                </dd>
                <dt className="text-muted-foreground">{t("vectorRecovery.available")}</dt>
                <dd className="text-right tabular-nums">{formatBytes(preflight.availableBytes)}</dd>
              </dl>
              {!preflight.canStart && (
                <p className="text-sm text-destructive">{t("vectorRecovery.insufficientSpace")}</p>
              )}
            </>
          )}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              disabled={busy || !preflight?.canStart}
              onClick={(event) => {
                event.preventDefault();
                void confirm();
              }}
            >
              {busy && <Loader2 className="size-4 animate-spin" />}
              {t("vectorRecovery.confirm")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
