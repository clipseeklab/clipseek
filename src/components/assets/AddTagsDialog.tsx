import { queryKeys } from "@/lib/queryKeys";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { X } from "lucide-react";
import { api } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import { toast } from "@/components/Toaster";
import type { Asset } from "@/types";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import AssetTagEditor from "@/components/assets/AssetTagEditor";
import { explainError } from "@/lib/errors";
export function AddTagsDialog({
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
  const [removedTags, setRemovedTags] = useState<Record<number, string[]>>({});
  const [removing, setRemoving] = useState(false);
  const removingRef = useRef(false);
  const targetKey = assets.map((asset) => asset.id).join(",");
  useEffect(() => {
    setRemovedTags({});
  }, [open, targetKey]);
  const existingTags = new Set<string>();
  for (const asset of assets) {
    for (const name of asset.tags) {
      if (!removedTags[asset.id]?.includes(name)) existingTags.add(name);
    }
  }
  const removeTag = async (name: string) => {
    if (removingRef.current) return;
    removingRef.current = true;
    setRemoving(true);
    try {
      for (const asset of assets) {
        if (!asset.tags.includes(name) || removedTags[asset.id]?.includes(name)) continue;
        await api.assetRemoveTag(asset.id, name);
        setRemovedTags((current) => ({
          ...current,
          [asset.id]: [...(current[asset.id] ?? []), name],
        }));
      }
    } catch (error) {
      toast.error(explainError(error));
    } finally {
      void queryClient.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.tags.library(libraryId) });
      for (const asset of assets)
        void queryClient.invalidateQueries({
          queryKey: queryKeys.assetDetail.item(libraryId, asset.id),
        });
      removingRef.current = false;
      setRemoving(false);
    }
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent disableAnimation className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t("assets.addTags")}</DialogTitle>
          <DialogDescription>
            {assets.length > 1
              ? t("assets.selection", { count: assets.length })
              : (assets[0]?.displayName ?? assets[0]?.fileName)}
          </DialogDescription>
        </DialogHeader>
        <section aria-label={t("assets.existingTags")} className="space-y-2">
          <div className="text-sm font-medium">{t("assets.existingTags")}</div>
          <div className="flex flex-wrap gap-1.5">
            {[...existingTags].map((name) => (
              <button
                key={name}
                type="button"
                disabled={removing}
                aria-label={`${t("common.delete")} ${name}`}
                onClick={() => void removeTag(name)}
                className="inline-flex max-w-full items-center gap-1 rounded-md bg-secondary px-2 py-1 text-xs text-secondary-foreground hover:bg-destructive hover:text-destructive-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
              >
                <span className="truncate">{name}</span>
                <X className="h-3 w-3 shrink-0" />
              </button>
            ))}
            {!existingTags.size && (
              <span className="text-sm text-muted-foreground">{t("common.none")}</span>
            )}
          </div>
        </section>
        <div className="space-y-2">
          <Label>{t("assets.tags")}</Label>
          <AssetTagEditor
            assets={assets}
            autoFocus
            onAdded={() => {
              toast.success(t("common.operationSuccess"));
              onOpenChange(false);
            }}
          />
        </div>
      </DialogContent>
    </Dialog>
  );
}
