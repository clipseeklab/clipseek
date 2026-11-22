import { queryKeys } from "@/lib/queryKeys";
import { useAppStore } from "@/stores/app";
import AssetMetadata from "./AssetMetadata";
import { useAssetDetailPanel } from "@/hooks/useAssetDetailPanel";
import AssetIndexStatus from "@/components/indexing/AssetIndexStatus";
import IndexBuildButton from "@/components/indexing/IndexBuildButton";
import {
  Check,
  FolderInput,
  Link2,
  Loader2,
  Maximize2,
  Pencil,
  Trash2,
  Unlink,
  X,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import AssetTagEditor from "@/components/assets/AssetTagEditor";
import { Input } from "@/components/ui/input";
import { revealManagedAsset } from "@/components/assets/ManagedAssetReveal";
import { explainError } from "@/lib/errors";
import { isWindows } from "@/lib/platform";
import PlaybackVideo from "@/components/assets/PlaybackVideo";
import VideoCoverDialog from "@/components/assets/VideoCoverDialog";
import { AssetSourcePreview } from "./AssetSourcePreview";
import { MoveToFolderDialog } from "./MoveToFolderDialog";
function stageText(stage: string): string | null {
  return stage.startsWith("error") ? stage.slice(6) : null;
}
export function AssetDetailPanel(props: Parameters<typeof useAssetDetailPanel>[0]) {
  const libraryId = useAppStore((state) => state.current?.id);
  const state = useAssetDetailPanel(props);
  const {
    previewAssets,
    open,
    onOpenChange,
    onTrash,
    t,
    queryClient,
    previewUrl,
    moveFolderOpen,
    setMoveFolderOpen,
    coverOpen,
    setCoverOpen,
    sourcePreviewOpen,
    setSourcePreviewOpen,
    previewStartMs,
    setPreviewStartMs,
    inlineVideoRef,
    editingTitle,
    setEditingTitle,
    titleDraft,
    setTitleDraft,
    panelWidth,
    resizeHandle,
    detail,
    isLoading,
    isError,
    error,
    refetch,
    removeTag,
    saveTitle,
  } = state;
  if (!open) return null;
  if (isError && !detail) {
    return (
      <aside
        className="relative flex h-full shrink-0 items-center justify-center overflow-hidden bg-card p-4"
        style={{
          width: panelWidth,
          minWidth: panelWidth,
          maxWidth: panelWidth,
          flexBasis: panelWidth,
        }}
      >
        {resizeHandle}
        <div role="alert" className="flex min-w-0 flex-col items-center gap-3 text-center">
          <p className="text-sm">{t("assets.loadFailed")}</p>
          <p className="break-all text-sm text-destructive">{explainError(error)}</p>
          <Button size="sm" onClick={() => void refetch()}>
            {t("common.retry")}
          </Button>
          <Button size="sm" variant="ghost" onClick={() => onOpenChange(false)}>
            {t("common.close")}
          </Button>
        </div>
      </aside>
    );
  }
  if (!detail || isLoading) {
    return (
      <aside
        className="relative flex h-full shrink-0 items-center justify-center overflow-hidden bg-card"
        style={{
          width: panelWidth,
          minWidth: panelWidth,
          maxWidth: panelWidth,
          flexBasis: panelWidth,
        }}
      >
        {resizeHandle}
        <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
      </aside>
    );
  }
  const err = stageText(detail.importStage);
  const isVideo = detail.kind === "video";
  return (
    <>
      <aside
        className="relative flex h-full min-w-0 shrink-0 flex-col overflow-hidden bg-card"
        style={{
          width: panelWidth,
          minWidth: panelWidth,
          maxWidth: panelWidth,
          flexBasis: panelWidth,
        }}
      >
        {resizeHandle}
        <div
          data-tauri-drag-region={isWindows ? undefined : true}
          className="flex h-12 items-center gap-2 px-4"
        >
          {editingTitle ? (
            <Input
              autoFocus
              aria-label={t("assets.editTitle")}
              className="h-8 min-w-0 flex-1"
              maxLength={500}
              value={titleDraft}
              onChange={(event) => setTitleDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void saveTitle();
                if (event.key === "Escape") {
                  setTitleDraft(detail.displayName ?? detail.fileName);
                  setEditingTitle(false);
                }
              }}
            />
          ) : (
            <div
              data-tauri-drag-region={isWindows ? undefined : true}
              className="min-w-0 flex-1 truncate text-sm font-semibold"
              title={detail.displayName ?? detail.fileName}
            >
              {detail.displayName ?? detail.fileName}
            </div>
          )}
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 shrink-0"
            title={t(editingTitle ? "common.save" : "assets.editTitle")}
            onClick={() => (editingTitle ? void saveTitle() : setEditingTitle(true))}
          >
            {editingTitle ? <Check className="h-4 w-4" /> : <Pencil className="h-4 w-4" />}
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 shrink-0"
            title={t("layout.closeDetails")}
            onClick={() => onOpenChange(false)}
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
        <div className="min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto">
          <div className="min-w-0 space-y-5 p-4">
            <div className="relative flex max-h-80 items-center justify-center overflow-hidden rounded-lg">
              {(isVideo || previewUrl) &&
                (isVideo ? (
                  <PlaybackVideo
                    ref={inlineVideoRef}
                    assetId={detail.id}
                    sourcePath={detail.absPath}
                    poster={previewUrl ?? undefined}
                    controls
                    className="max-h-80 w-full object-contain"
                  />
                ) : (
                  <button
                    type="button"
                    aria-label={t("assets.enlargePreview")}
                    className="relative block max-w-full cursor-zoom-in focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
                    onClick={() => setSourcePreviewOpen(true)}
                  >
                    <img
                      src={previewUrl ?? undefined}
                      alt={detail.displayName ?? detail.fileName}
                      className="max-h-80 max-w-full object-contain"
                    />
                    <span className="pointer-events-none absolute bottom-2 right-2">
                      <AssetIndexStatus
                        tasks={detail.processingTasks}
                        id={detail.id}
                        kind="visual"
                        indexed={detail.visualIndexed}
                        label=""
                      />
                    </span>
                  </button>
                ))}
              {isVideo && (
                <div className="absolute right-2 top-2 flex items-center gap-1">
                  {detail.deletedAt == null && (
                    <button
                      type="button"
                      onClick={() => setCoverOpen(true)}
                      className="rounded-md bg-black/60 px-2 py-1 text-xs text-white transition-colors hover:bg-black/80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white"
                    >
                      {t("assets.replaceCover")}
                    </button>
                  )}
                  <button
                    type="button"
                    aria-label={t("assets.enlargePreview")}
                    title={t("assets.enlargePreview")}
                    className="rounded-md bg-black/60 p-1 text-white transition-colors hover:bg-black/80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white"
                    onClick={() => {
                      setPreviewStartMs((inlineVideoRef.current?.currentTime ?? 0) * 1000);
                      inlineVideoRef.current?.pause();
                      setSourcePreviewOpen(true);
                    }}
                  >
                    <Maximize2 className="h-4 w-4" />
                  </button>
                </div>
              )}
              {/* Reference status: warn when the source is missing, otherwise show its storage badge. */}
              {detail.storage === "reference" && (
                <div
                  className={cn(
                    "absolute left-2 top-2 flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] font-medium",
                    detail.sourceMissing ? "bg-amber-500 text-amber-950" : "bg-black/60 text-white",
                  )}
                  title={t(
                    detail.sourceMissing ? "asset.sourceMissingTitle" : "asset.referenceTitle",
                  )}
                >
                  {detail.sourceMissing ? (
                    <Unlink className="h-3 w-3" />
                  ) : (
                    <Link2 className="h-3 w-3" />
                  )}
                  {t(detail.sourceMissing ? "asset.sourceMissing" : "asset.reference")}
                </div>
              )}
            </div>
            {!isVideo && !detail.visualIndexed && (
              <div className="flex justify-end">
                <IndexBuildButton
                  scope={{ assetIds: [detail.id] }}
                  visualOnly
                  disabled={detail.deletedAt != null}
                />
              </div>
            )}
            {/* Folder membership. */}
            <div className="space-y-2">
              <div className="text-sm font-medium">{t("assets.detailsFolder")}</div>
              <button
                type="button"
                className="flex w-full min-w-0 items-center gap-2 overflow-hidden rounded-md border bg-background px-3 py-2 text-left text-sm transition-colors hover:bg-accent"
                onClick={() => setMoveFolderOpen(true)}
              >
                <FolderInput className="h-4 w-4 shrink-0 text-muted-foreground" />
                <span className="min-w-0 flex-1 truncate">
                  {detail.folderName ?? t("assets.uncategorized")}
                </span>
                <span className="shrink-0 text-xs text-muted-foreground">
                  {detail.folderId == null ? t("assets.addToFolder") : t("assets.changeFolder")}
                </span>
              </button>
            </div>
            {/* Tag editing. */}
            <div className="space-y-2">
              <div className="text-sm font-medium">{t("assets.tags")}</div>
              <div className="flex flex-wrap gap-1.5">
                {detail.tags.map((tag) => (
                  <Badge
                    key={tag}
                    variant="secondary"
                    className="cursor-pointer pr-1 hover:bg-destructive hover:text-destructive-foreground"
                    title={t("common.delete")}
                    onClick={() => {
                      void removeTag(tag);
                    }}
                  >
                    {tag} ×
                  </Badge>
                ))}
                {detail.tags.length === 0 && (
                  <span className="text-xs text-muted-foreground">{t("common.none")}</span>
                )}
              </div>
              <AssetTagEditor assets={[detail]} />
            </div>

            {/* Internal index units. */}

            {/* Metadata. */}
            <AssetMetadata detail={detail} err={err} />
          </div>
        </div>
        <div className="flex gap-2 border-t p-3">
          <Button
            variant="outline"
            size="sm"
            className="flex-1"
            onClick={() => revealManagedAsset(detail.absPath, detail.storage === "reference")}
          >
            {t("assets.revealInFinder")}
          </Button>
          {onTrash && (
            <Button
              variant="outline"
              size="sm"
              className="text-destructive"
              aria-label={t("assets.trash")}
              onClick={() => {
                onTrash(detail);
                onOpenChange(false);
              }}
            >
              <Trash2 className="h-4 w-4" />
            </Button>
          )}
        </div>
      </aside>
      <MoveToFolderDialog
        assets={[detail]}
        open={moveFolderOpen}
        onOpenChange={(next) => setMoveFolderOpen(next)}
      />
      {isVideo && <></>}
      {isVideo && (
        <VideoCoverDialog
          key={detail.id}
          asset={detail}
          open={coverOpen}
          onOpenChange={setCoverOpen}
          onSaved={(updated) => {
            queryClient.setQueryData(queryKeys.assetDetail.item(libraryId, updated.id), {
              ...detail,
              ...updated,
            });
            void queryClient.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
          }}
        />
      )}
      <AssetSourcePreview
        asset={detail}
        assets={previewAssets}
        pagination={props.previewPagination}
        open={sourcePreviewOpen}
        onOpenChange={setSourcePreviewOpen}
        initialTimeMs={previewStartMs}
      />
    </>
  );
}
