import { useEffect, useState } from "react";
import { FolderOpen, Images, List, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import { useImportStore } from "@/stores/import";
import type { ImportRecoveryJob } from "@/types";
import { toast } from "@/components/Toaster";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { explainError } from "@/lib/errors";
function leaf(path: string) {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}
export default function ImportRecoveryDialog() {
  const { t } = useTranslation();
  const libraryId = useAppStore((state) => state.current?.id);
  const [jobs, setJobs] = useState<ImportRecoveryJob[]>([]);
  const [showTasks, setShowTasks] = useState(false);
  const [busy, setBusy] = useState<"resume" | "abandon" | null>(null);
  useEffect(() => {
    if (!libraryId) return;
    let cancelled = false;
    api
      .importRecoveryList()
      .then((items) => {
        if (!cancelled) setJobs(items);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [libraryId]);
  const resume = async () => {
    const pendingJobs = jobs;
    setBusy("resume");
    // Close the recovery dialog before background scanning so slow preflight does not block
    // the main layout; failed submissions reappear.
    setJobs([]);
    setShowTasks(false);
    const remaining: ImportRecoveryJob[] = [];
    for (const job of pendingJobs) {
      // Recovery bypasses ordinary drag/drop dispatch, so seed the in-memory queue with its
      // persistent job ID before backend completion events arrive.
      useImportStore.getState().restoreTask(job);
      try {
        const accepted = await api.importRecoveryResume(job.id);
        // Already completed recovery requests are idempotent successes; remove their queue
        // entry instead of leaving phantom active work.
        if (accepted.alreadyResolved) {
          useImportStore.getState().deleteTask(job.id);
        }
      } catch (error) {
        remaining.push(job);
        useImportStore.getState().failTask(job.id, explainError(error));
        toast.error(explainError(error));
      }
    }
    if (remaining.length > 0) {
      // Refresh persistent jobs after failed preflight rather than restoring a stale dialog
      // snapshot for an already-removed job.
      try {
        setJobs(await api.importRecoveryList());
      } catch {
        setJobs(remaining);
      }
    } else {
      toast.success(t("importRecovery.resumed"));
    }
    setBusy(null);
  };
  const abandon = async () => {
    setBusy("abandon");
    try {
      await Promise.all(jobs.map((job) => api.importRecoveryAbandon(job.id)));
      setJobs([]);
      toast.success(t("importRecovery.abandoned"));
    } catch (error) {
      toast.error(explainError(error));
    } finally {
      setBusy(null);
    }
  };
  return (
    <Dialog open={jobs.length > 0} onOpenChange={() => {}}>
      <DialogContent showCloseButton={false} className="max-w-xl">
        <DialogHeader>
          <DialogTitle>{t("importRecovery.title")}</DialogTitle>
        </DialogHeader>
        <p className="text-sm text-muted-foreground">
          {t("importRecovery.description", { count: jobs.length })}
        </p>

        {showTasks && (
          <div className="max-h-72 space-y-2 overflow-y-auto rounded-md border p-2">
            {jobs.map((job) => (
              <div
                key={job.id}
                className="flex items-start gap-3 rounded-md px-2 py-2 hover:bg-accent/50"
              >
                {job.mode === "folders" ? (
                  <FolderOpen className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
                ) : (
                  <Images className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
                )}
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium" title={job.paths.join("\n")}>
                    {leaf(job.paths[0] ?? "")}
                    {job.paths.length > 1 ? ` (+${job.paths.length - 1})` : ""}
                  </div>
                  <div className="mt-0.5 text-xs text-muted-foreground">
                    {t(
                      job.mode === "folders"
                        ? "importRecovery.folderTask"
                        : "importRecovery.fileTask",
                    )}
                    {job.total > 0 ? ` · ${job.done}/${job.total}` : ""}
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}

        <div className="flex items-center justify-end gap-2 pt-2">
          <Button
            variant="ghost"
            disabled={busy != null}
            onClick={() => setShowTasks((value) => !value)}
          >
            <List className="h-4 w-4" />
            {t("importRecovery.viewTasks")}
          </Button>
          <Button variant="outline" disabled={busy != null} onClick={() => void abandon()}>
            {busy === "abandon" && <Loader2 className="h-4 w-4 animate-spin" />}
            {t("importRecovery.abandon")}
          </Button>
          <Button disabled={busy != null} onClick={() => void resume()}>
            {busy === "resume" && <Loader2 className="h-4 w-4 animate-spin" />}
            {t("importRecovery.resume")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
