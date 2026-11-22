import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import { toast } from "@/components/Toaster";
import type { Asset } from "@/types";
import { explainError } from "@/lib/errors";
import { usePreviewNavigation, type PreviewPagination } from "@/hooks/usePreviewNavigation";
export function useAssetSourcePreview({
  asset: sourceAsset,
  assets = [],
  initialTimeMs = 0,
  selectionEndMs,
  open,
  onOpenChange,
  pagination,
}: {
  asset: Asset | null;
  assets?: Asset[];
  initialTimeMs?: number;
  selectionEndMs?: number;
  open: boolean;
  onOpenChange: (v: boolean) => void;
  pagination?: PreviewPagination;
}) {
  const { t } = useTranslation();
  const libraryId = useAppStore((state) => state.current?.id);
  const contentRef = useRef<HTMLDivElement>(null);
  const {
    asset,
    index,
    canNavigate,
    canNext,
    loadingMore,
    total,
    navigate: goTo,
  } = usePreviewNavigation({ sourceAsset, assets, open, pagination });
  const isOriginalAsset = asset?.id === sourceAsset?.id;
  const navigate = (direction: -1 | 1) => {
    dragRef.current = null;
    contentRef.current?.focus({ preventScroll: true });
    void goTo(direction);
  };
  const [imageUrl, setImageUrl] = useState("");
  const [scale, setScale] = useState(1);
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const [windowFullscreen, setWindowFullscreen] = useState(false);
  const [fullscreenHint, setFullscreenHint] = useState(false);
  useEffect(() => {
    setFullscreenHint(open && windowFullscreen);
    if (!open || !windowFullscreen) return;
    const timer = window.setTimeout(() => setFullscreenHint(false), 3000);
    return () => window.clearTimeout(timer);
  }, [open, windowFullscreen]);
  const videoRef = useRef<HTMLVideoElement>(null);
  const dragRef = useRef<{
    x: number;
    y: number;
    ox: number;
    oy: number;
  } | null>(null);
  const fullPreviewRequestedRef = useRef(false);
  const previewRequestRef = useRef(0);
  const previewObjectUrlRef = useRef<string | null>(null);
  const previewPath = asset?.absPath;
  const previewKind = asset?.kind;
  useEffect(() => {
    previewRequestRef.current += 1;
    fullPreviewRequestedRef.current = false;
    if (previewObjectUrlRef.current) URL.revokeObjectURL(previewObjectUrlRef.current);
    previewObjectUrlRef.current = null;
    setImageUrl(previewPath && previewKind !== "video" ? convertFileSrc(previewPath) : "");
    dragRef.current = null;
    setScale(1);
    setOffset({ x: 0, y: 0 });
    return () => {
      previewRequestRef.current += 1;
      if (previewObjectUrlRef.current) URL.revokeObjectURL(previewObjectUrlRef.current);
      previewObjectUrlRef.current = null;
    };
  }, [asset?.id, libraryId, previewPath, previewKind, open, initialTimeMs, isOriginalAsset]);
  const handleImageError = async () => {
    if (!asset) return;
    const thumbnailUrl = asset.thumbPath ? convertFileSrc(asset.thumbPath) : "";
    if (/\.(heic|heif)$/i.test(asset.fileName) && !fullPreviewRequestedRef.current && open) {
      fullPreviewRequestedRef.current = true;
      const request = previewRequestRef.current;
      if (thumbnailUrl) setImageUrl(thumbnailUrl);
      try {
        const bytes = await api.assetFullPreview(asset.id);
        if (request !== previewRequestRef.current) return;
        const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: "image/png" }));
        previewObjectUrlRef.current = url;
        setImageUrl(url);
      } catch {
        // Retain the generated thumbnail when decoding fails.
      }
    } else if (thumbnailUrl && imageUrl !== thumbnailUrl) {
      if (previewObjectUrlRef.current) URL.revokeObjectURL(previewObjectUrlRef.current);
      previewObjectUrlRef.current = null;
      setImageUrl(thumbnailUrl);
    }
  };
  const zoomTo = (next: number) => {
    const value = Math.min(8, Math.max(0.25, next));
    setScale(value);
    if (value === 1) setOffset({ x: 0, y: 0 });
  };
  const toggleVideoFullscreen = async () => {
    try {
      const next = !windowFullscreen;
      await getCurrentWindow().setFullscreen(next);
      setWindowFullscreen(next);
    } catch (error) {
      toast.error(explainError(error));
    }
  };
  const handleOpenChange = async (nextOpen: boolean) => {
    if (!nextOpen && windowFullscreen) {
      try {
        await getCurrentWindow().setFullscreen(false);
      } catch (error) {
        toast.error(explainError(error));
      } finally {
        setWindowFullscreen(false);
      }
    }
    onOpenChange(nextOpen);
  };
  if (!asset) return null;
  const isVideo = asset.kind === "video";
  return {
    assets,
    initialTimeMs,
    selectionEndMs,
    open,
    onOpenChange,
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
  };
}
