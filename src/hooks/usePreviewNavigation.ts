import { useEffect, useRef, useState } from "react";
import type { Asset } from "@/types";
import { useAppStore } from "@/stores/app";
import { toast } from "@/components/Toaster";
import { explainError } from "@/lib/errors";
export interface PreviewPagination {
  scopeKey: string;
  total: number;
  hasMore: boolean;
  /** Return the complete loaded list, including the newly fetched page. */
  loadMore: () => Promise<Asset[]>;
}
export function usePreviewNavigation({
  sourceAsset,
  assets,
  open,
  pagination,
}: {
  sourceAsset: Asset | null;
  assets: Asset[];
  open: boolean;
  pagination?: PreviewPagination;
}) {
  const libraryId = useAppStore((state) => state.current?.id);
  const context = JSON.stringify([libraryId, pagination?.scopeKey, sourceAsset?.id, open]);
  const [navigation, setNavigation] = useState<{
    context: string;
    assetId: number;
  } | null>(null);
  const [loadingMore, setLoadingMore] = useState(false);
  const pending = useRef<object | null>(null);
  const asset =
    navigation?.context === context
      ? (assets.find((item) => item.id === navigation.assetId) ?? sourceAsset)
      : sourceAsset;
  const index = assets.findIndex((item) => item.id === asset?.id);
  const canLoadMore = !!pagination?.hasMore;
  const canNavigate = index >= 0 && (assets.length > 1 || canLoadMore);
  const canNext = index >= 0 && (index < assets.length - 1 || (canLoadMore && !loadingMore));
  useEffect(() => {
    setNavigation(null);
    setLoadingMore(false);
    return () => {
      pending.current = null;
    };
  }, [context]);
  const select = (next: Asset) => {
    // Returning to an earlier item cancels the pending navigation intent, not the shared query.
    pending.current = null;
    setLoadingMore(false);
    setNavigation({ context, assetId: next.id });
  };
  const navigate = async (direction: -1 | 1) => {
    if (!open || index < 0 || !asset) return;
    const next = assets[index + direction];
    if (next) {
      select(next);
      return;
    }
    if (direction !== 1 || !pagination?.hasMore || pending.current) return;
    const request = {};
    pending.current = request;
    setLoadingMore(true);
    try {
      const loaded = await pagination.loadMore();
      if (pending.current !== request) return;
      const position = loaded.findIndex((item) => item.id === asset.id);
      const following = position >= 0 ? loaded[position + 1] : undefined;
      if (following) setNavigation({ context, assetId: following.id });
    } catch (error) {
      if (pending.current === request) toast.error(explainError(error));
    } finally {
      if (pending.current === request) {
        pending.current = null;
        setLoadingMore(false);
      }
    }
  };
  return {
    asset,
    index,
    canNavigate,
    canNext,
    loadingMore,
    navigate,
    total: Math.max(pagination?.total ?? assets.length, assets.length),
  };
}
