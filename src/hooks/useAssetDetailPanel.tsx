import { queryKeys } from "@/lib/queryKeys";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { convertFileSrc } from "@tauri-apps/api/core";
import { assetPosterPath } from "@/lib/assetPreview";
import { api } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import { toast } from "@/components/Toaster";
import type { Asset } from "@/types";
import { explainError } from "@/lib/errors";
import type { PreviewPagination } from "@/hooks/usePreviewNavigation";
export function useAssetDetailPanel({
  assetId,
  previewAssets,
  open,
  onOpenChange,
  onTrash,
}: {
  assetId: number | null;
  previewAssets?: Asset[];
  previewPagination?: PreviewPagination;
  open: boolean;
  onOpenChange: (v: boolean) => void;
  onTrash?: (asset: Asset) => void;
}) {
  const { t, i18n } = useTranslation();
  const libraryId = useAppStore((state) => state.current?.id);
  const queryClient = useQueryClient();
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  const [frameItems, setFrameItems] = useState<
    Array<{
      id: number;
      timestampMs: number;
      url: string;
    }>
  >([]);
  const [moveFolderOpen, setMoveFolderOpen] = useState(false);
  const [allFramesOpen, setAllFramesOpen] = useState(false);
  const [detailTrim, setDetailTrim] = useState<{
    startMs: number;
    endMs?: number;
    subtitleVersion?: string;
  } | null>(null);
  const [coverOpen, setCoverOpen] = useState(false);
  const [sourcePreviewOpen, setSourcePreviewOpen] = useState(false);
  const [previewStartMs, setPreviewStartMs] = useState(0);
  const inlineVideoRef = useRef<HTMLVideoElement>(null);
  useEffect(() => {
    setCoverOpen(false);
    setSourcePreviewOpen(false);
    setAllFramesOpen(false);
    setDetailTrim(null);
  }, [assetId, open]);
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleDraft, setTitleDraft] = useState("");
  const [panelWidth, setPanelWidth] = useState(() => {
    const saved = Number(localStorage.getItem("asset-detail-panel-width"));
    return Number.isFinite(saved) && saved >= 280 && saved <= 520 ? saved : 320;
  });
  const panelResizeRef = useRef<{
    pointerId: number;
    startX: number;
    startWidth: number;
    currentWidth: number;
  } | null>(null);
  const resizeHandle = (
    <button
      type="button"
      aria-label={t("assets.resizeDetails")}
      className="group absolute inset-y-0 left-0 z-20 w-2 -translate-x-1/2 cursor-col-resize touch-none focus:outline-none"
      onPointerDown={(event) => {
        panelResizeRef.current = {
          pointerId: event.pointerId,
          startX: event.clientX,
          startWidth: panelWidth,
          currentWidth: panelWidth,
        };
        event.currentTarget.setPointerCapture(event.pointerId);
      }}
      onPointerMove={(event) => {
        const drag = panelResizeRef.current;
        if (!drag || drag.pointerId !== event.pointerId) return;
        drag.currentWidth = Math.min(
          520,
          Math.max(280, drag.startWidth + drag.startX - event.clientX),
        );
        setPanelWidth(drag.currentWidth);
      }}
      onPointerUp={(event) => {
        if (panelResizeRef.current?.pointerId !== event.pointerId) return;
        localStorage.setItem(
          "asset-detail-panel-width",
          String(panelResizeRef.current.currentWidth),
        );
        panelResizeRef.current = null;
        event.currentTarget.releasePointerCapture(event.pointerId);
      }}
      onPointerCancel={() => {
        panelResizeRef.current = null;
      }}
    >
      <span className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-border transition-colors group-hover:w-0.5 group-hover:bg-primary group-focus-visible:w-0.5 group-focus-visible:bg-primary" />
    </button>
  );
  const {
    data: detail,
    isLoading,
    isError,
    error,
    refetch,
  } = useQuery({
    queryKey: queryKeys.assetDetail.item(libraryId, assetId),
    queryFn: () => api.assetDetail(assetId!),
    enabled: open && assetId != null,
  });
  // Video previews use playable sources; images use thumbnails.
  useEffect(() => {
    setPreviewUrl(null);
    setFrameItems([]);
    if (!detail) return;
    setTitleDraft(detail.displayName ?? detail.fileName);
    setEditingTitle(false);
    const posterPath = assetPosterPath(detail);
    setPreviewUrl(posterPath ? convertFileSrc(posterPath) : null);
    setFrameItems(
      detail.frames
        .filter((f) => f.absPath)
        .map((f) => ({
          id: f.id,
          timestampMs: f.timestampMs,
          url: convertFileSrc(f.absPath!),
        })),
    );
  }, [detail]);
  const removeTag = async (name: string) => {
    if (!assetId) return;
    try {
      await api.assetRemoveTag(assetId, name);
      void queryClient.invalidateQueries({
        queryKey: queryKeys.assetDetail.item(libraryId, assetId),
      });
      void queryClient.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
    } catch (e) {
      toast.error(explainError(e));
    }
  };
  const saveTitle = async () => {
    if (!detail) return;
    const value = titleDraft.trim();
    try {
      await api.assetSetDisplayName(detail.id, value && value !== detail.fileName ? value : null);
      await queryClient.invalidateQueries({
        queryKey: queryKeys.assetDetail.item(libraryId, detail.id),
      });
      await queryClient.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
      setEditingTitle(false);
    } catch (error) {
      toast.error(explainError(error));
    }
  };
  return {
    assetId,
    previewAssets,
    open,
    onOpenChange,
    onTrash,
    t,
    i18n,
    queryClient,
    previewUrl,
    frameItems,
    moveFolderOpen,
    setMoveFolderOpen,
    allFramesOpen,
    setAllFramesOpen,
    detailTrim,
    setDetailTrim,
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
  };
}
