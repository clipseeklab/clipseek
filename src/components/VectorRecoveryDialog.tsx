import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { AlertTriangle, Loader2, RotateCcw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { explainError } from "@/lib/errors";
import { useAppStore } from "@/stores/app";
import type { VectorRebuildPreflight, VectorRebuildProgress, VectorStatus } from "@/types";
import { toast } from "@/components/Toaster";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
function bytes(value: number) {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let amount = value;
  let unit = 0;
  while (amount >= 1024 && unit < units.length - 1) {
    amount /= 1024;
    unit += 1;
  }
  return `${amount.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}
export default function VectorRecoveryDialog() {
  const { t } = useTranslation();
  const libraryId = useAppStore((state) => state.current?.id);
  const [status, setStatus] = useState<VectorStatus | null>(null);
  const [preflight, setPreflight] = useState<VectorRebuildPreflight | null>(null);
  const [progress, setProgress] = useState<VectorRebuildProgress | null>(null);
  const [busy, setBusy] = useState(false);
  const [reviewOpen, setReviewOpen] = useState(true);
  const [repairContext, setRepairContext] = useState(false);
  useEffect(() => {
    if (!libraryId) {
      setStatus(null);
      return;
    }
    setReviewOpen(true);
    setRepairContext(false);
    let disposed = false;
    const stops: Array<() => void> = [];
    void api
      .vectorStatus()
      .then((next) => {
        if (disposed) return;
        setStatus(next);
        if (next.state === "rebuild_pending") setRepairContext(true);
      })
      .catch(() => {});
    void Promise.all([
      listen<VectorStatus>("vector://status", ({ payload }) => {
        if (payload.libraryId !== libraryId) return;
        setStatus(payload);
        if (payload.state === "rebuild_pending") setRepairContext(true);
      }),
      listen<VectorStatus>("vector://rebuild-done", ({ payload }) => {
        if (payload.libraryId !== libraryId) return;
        setStatus(payload);
        setProgress(null);
        setPreflight(null);
        if (payload.failedFrames)
          toast.persistentError(t("vectorRecovery.partialNotice", { count: payload.failedFrames }));
        else toast.persistentSuccess(t("vectorRecovery.doneNotice"));
      }),
      listen<VectorStatus>("vector://rebuild-failed", ({ payload }) => {
        if (payload.libraryId !== libraryId) return;
        setStatus(payload);
        setProgress(null);
        toast.error(t("vectorRecovery.failed"));
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
  }, [libraryId, t]);
  const retry = async () => {
    setBusy(true);
    try {
      setStatus(await api.vectorRetryOpen());
    } catch (error) {
      toast.error(explainError(error));
    } finally {
      setBusy(false);
    }
  };
  const inspect = async () => {
    setBusy(true);
    try {
      setPreflight(await api.vectorRebuildPreflight());
    } catch (error) {
      toast.error(explainError(error));
    } finally {
      setBusy(false);
    }
  };
  const confirm = async () => {
    setBusy(true);
    try {
      await api.vectorRebuildStart(true);
      setStatus(await api.vectorStatus());
    } catch (error) {
      toast.error(explainError(error));
    } finally {
      setBusy(false);
    }
  };
  if (!status || status.state === "ready" || (status.state === "rebuilding" && !repairContext))
    return null;
  const rebuilding = status.state === "rebuilding";
  const percent = progress?.total ? Math.round((progress.completed * 100) / progress.total) : 0;
  return (
    <>
      {!reviewOpen && (
        <button
          className="fixed right-4 top-4 z-[80] flex items-center gap-2 rounded-md border border-amber-500/40 bg-card px-3 py-2 text-sm shadow-lg"
          onClick={() => setReviewOpen(true)}
        >
          <AlertTriangle className="h-4 w-4 text-amber-500" />
          {t(rebuilding ? "vectorRecovery.rebuildingShort" : "vectorRecovery.openRepair")}
        </button>
      )}
      <Dialog open={reviewOpen} onOpenChange={setReviewOpen}>
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <AlertTriangle className="h-5 w-5 text-amber-500" />
              {t("vectorRecovery.title")}
            </DialogTitle>
          </DialogHeader>
          {rebuilding ? (
            <>
              <p className="text-sm text-muted-foreground">
                {t("vectorRecovery.rebuilding", {
                  done: progress?.completed ?? 0,
                  total: progress?.total ?? 0,
                })}
              </p>
              <div className="h-2 overflow-hidden rounded-full bg-muted">
                <div
                  className="h-full bg-primary transition-all"
                  style={{ width: `${percent}%` }}
                />
              </div>
              <div className="flex justify-end">
                <Button variant="outline" onClick={() => void api.vectorRebuildCancel()}>
                  {t("common.cancel")}
                </Button>
              </div>
            </>
          ) : preflight ? (
            <>
              <p className="text-sm">{t("vectorRecovery.confirmDescription")}</p>
              <dl className="grid grid-cols-[max-content_minmax(0,1fr)] gap-2 rounded-md border p-3 text-sm">
                <dt className="whitespace-nowrap pr-4 text-muted-foreground">
                  {t("vectorRecovery.images")}
                </dt>
                <dd>{preflight.frameCount}</dd>
                <dt className="text-muted-foreground">{t("vectorRecovery.required")}</dt>
                <dd>{bytes(preflight.estimatedRequiredBytes)}</dd>
                <dt className="text-muted-foreground">{t("vectorRecovery.available")}</dt>
                <dd>{bytes(preflight.availableBytes)}</dd>
              </dl>
              {!preflight.canStart && (
                <p className="text-sm text-destructive">{t("vectorRecovery.insufficientSpace")}</p>
              )}
              <div className="flex justify-end gap-2">
                <Button variant="outline" onClick={() => setPreflight(null)}>
                  {t("common.back")}
                </Button>
                <Button disabled={busy || !preflight.canStart} onClick={() => void confirm()}>
                  {busy && <Loader2 className="h-4 w-4 animate-spin" />}
                  {t("vectorRecovery.confirm")}
                </Button>
              </div>
            </>
          ) : (
            <>
              <p className="text-sm text-muted-foreground">{t("vectorRecovery.description")}</p>
              {status.message && (
                <p className="max-h-24 overflow-auto rounded bg-muted p-2 font-mono text-xs">
                  {status.code}: {status.message}
                </p>
              )}
              <div className="flex justify-end gap-2">
                <Button variant="outline" disabled={busy} onClick={() => void retry()}>
                  <RotateCcw className="h-4 w-4" />
                  {t("vectorRecovery.retry")}
                </Button>
                <Button disabled={busy} onClick={() => void inspect()}>
                  {busy && <Loader2 className="h-4 w-4 animate-spin" />}
                  {t("vectorRecovery.rebuild")}
                </Button>
              </div>
            </>
          )}
        </DialogContent>
      </Dialog>
    </>
  );
}
