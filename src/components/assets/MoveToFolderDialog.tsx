import { queryKeys } from "@/lib/queryKeys";
import FolderField from "@/components/metadata/FolderField";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { FolderInput } from "lucide-react";
import { api } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import { toast } from "@/components/Toaster";
import type { Asset } from "@/types";
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
import { explainError } from "@/lib/errors";
export function MoveToFolderDialog({
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
  const [folder, setFolder] = useState<number | null>(null);
  const initialFolder =
    assets.length && assets.every((asset) => asset.folderId === assets[0].folderId)
      ? assets[0].folderId
      : null;
  useEffect(() => {
    if (open) setFolder(initialFolder);
  }, [open, initialFolder]);
  const submit = async () => {
    try {
      await api.assetMoveToFolder(
        assets.map((a) => a.id),
        folder,
      );
      void queryClient.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.folders.library(libraryId) });
      assets.forEach(
        (asset) =>
          void queryClient.invalidateQueries({
            queryKey: queryKeys.assetDetail.item(libraryId, asset.id),
          }),
      );
      toast.success(t("common.operationSuccess"));
      onOpenChange(false);
    } catch (e) {
      toast.error(explainError(e));
    }
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t("assets.moveToFolder")}</DialogTitle>
          <DialogDescription>
            {assets.length > 1
              ? t("assets.selection", { count: assets.length })
              : (assets[0]?.displayName ?? assets[0]?.fileName)}
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-2">
          <Label>{t("assets.moveToFolder")}</Label>
          {open && <FolderField key={libraryId} value={folder} onChange={setFolder} />}
        </div>
        <DialogFooter>
          <Button size="sm" onClick={() => void submit()}>
            <FolderInput className="mr-1 h-4 w-4" />
            {t("common.confirm")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
