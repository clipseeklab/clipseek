import { useAssetSourcePreview } from "@/hooks/useAssetSourcePreview";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  ChevronLeft,
  ChevronRight,
  Maximize2,
  Minimize2,
  Minus,
  Plus,
  RotateCcw,
  Loader2,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import PlaybackVideo from "@/components/assets/PlaybackVideo";
export function AssetSourcePreview(props: Parameters<typeof useAssetSourcePreview>[0]) {
  const state = useAssetSourcePreview(props);
  if (!state) return null;
  const {
    initialTimeMs,
    selectionEndMs,
    open,
    t,
    contentRef,
    asset,
    index,
    canNavigate,
    canNext,
    loadingMore,
    total,
    isOriginalAsset,
    navigate,
    imageUrl,
    scale,
    offset,
    setOffset,
    windowFullscreen,
    fullscreenHint,
    videoRef,
    dragRef,
    handleImageError,
    zoomTo,
    toggleVideoFullscreen,
    handleOpenChange,
    isVideo,
  } = state;
  return (
    <Dialog open={open} onOpenChange={(nextOpen) => void handleOpenChange(nextOpen)}>
      <DialogContent
        ref={contentRef}
        showCloseButton={!windowFullscreen}
        onEscapeKeyDown={(event) => {
          if (windowFullscreen) {
            event.preventDefault();
            void toggleVideoFullscreen();
          }
        }}
        aria-describedby={undefined}
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          contentRef.current?.focus({ preventScroll: true });
        }}
        overlayClassName="bg-zinc-900/10 dark:bg-black/25 backdrop-blur-sm"
        onKeyDown={(event) => {
          if (
            event.defaultPrevented ||
            event.altKey ||
            event.ctrlKey ||
            event.metaKey ||
            event.shiftKey
          )
            return;
          if (
            (event.target as HTMLElement).closest(
              "input, textarea, select, video, [contenteditable=true], [role=slider]",
            )
          )
            return;
          if (canNavigate && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
            event.preventDefault();
            navigate(event.key === "ArrowLeft" ? -1 : 1);
          }
        }}
        className={cn(
          "flex flex-col gap-0 overflow-hidden rounded-2xl border-border/60 bg-background/60 p-0 text-foreground shadow-2xl backdrop-blur-2xl",
          windowFullscreen
            ? "z-[80] h-screen w-screen max-w-none rounded-none border-0 bg-black sm:max-w-none"
            : "h-[94vh] max-w-[96vw] sm:max-w-[96vw]",
        )}
      >
        <DialogHeader
          className={
            windowFullscreen
              ? "sr-only"
              : "shrink-0 flex-row items-center justify-between gap-4 space-y-0 border-b border-border/50 px-5 py-4 pr-14 text-left"
          }
        >
          <DialogTitle className="min-w-0 truncate text-sm font-medium">
            {asset.displayName ?? asset.fileName}
          </DialogTitle>
          {canNavigate && (
            <span
              role="status"
              aria-live="polite"
              aria-atomic="true"
              className="shrink-0 text-xs tabular-nums text-muted-foreground"
            >
              {index + 1} / {total}
            </span>
          )}
        </DialogHeader>
        <div
          className={cn(
            "relative flex min-h-0 flex-1 items-center justify-center overflow-hidden",
            !windowFullscreen && "px-4 pb-20 pt-4 sm:px-20",
          )}
          onWheel={
            isVideo
              ? undefined
              : (event) => {
                  event.preventDefault();
                  zoomTo(scale * (event.deltaY < 0 ? 1.15 : 0.87));
                }
          }
        >
          {canNavigate && !windowFullscreen && (
            <>
              <Button
                variant="ghost"
                size="icon"
                className="absolute left-3 top-1/2 z-10 h-10 w-10 -translate-y-1/2 rounded-full border border-border/60 bg-background/80 shadow-sm backdrop-blur-xl disabled:opacity-25 sm:left-5"
                aria-label={t("assets.previousAsset")}
                title={t("assets.previousAsset")}
                disabled={index === 0}
                onClick={() => navigate(-1)}
              >
                <ChevronLeft className="h-5 w-5" />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                className="absolute right-3 top-1/2 z-10 h-10 w-10 -translate-y-1/2 rounded-full border border-border/60 bg-background/80 shadow-sm backdrop-blur-xl disabled:opacity-25 sm:right-5"
                aria-label={t("assets.nextAsset")}
                title={t("assets.nextAsset")}
                disabled={!canNext}
                aria-busy={loadingMore}
                onClick={() => navigate(1)}
              >
                {loadingMore ? (
                  <Loader2 className="h-5 w-5 animate-spin" />
                ) : (
                  <ChevronRight className="h-5 w-5" />
                )}
              </Button>
            </>
          )}
          {isVideo ? (
            <>
              <PlaybackVideo
                key={asset.id}
                ref={videoRef}
                assetId={asset.id}
                sourcePath={asset.absPath}
                poster={asset.thumbPath ? convertFileSrc(asset.thumbPath) : undefined}
                controls
                autoPlay
                playsInline
                className={
                  windowFullscreen
                    ? "absolute inset-0 h-full w-full object-contain"
                    : "max-h-full max-w-full"
                }
                onTimeUpdate={(event) => {
                  const time = event.currentTarget.currentTime * 1000;
                  if (isOriginalAsset && selectionEndMs != null && time >= selectionEndMs) {
                    event.currentTarget.pause();
                  }
                }}
                onLoadedMetadata={(event) => {
                  const seconds = Math.max(0, (isOriginalAsset ? initialTimeMs : 0) / 1000);
                  event.currentTarget.currentTime = Math.min(
                    seconds,
                    event.currentTarget.duration || seconds,
                  );
                  void event.currentTarget.play().catch(() => undefined);
                }}
              ></PlaybackVideo>
              {fullscreenHint && (
                <div
                  role="status"
                  className="pointer-events-none absolute inset-x-4 top-6 z-10 text-center"
                >
                  <span className="inline-block rounded-lg bg-black/70 px-4 py-2 text-sm text-white">
                    {t("assets.fullscreenHint")}
                  </span>
                </div>
              )}
              {!windowFullscreen && (
                <div className="absolute right-4 top-4 z-10 flex items-center gap-2">
                  <Button
                    variant="secondary"
                    size="sm"
                    className="bg-black/55 text-white hover:bg-black/75"
                    title={windowFullscreen ? t("assets.exitFullscreen") : t("assets.fullscreen")}
                    onClick={() => void toggleVideoFullscreen()}
                  >
                    {windowFullscreen ? (
                      <Minimize2 className="h-4 w-4" />
                    ) : (
                      <Maximize2 className="h-4 w-4" />
                    )}
                    {windowFullscreen ? t("assets.exitFullscreen") : t("assets.fullscreen")}
                  </Button>
                </div>
              )}
            </>
          ) : (
            <>
              <img
                key={asset.id}
                src={imageUrl}
                alt={asset.fileName}
                draggable={false}
                className={cn(
                  "max-h-full max-w-full select-none object-contain will-change-transform",
                  scale > 1 ? "cursor-grab active:cursor-grabbing" : "cursor-zoom-in",
                )}
                style={{ transform: `translate(${offset.x}px, ${offset.y}px) scale(${scale})` }}
                onDoubleClick={() => zoomTo(scale === 1 ? 2 : 1)}
                onPointerDown={(event) => {
                  if (scale <= 1) return;
                  event.currentTarget.setPointerCapture(event.pointerId);
                  dragRef.current = {
                    x: event.clientX,
                    y: event.clientY,
                    ox: offset.x,
                    oy: offset.y,
                  };
                }}
                onPointerMove={(event) => {
                  const drag = dragRef.current;
                  if (!drag) return;
                  setOffset({
                    x: drag.ox + event.clientX - drag.x,
                    y: drag.oy + event.clientY - drag.y,
                  });
                }}
                onPointerUp={(event) => {
                  if (!dragRef.current) return;
                  dragRef.current = null;
                  event.currentTarget.releasePointerCapture(event.pointerId);
                }}
                onError={() => void handleImageError()}
              />
              <div className="absolute bottom-5 left-1/2 z-20 flex -translate-x-1/2 items-center gap-1 rounded-full border border-border/60 bg-background/85 p-1 text-foreground shadow-lg backdrop-blur-xl">
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8 rounded-full"
                  title={t("assets.zoomOut")}
                  onClick={() => zoomTo(scale / 1.25)}
                >
                  <Minus className="h-4 w-4" />
                </Button>
                <span className="w-14 text-center text-xs tabular-nums">
                  {Math.round(scale * 100)}%
                </span>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8 rounded-full"
                  title={t("assets.zoomIn")}
                  onClick={() => zoomTo(scale * 1.25)}
                >
                  <Plus className="h-4 w-4" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8 rounded-full"
                  title={t("assets.resetView")}
                  onClick={() => zoomTo(1)}
                >
                  <RotateCcw className="h-4 w-4" />
                </Button>
              </div>
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
