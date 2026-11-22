import { queryKeys } from "@/lib/queryKeys";
import { useAppStore } from "@/stores/app";
/** Masonry asset card with previews, video hover playback, tags, selection, and context actions. */
import { memo, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { convertFileSrc } from "@tauri-apps/api/core";
import { assetPosterPath } from "@/lib/assetPreview";
import { Film, Image as ImageIcon, Loader2, AlertTriangle, RotateCw, ZoomIn } from "lucide-react";
import { assetAspectRatio } from "@/lib/assetGridLayout";
import { api } from "@/lib/api";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "@/components/Toaster";
import type { Asset } from "@/types";
import { cn, formatDuration } from "@/lib/utils";
import { Checkbox } from "@/components/ui/checkbox";
import { ContextMenu, ContextMenuTrigger } from "@/components/ui/context-menu";
import { AssetContextMenuContent } from "@/components/assets/AssetContextMenu";
import AssetCardMetadata from "@/components/assets/AssetCardMetadata";
import HoverVideoPreview from "@/components/assets/HoverVideoPreview";
import { useAssetVideoCodec } from "@/hooks/useAssetVideoCodec";
import { explainError } from "@/lib/errors";
import {
  activeProcessing,
  currentProcessingTasks,
  needsProcessingAttention,
  visibleProcessingTasks,
} from "@/lib/processing";
interface Props {
  asset: Asset;
  selected: boolean;
  selectMode: boolean;
  onSelect: (id: number, additive: boolean) => void;
  onOpen: (asset: Asset) => void;
  onPreview: (asset: Asset) => void;
  onAddTags: (asset: Asset) => void;
  onMove: (asset: Asset) => void;
  onBatch: (asset: Asset) => void;
  onTrash: (asset: Asset) => void;
  onSimilar: (asset: Asset) => void;
}
function AssetCardInner({
  asset,
  selected,
  selectMode,
  onSelect,
  onOpen,
  onPreview,
  onAddTags,
  onMove,
  onBatch,
  onTrash,
  onSimilar,
}: Props) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const libraryId = useAppStore((state) => state.current?.id);
  const { data: videoCodec } = useAssetVideoCodec(asset);
  const [hover, setHover] = useState(false);
  const awaitingHeifPreview =
    asset.kind === "image" &&
    !asset.thumbPath &&
    (asset.mimeType === "image/heic" ||
      asset.mimeType === "image/heif" ||
      /\.hei[cf]$/i.test(asset.fileName));
  // Indexing can rewrite a thumbnail at the same path. A terminal generation
  // must retry the image URL and invalidate any in-flight fallback for the old file.
  const previewVersion = currentProcessingTasks(asset.processingTasks)
    .filter((task) => ["import", "visual"].includes(task.kind) && !activeProcessing(task))
    .map((task) => `${task.kind}-${task.id}-${task.status}`)
    .sort()
    .join("_");
  // Synchronous Tauri v2 asset:// conversion.
  const posterPath = assetPosterPath(asset);
  const src = useMemo(() => {
    const path = posterPath;
    if (awaitingHeifPreview || !path) return null;
    const url = convertFileSrc(path);
    return previewVersion
      ? `${url}${url.includes("?") ? "&" : "?"}preview=${encodeURIComponent(previewVersion)}`
      : url;
  }, [awaitingHeifPreview, posterPath, previewVersion]);
  const [previewSrc, setPreviewSrc] = useState(src);
  const fallbackStarted = useRef(false);
  const previewGeneration = useRef(0);
  const fallbackUrl = useRef<string | null>(null);
  useEffect(() => {
    if (fallbackUrl.current) URL.revokeObjectURL(fallbackUrl.current);
    fallbackUrl.current = null;
    fallbackStarted.current = false;
    setPreviewSrc(src);
    return () => {
      previewGeneration.current += 1;
      if (fallbackUrl.current) URL.revokeObjectURL(fallbackUrl.current);
    };
  }, [src]);
  const loadPreviewFallback = async () => {
    setPreviewSrc(null);
    if (fallbackStarted.current) return;
    fallbackStarted.current = true;
    const generation = previewGeneration.current;
    try {
      const bytes = await api.assetPreview(asset.id);
      if (generation !== previewGeneration.current) return;
      const blobUrl = URL.createObjectURL(
        new Blob([new Uint8Array(bytes)], {
          type: asset.thumbPath ? "image/webp" : asset.mimeType,
        }),
      );
      fallbackUrl.current = blobUrl;
      setPreviewSrc(blobUrl);
    } catch {
      // Automatic previews keep the placeholder on failure. Completion changes
      // the preview version and retries; per-card failures must not flood toasts.
    }
  };
  const retryImport = async () => {
    try {
      await api.importRetry([asset.id]);
      await queryClient.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
    } catch (err) {
      toast.error(explainError(err));
    }
  };
  // Video assets reuse the shared hover player.
  const canPreviewVideo = asset.kind === "video";
  const isGifPreview = asset.kind === "gif" && asset.fileName.toLowerCase().endsWith(".gif");
  const handleMouseEnter = () => {
    // Static images, especially SVG previews, need no React rerender on hover.
    if (canPreviewVideo || isGifPreview) setHover(true);
  };
  const handleMouseLeave = () => {
    setHover(false);
  };
  const error = asset.importStage.startsWith("error");
  const processing = !error && asset.importStage !== "done";
  const indexingTasks = visibleProcessingTasks(asset);
  const awaitingVideoPreview = asset.kind === "video" && !asset.thumbPath && processing;
  const extension = asset.fileName.split(".").pop()?.toUpperCase() || asset.kind.toUpperCase();
  // Share aspect ratios with the virtual grid so placeholders match real card heights.
  const aspectRatio = assetAspectRatio(asset);
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          className={cn(
            "group relative cursor-pointer overflow-hidden rounded-lg border bg-muted transition-colors hover:border-foreground/30",
            selected && "ring-2 ring-primary",
          )}
          onMouseEnter={handleMouseEnter}
          onMouseLeave={handleMouseLeave}
          onClick={(e) => {
            if (selectMode || e.metaKey || e.ctrlKey) onSelect(asset.id, true);
            else onOpen(asset);
          }}
        >
          {/* Image or video preview. */}
          <div className="relative w-full bg-muted" style={{ aspectRatio: aspectRatio }}>
            {awaitingVideoPreview ? (
              <div className="flex h-full w-full items-center justify-center text-muted-foreground">
                <Loader2 className="h-5 w-5 animate-spin" />
              </div>
            ) : previewSrc ? (
              <img
                src={previewSrc}
                alt={asset.displayName ?? asset.fileName}
                loading="eager"
                decoding="async"
                className={cn("h-full w-full object-cover", hover && isGifPreview && "opacity-0")}
                draggable={false}
                onError={() => void loadPreviewFallback()}
              />
            ) : (
              <div
                className="flex h-full w-full items-center justify-center text-muted-foreground"
                aria-hidden="true"
              >
                <ImageIcon className="h-8 w-8 opacity-50" />
              </div>
            )}
            {canPreviewVideo && (
              <HoverVideoPreview
                active={hover}
                assetId={asset.id}
                videoPath={asset.absPath}
                poster={previewSrc ?? undefined}
                className="absolute inset-0 h-full w-full object-cover"
              />
            )}
            {hover && isGifPreview && (
              <img
                src={convertFileSrc(asset.absPath)}
                alt=""
                className="absolute inset-0 h-full w-full object-cover"
                draggable={false}
              />
            )}
            {/* Video duration. */}
            {asset.kind === "video" && (
              <div
                className={cn(
                  "absolute bottom-1.5 rounded bg-black/60 px-1.5 py-0.5 text-[10px] text-white",
                  hover ? "left-10" : "left-1.5",
                )}
              >
                {asset.durationMs ? formatDuration(asset.durationMs) : "video"}
              </div>
            )}
            {/* Kind and format. */}
            <div className="pointer-events-none absolute bottom-1.5 right-1.5 flex flex-col items-end gap-1">
              {asset.kind === "video" && videoCodec === "prores_raw" && (
                <span className="rounded bg-black/70 px-1.5 py-0.5 text-[9px] font-semibold text-white">
                  ProRes RAW
                </span>
              )}
              <div className="flex items-center gap-1 rounded bg-black/70 px-1.5 py-0.5 text-[9px] font-semibold text-white">
                {asset.kind === "video" ? (
                  <Film className="h-3 w-3" />
                ) : (
                  <ImageIcon className="h-3 w-3" />
                )}
                {extension}
              </div>
            </div>
            {/* View source. */}
            <button
              className={cn(
                "invisible absolute top-1.5 flex h-8 w-8 cursor-pointer items-center justify-center rounded-full bg-black/45 text-white hover:bg-black/65 group-hover:visible",
                selectMode || selected ? "right-10" : "right-1.5",
              )}
              title={t("assets.previewSource")}
              onClick={(event) => {
                event.stopPropagation();
                onPreview(asset);
              }}
            >
              <ZoomIn className="h-4 w-4" />
            </button>
            {asset.kind === "image" && (
              <ImageIcon className="absolute left-1.5 top-1.5 h-3.5 w-3.5 text-white/80 opacity-0 transition-opacity group-hover:opacity-100" />
            )}
            {/* Per-asset processing or failure status. */}
            {indexingTasks.length > 0 && (
              <div
                role="status"
                className={cn(
                  "absolute left-1.5 flex max-w-[calc(100%-12px)] flex-col gap-1",
                  processing || error ? "top-8" : "top-1.5",
                )}
              >
                {indexingTasks.map((task) => (
                  <span
                    key={task.id}
                    title={task.error ?? undefined}
                    className={cn(
                      "flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] text-white",
                      needsProcessingAttention(task) ? "bg-destructive/90" : "bg-black/70",
                    )}
                  >
                    {activeProcessing(task) ? (
                      <Loader2 className="h-3 w-3 shrink-0 animate-spin" />
                    ) : (
                      <AlertTriangle className="h-3 w-3 shrink-0" />
                    )}
                    <span className="truncate">
                      {t(`processingQueue.kind.${task.kind}`)} ·{" "}
                      {t(`processingQueue.state.${task.status}`)}
                    </span>
                  </span>
                ))}
              </div>
            )}
            {processing && (
              <div className="absolute left-1.5 top-1.5 flex items-center gap-1 rounded bg-black/60 px-1.5 py-0.5 text-[10px] font-medium text-white">
                <Loader2 className="h-3 w-3 animate-spin" />
                {t(`stage.${asset.importStage}`, {
                  defaultValue: t("stage.processing"),
                })}
              </div>
            )}
            {error && (
              <button
                className="absolute left-1.5 top-1.5 flex items-center gap-1 rounded bg-destructive px-1.5 py-0.5 text-[10px] font-medium text-destructive-foreground"
                title={asset.importStage.slice(6)}
                onClick={(e) => {
                  e.stopPropagation();
                  void retryImport();
                }}
              >
                <AlertTriangle className="h-3 w-3" />
                {t("stage.failed")}
                <RotateCw className="h-3 w-3" />
              </button>
            )}
          </div>

          {/* Keep footer height consistent with the virtual grid. */}
          <div className="px-2 py-1.5">
            <div
              className="truncate text-xs text-muted-foreground"
              title={asset.displayName ?? asset.fileName}
            >
              {asset.displayName ?? asset.fileName}
            </div>
            <AssetCardMetadata asset={asset} />
          </div>

          {/* Selection checkbox. */}
          {(selectMode || selected) && (
            <div className="absolute right-1.5 top-1.5 z-10" onClick={(e) => e.stopPropagation()}>
              <Checkbox
                checked={selected}
                onCheckedChange={() => onSelect(asset.id, true)}
                className="border-white bg-white/30"
              />
            </div>
          )}
        </div>
      </ContextMenuTrigger>

      <AssetContextMenuContent
        asset={asset}
        actions={{
          onSimilar,
          onAddTags,
          onMove,
          onBatch,
          onDetails: onOpen,
          onTrash,
        }}
      />
    </ContextMenu>
  );
}
// Filtering recreates parent callbacks; compare only visual card inputs to avoid rerendering
// unchanged cards with the toolbar.
const AssetCard = memo(
  AssetCardInner,
  (previous, next) =>
    previous.asset === next.asset &&
    previous.selected === next.selected &&
    previous.selectMode === next.selectMode,
);
export default AssetCard;
