import { useAppStore } from "@/stores/app";
import { queryKeys } from "@/lib/queryKeys";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { Loader2, ScanSearch } from "lucide-react";
import { api } from "@/lib/api";
import { explainError } from "@/lib/errors";
import type { IndexScope } from "@/types";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { toast } from "@/components/Toaster";
import ModelDownloadGate from "@/components/ModelDownloadGate";
export default function IndexBuildButton({
  scope,
  disabled,
  label,
  open: controlledOpen,
  onOpenChange,
}: {
  scope: IndexScope;
  disabled?: boolean;
  visualOnly?: boolean;
  subtitleOnly?: boolean;
  label?: string;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const libraryId = useAppStore((state) => state.current?.id);
  const [internalOpen, setInternalOpen] = useState(false);
  const open = controlledOpen ?? internalOpen;
  const setOpen = onOpenChange ?? setInternalOpen;
  const [busy, setBusy] = useState(false);
  const [needsModels, setNeedsModels] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    if (open) {
      setError("");
      setNeedsModels(false);
    }
  }, [open]);
  const build = async () => {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      if (!(await api.modelStatus()).ready) {
        setNeedsModels(true);
        return;
      }
      if (useAppStore.getState().current?.id !== libraryId) {
        setOpen(false);
        return;
      }
      const count = await api.indexBuild(scope, ["visual"]);
      toast.success(
        count ? t("indexManagement.queuedCount", { count }) : t("indexManagement.nothingToBuild"),
      );
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.indexTasks.library(libraryId) }),
        queryClient.invalidateQueries({ queryKey: queryKeys.indexSummary.library(libraryId) }),
      ]);
      setOpen(false);
    } catch (cause) {
      setError(explainError(cause));
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      {controlledOpen === undefined && (
        <Button
          variant="outline"
          size="sm"
          title={label ? t("indexManagement.build") : undefined}
          disabled={disabled}
          onClick={() => setOpen(true)}
        >
          <ScanSearch className="h-4 w-4" />
          {label ?? t("indexManagement.build")}
        </Button>
      )}
      <Dialog
        open={open}
        onOpenChange={(next) => {
          if (!busy) setOpen(next);
        }}
      >
        <DialogContent className={needsModels ? "max-w-xl" : "max-w-md"}>
          <DialogHeader>
            <DialogTitle>{t("indexManagement.build")}</DialogTitle>
            <DialogDescription>{t("indexManagement.visualBuildHint")}</DialogDescription>
          </DialogHeader>
          {needsModels ? (
            <ModelDownloadGate embedded onReady={() => setNeedsModels(false)} />
          ) : (
            <>
              {error && (
                <p role="alert" className="text-sm text-destructive">
                  {error}
                </p>
              )}
              <DialogFooter>
                <Button variant="outline" disabled={busy} onClick={() => setOpen(false)}>
                  {t("common.cancel")}
                </Button>
                <Button disabled={busy} onClick={() => void build()}>
                  {busy && <Loader2 className="h-4 w-4 animate-spin" />}
                  {t("indexManagement.start")}
                </Button>
              </DialogFooter>
            </>
          )}
        </DialogContent>
      </Dialog>
    </>
  );
}
