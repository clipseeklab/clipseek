import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import { useState } from "react";
import {
  Copy,
  FileSearch,
  FolderInput,
  Info,
  SearchCode,
  Tags,
  Trash2,
  Layers,
  ScanSearch,
} from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/components/Toaster";
import type { Asset } from "@/types";
import {
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
} from "@/components/ui/context-menu";
import { revealManagedAsset } from "@/components/assets/ManagedAssetReveal";
import { explainError } from "@/lib/errors";
import IndexBuildButton from "@/components/indexing/IndexBuildButton";
export interface AssetMenuActions {
  onSimilar: (asset: Asset) => void;
  onAddTags: (asset: Asset) => void;
  onMove: (asset: Asset) => void;
  onBatch: (asset: Asset) => void;
  onDetails: (asset: Asset) => void;
  onTrash: (asset: Asset) => void;
}
export function AssetContextMenuContent({
  asset,
  actions,
  context = "asset",
}: {
  asset: Asset;
  actions: AssetMenuActions;
  context?: "asset" | "search-result";
}) {
  const { t } = useTranslation();
  const [indexBuildOpen, setIndexBuildOpen] = useState(false);
  const needsVisual = !asset.visualIndexed;
  const canBuildIndex = asset.deletedAt == null && needsVisual;
  const exportAsset = async () => {
    const dir = await openDialog({
      directory: true,
      multiple: false,
      title: t("assets.chooseExportFolder"),
    });
    if (typeof dir !== "string") return;
    try {
      await api.assetExport([asset.id], dir);
      toast.success(t("assets.exported"));
    } catch (error) {
      toast.error(explainError(error));
    }
  };
  return (
    <>
      <ContextMenuContent
        className="min-w-52"
        onCloseAutoFocus={(event) => {
          if (indexBuildOpen) event.preventDefault();
        }}
      >
        <ContextMenuItem onSelect={() => void exportAsset()}>
          <Copy />
          {t("assets.export")}
        </ContextMenuItem>
        {
          <ContextMenuItem onSelect={() => actions.onSimilar(asset)}>
            <SearchCode />
            {t("assets.findSimilar")}
          </ContextMenuItem>
        }
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={() => actions.onAddTags(asset)}>
          <Tags />
          {t("assets.addTags")}
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => actions.onMove(asset)}>
          <FolderInput />
          {t("assets.moveToFolder")}
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => actions.onBatch(asset)}>
          <Layers />
          {t("assets.batchActions")}
        </ContextMenuItem>
        {context === "asset" && (
          <ContextMenuItem disabled={!canBuildIndex} onSelect={() => setIndexBuildOpen(true)}>
            <ScanSearch />
            {t("indexManagement.build")}
          </ContextMenuItem>
        )}
        <ContextMenuSeparator />
        <ContextMenuItem
          onSelect={() => revealManagedAsset(asset.absPath, asset.storage === "reference")}
        >
          <FileSearch />
          {t("assets.revealInFinder")}
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => actions.onDetails(asset)}>
          <Info />
          {t("assets.details")}
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem variant="destructive" onSelect={() => actions.onTrash(asset)}>
          <Trash2 />
          {t("assets.trash")}
        </ContextMenuItem>
      </ContextMenuContent>
      <IndexBuildButton
        scope={{ assetIds: [asset.id] }}
        open={indexBuildOpen}
        onOpenChange={setIndexBuildOpen}
        visualOnly
      />
    </>
  );
}
