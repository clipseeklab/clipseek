import { useAppStore } from "@/stores/app";
import { queryKeys } from "@/lib/queryKeys";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLocation, useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { api } from "@/lib/api";
import { toast } from "@/components/Toaster";
import { useAssetPages } from "@/hooks/useAssetPages";
import { useImportStore } from "@/stores/import";
import type { Asset as AssetType } from "@/types";
import type { Asset, AssetFilter } from "@/types";
import { collectPendingPreviews } from "@/lib/importPerformance";
import { useImportEta } from "@/hooks/useImportEta";
import {
  EMPTY_FILTER,
  activeFilterCount,
  toAssetFilter,
  type FilterState,
} from "@/components/assets/AssetFilters";
import { explainError } from "@/lib/errors";
export function useAssetBrowser({
  filter,
  emptyHint,
  importFolder,
  showStorageStats = false,
  backTo,
  scopeName,
}: {
  filter: AssetFilter;
  emptyHint: string;
  importFolder: number | null | undefined;
  showStorageStats?: boolean;
  backTo?: string;
  scopeName?: string;
}) {
  const { t } = useTranslation();
  const libraryId = useAppStore((state) => state.current?.id);
  const queryClient = useQueryClient();
  const location = useLocation();
  const navigate = useNavigate();
  const searchParams = new URLSearchParams(location.search);
  const missingOnly = searchParams.get("missing") === "1";
  const [listFilter, setListFilter] = useState<FilterState>(EMPTY_FILTER);
  const [showFilters, setShowFilters] = useState(false);
  const canFilterFolder = filter.folderId == null && !filter.uncategorized;
  const queryFilter = useMemo(() => {
    const selected = toAssetFilter(listFilter);
    if (!canFilterFolder) {
      delete selected.folderId;
      delete selected.folderIds;
      delete selected.uncategorized;
    }
    const tags = [...new Set([...(filter.tags ?? []), ...(selected.tags ?? [])])];
    return { ...selected, ...filter, sourceMissing: missingOnly, tags };
  }, [canFilterFolder, filter, listFilter, missingOnly]);
  const activeFilters = activeFilterCount({
    ...listFilter,
    tags: listFilter.tags.filter((tag) => !filter.tags?.includes(tag)),
  });
  const {
    items,
    total,
    isLoading,
    isError,
    isFetchingNextPage,
    fetchNextPage,
    hasNextPage,
    refetch,
    isPlaceholderData,
    isFilterPending,
  } = useAssetPages(queryFilter);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [selectMode, setSelectMode] = useState(false);
  const [selectedListOpen, setSelectedListOpen] = useState(false);
  const [detailId, setDetailId] = useState<number | null>(null);
  useEffect(() => {
    const id = Number(new URLSearchParams(location.search).get("assetId"));
    if (Number.isSafeInteger(id) && id > 0) setDetailId(id);
  }, [location.search]);
  const [previewAsset, setPreviewAsset] = useState<Asset | null>(null);
  const previewScopeKey = JSON.stringify([libraryId, queryFilter]);
  const previousPreviewScope = useRef(previewScopeKey);
  useEffect(() => {
    if (previousPreviewScope.current !== previewScopeKey) {
      previousPreviewScope.current = previewScopeKey;
      setPreviewAsset(null);
      setDetailId(null);
    }
  }, [previewScopeKey]);
  const [tagsOpen, setTagsOpen] = useState(false);
  const [moveOpen, setMoveOpen] = useState(false);
  const [tagTargets, setTagTargets] = useState<Asset[]>([]);
  const [moveTargets, setMoveTargets] = useState<Asset[]>([]);
  const [moveLibOpen, setMoveLibOpen] = useState(false);
  const [indexBuildOpen, setIndexBuildOpen] = useState(false);
  const progress = useImportStore((s) => s.progress);
  const liveAssets = useImportStore((s) => s.liveAssets);
  const clearLiveAssets = useImportStore((s) => s.clearLiveAssets);
  const tasks = useImportStore((s) => s.tasks);
  const activeImportStartedAt = useMemo(() => {
    const active = tasks.filter((task) => task.status === "processing");
    return active.length > 0 ? Math.min(...active.map((task) => task.createdAt)) : undefined;
  }, [tasks]);
  const etaSeconds = useImportEta(progress, activeImportStartedAt);
  const pendingPreview = useMemo(() => {
    if (missingOnly || filter.inTrash || activeFilters > 0 || filter.tags?.length)
      return { items: [], total: 0 };
    return collectPendingPreviews(
      tasks.filter(
        (task) =>
          (task.status === "pending" || task.status === "processing") &&
          (!filter.uncategorized || task.folderId == null) &&
          (filter.folderId == null || task.folderId === filter.folderId),
      ),
    );
  }, [tasks, filter, activeFilters, missingOnly]);
  const pendingFiles = pendingPreview.items;
  useEffect(() => {
    setSelected(new Set());
    setSelectMode(false);
  }, [location.pathname]);
  /** Merge live asset events into query results, prepending new assets and replacing existing
   * state.
   */
  const mergedItems = useMemo(() => {
    const matchesFilter = (a: AssetType) => {
      if (filter.tags?.some((tag) => !a.tags.includes(tag))) return false;
      if (missingOnly && !a.sourceMissing) return false;
      if (filter.inTrash) return false; // Do not inject live import events into trash.
      if (filter.uncategorized) return a.folderId == null;
      if (filter.folderId != null) return a.folderId === filter.folderId;
      return true;
    };
    const liveArr = [...liveAssets.values()];
    const byId = new Map(items.map((a) => [a.id, a]));
    for (const la of liveArr) {
      if (byId.has(la.id)) byId.set(la.id, la); // Use the latest live state.
    }
    const overridden = items.map((a) => byId.get(a.id) ?? a);
    // Filtered views follow database snapshots so live imports cannot bypass active filters.
    const fresh =
      activeFilters === 0 ? liveArr.filter((la) => !byId.has(la.id) && matchesFilter(la)) : [];
    return [...fresh.sort((x, y) => y.importedAt - x.importedAt), ...overridden];
  }, [activeFilters, items, liveAssets, filter, missingOnly]);
  const selectedAssets = useMemo(
    () => mergedItems.filter((a) => selected.has(a.id)),
    [mergedItems, selected],
  );
  const allLoadedSelected =
    mergedItems.length > 0 && mergedItems.every((asset) => selected.has(asset.id));
  const toggleSelect = (id: number, additive: boolean) => {
    setSelected((prev) => {
      const next = additive ? new Set(prev) : new Set<number>();
      if (prev.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
    if (!additive) setSelectMode(false);
  };
  const importViaDialog = async () => {
    const res = await openFileDialog({
      multiple: true,
      directory: false,
      title: t("assets.import"),
    });
    const paths = Array.isArray(res) ? res : res ? [res] : [];
    if (paths.length === 0) return;
    useImportStore.getState().enqueue(paths, importFolder ?? null, "paths", filter.tags);
  };
  const doTrash = async (assets: Asset[]) => {
    try {
      await api.assetTrash(assets.map((a) => a.id));
      clearLiveAssets();
      void queryClient.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.folders.library(libraryId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.storageStats.library(libraryId) });
      setSelected(new Set());
      toast.success(t("common.operationSuccess"));
    } catch (e) {
      toast.error(explainError(e));
    }
  };
  const empty = !isLoading && mergedItems.length === 0 && pendingFiles.length === 0;
  return {
    filter,
    emptyHint,
    importFolder,
    showStorageStats,
    backTo,
    scopeName,
    t,
    location,
    navigate,
    searchParams,
    missingOnly,
    listFilter,
    setListFilter,
    showFilters,
    setShowFilters,
    canFilterFolder,
    activeFilters,
    total,
    isLoading,
    isError,
    isFetchingNextPage,
    fetchNextPage,
    hasNextPage,
    refetch,
    scrollRef,
    selected,
    setSelected,
    selectMode,
    setSelectMode,
    selectedListOpen,
    setSelectedListOpen,
    detailId,
    setDetailId,
    previewAsset,
    previewPagination: {
      scopeKey: previewScopeKey,
      total,
      hasMore: !!hasNextPage && !isPlaceholderData && !isFilterPending,
      loadMore: async () => {
        const result = await fetchNextPage({ cancelRefetch: false, throwOnError: true });
        if (result.isError) throw result.error;
        return result.data?.pages.flatMap((page) => page.items) ?? [];
      },
    },
    setPreviewAsset,
    tagsOpen,
    setTagsOpen,
    moveOpen,
    setMoveOpen,
    tagTargets,
    setTagTargets,
    moveTargets,
    setMoveTargets,
    moveLibOpen,
    setMoveLibOpen,
    indexBuildOpen,
    setIndexBuildOpen,
    progress,
    etaSeconds,
    pendingPreview,
    pendingFiles,
    mergedItems,
    selectedAssets,
    allLoadedSelected,
    toggleSelect,
    importViaDialog,
    doTrash,
    empty,
  };
}
