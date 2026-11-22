import { memo, useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { convertFileSrc } from "@tauri-apps/api/core";
import { assetPosterPath } from "@/lib/assetPreview";
import { Film, Image as ImageIcon, ZoomIn } from "lucide-react";
import { api } from "@/lib/api";
import { cn, formatDuration } from "@/lib/utils";
import type { Asset, SearchHit } from "@/types";
import {
  AssetContextMenuContent,
  type AssetMenuActions,
} from "@/components/assets/AssetContextMenu";
import { ContextMenu, ContextMenuTrigger } from "@/components/ui/context-menu";
import { Checkbox } from "@/components/ui/checkbox";
import HoverVideoPreview from "@/components/assets/HoverVideoPreview";
import AssetCardMetadata from "@/components/assets/AssetCardMetadata";
/** Search result card shows the matched preview and asset information. */
export const SearchResultCard = memo(
  function SearchResultCard({
    hit,
    onOpen,
    onPreview,
    actions,
    selected,
    selectMode,
    onSelect,
  }: {
    hit: SearchHit;
    onOpen: (a: Asset) => void;
    onPreview: (a: Asset) => void;
    actions: AssetMenuActions;
    selected: boolean;
    selectMode: boolean;
    onSelect: (a: Asset) => void;
  }) {
    const { t } = useTranslation();
    const matchedPath = assetPosterPath(hit.asset, hit.framePath);
    const [posterSrc, setPosterSrc] = useState<string | undefined>(() =>
      matchedPath ? convertFileSrc(matchedPath) : undefined,
    );
    const [previewing, setPreviewing] = useState(false);
    const fallbackAttempted = useRef(false);
    const objectUrlRef = useRef<string | null>(null);
    const hoverVideoPath = hit.asset.kind === "video" ? hit.asset.absPath : null;
    const isGif = hit.asset.kind === "gif" && hit.asset.fileName.toLowerCase().endsWith(".gif");
    const extension =
      hit.asset.fileName.split(".").pop()?.toUpperCase() || hit.asset.kind.toUpperCase();
    useEffect(() => {
      fallbackAttempted.current = false;
      setPosterSrc(matchedPath ? convertFileSrc(matchedPath) : undefined);
      return () => {
        if (objectUrlRef.current) URL.revokeObjectURL(objectUrlRef.current);
        objectUrlRef.current = null;
      };
    }, [matchedPath]);
    const handlePreviewError = useCallback(async () => {
      setPosterSrc(undefined);
      if (fallbackAttempted.current) return;
      fallbackAttempted.current = true;
      try {
        const bytes = hit.framePath
          ? await api.framePreview(hit.frameId)
          : await api.assetPreview(hit.asset.id);
        if (objectUrlRef.current) URL.revokeObjectURL(objectUrlRef.current);
        objectUrlRef.current = URL.createObjectURL(new Blob([new Uint8Array(bytes)]));
        setPosterSrc(objectUrlRef.current);
      } catch {
        // Keep the filename and details available when one preview fails.
      }
    }, [hit.asset.id, hit.frameId, hit.framePath]);
    return (
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            className="group relative cursor-pointer overflow-hidden rounded-lg border bg-muted transition-colors hover:border-foreground/30"
            onClick={() => (selectMode ? onSelect(hit.asset) : onOpen(hit.asset))}
            onMouseEnter={() => setPreviewing(true)}
            onMouseLeave={() => setPreviewing(false)}
          >
            <div
              className="relative overflow-hidden"
              style={{
                aspectRatio:
                  hit.asset.width && hit.asset.height
                    ? `${hit.asset.width} / ${hit.asset.height}`
                    : "4 / 3",
              }}
            >
              {posterSrc ? (
                <img
                  src={posterSrc}
                  alt={hit.asset.displayName ?? hit.asset.fileName}
                  loading="eager"
                  className={cn(
                    "absolute inset-0 h-full w-full object-cover",
                    previewing && isGif && "opacity-0",
                  )}
                  draggable={false}
                  onError={() => void handlePreviewError()}
                />
              ) : (
                <div
                  className="flex h-full items-center justify-center text-muted-foreground"
                  aria-label={hit.asset.displayName ?? hit.asset.fileName}
                >
                  <ImageIcon className="h-8 w-8" />
                </div>
              )}
              {hoverVideoPath && (
                <HoverVideoPreview
                  active={previewing}
                  assetId={hit.asset.id}
                  videoPath={hoverVideoPath}
                  startMs={hit.timestampMs}
                  poster={posterSrc}
                  className="absolute inset-0 h-full w-full object-cover"
                />
              )}
              {previewing && isGif && (
                <img
                  src={convertFileSrc(hit.asset.absPath)}
                  alt=""
                  className="absolute inset-0 h-full w-full object-cover"
                  draggable={false}
                />
              )}
              <div className="absolute bottom-1.5 right-1.5 flex items-center gap-1 rounded bg-black/70 px-1.5 py-0.5 text-[9px] font-semibold text-white">
                {hit.asset.kind === "video" ? (
                  <Film className="h-3 w-3" />
                ) : (
                  <ImageIcon className="h-3 w-3" />
                )}
                {extension}
              </div>
              <button
                className="invisible absolute right-1.5 top-1.5 flex h-8 w-8 cursor-pointer items-center justify-center rounded-full bg-black/45 text-white hover:bg-black/65 group-hover:visible"
                title={t("assets.previewSource")}
                onClick={(event) => {
                  event.stopPropagation();
                  onPreview(hit.asset);
                }}
              >
                <ZoomIn className="h-4 w-4" />
              </button>
            </div>
            <div className="px-2 py-1.5">
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <div className="min-w-0 flex-1 truncate">
                  {hit.asset.displayName ?? hit.asset.fileName}
                </div>
                {hit.asset.kind !== "image" && (
                  <div className="shrink-0 tabular-nums">
                    {formatDuration(hit.asset.durationMs ?? 0)}
                  </div>
                )}
              </div>
              <AssetCardMetadata asset={hit.asset} />
            </div>
            {(selectMode || selected) && (
              <div
                className="absolute right-1.5 top-1.5 z-20"
                onClick={(event) => event.stopPropagation()}
              >
                <Checkbox
                  checked={selected}
                  onCheckedChange={() => onSelect(hit.asset)}
                  className="border-white bg-white/30"
                />
              </div>
            )}
          </div>
        </ContextMenuTrigger>
        <AssetContextMenuContent asset={hit.asset} actions={actions} context="search-result" />
      </ContextMenu>
    );
  },
  (previous, next) =>
    previous.hit === next.hit &&
    previous.selected === next.selected &&
    previous.selectMode === next.selectMode,
);
