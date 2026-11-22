import { queryKeys } from "@/lib/queryKeys";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { ArrowRightLeft } from "lucide-react";
import { api } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import { toast } from "@/components/Toaster";
import type { Asset, OperationProgress } from "@/types";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { OperationStatus } from "@/components/library/OperationStatus";
import { explainError } from "@/lib/errors";
export function MoveToLibraryDialog({
  assets,
  open,
  onOpenChange,
}: {
  assets: Asset[];
  open: boolean;
  onOpenChange: (v: boolean) => void;
}) {
  const { t } = useTranslation();
  const libraryId = useAppStore((state) => state.current?.id);
  const queryClient = useQueryClient();
  const { libraries, current } = useAppStore();
  const [target, setTarget] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<OperationProgress | null>(null);
  useEffect(() => {
    const unlisten = listen<OperationProgress>("operation://progress", ({ payload }) => {
      if (payload.operationType === "move_assets") setProgress(payload);
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);
  const submit = async () => {
    if (!target) return;
    setProgress(null);
    setBusy(true);
    try {
      await api.assetMoveToLibrary(
        assets.map((a) => a.id),
        target,
      );
      void queryClient.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
      toast.success(t("common.operationSuccess"));
      onOpenChange(false);
    } catch (e) {
      toast.error(`${explainError(e)} ${t("library.errors.moveRetry")}`);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open={open} onOpenChange={(next) => !busy && onOpenChange(next)}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t("assets.moveToLibrary")}</DialogTitle>
          <DialogDescription>
            {assets.length > 1
              ? t("assets.selection", { count: assets.length })
              : (assets[0]?.displayName ?? assets[0]?.fileName)}
          </DialogDescription>
        </DialogHeader>
        {progress && progress.status !== "completed" && <OperationStatus progress={progress} />}
        <div className="space-y-2">
          <Label>{t("library.target")}</Label>
          <Select value={target} onValueChange={setTarget}>
            <SelectTrigger className="w-full">
              <SelectValue placeholder={t("library.target")} />
            </SelectTrigger>
            <SelectContent>
              {libraries
                .filter((l) => l.id !== current?.id)
                .map((l) => (
                  <SelectItem key={l.id} value={l.id}>
                    {l.name}
                  </SelectItem>
                ))}
            </SelectContent>
          </Select>
        </div>
        <DialogFooter>
          <Button size="sm" disabled={!target || busy} onClick={() => void submit()}>
            <ArrowRightLeft className="mr-1 h-4 w-4" />
            {busy ? t("common.loading") : t("common.confirm")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
