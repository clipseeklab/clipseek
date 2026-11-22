import { useAppStore } from "@/stores/app";
import { queryKeys } from "@/lib/queryKeys";
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useLocation, useNavigate, useSearchParams } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { Film, X } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/components/Toaster";
import { useImportStore } from "@/stores/import";
import type { Asset, AssetFilter, SearchHit } from "@/types";
import {
  EMPTY_FILTER,
  activeFilterCount,
  toAssetFilter,
  type FilterState,
} from "@/components/assets/AssetFilters";
import { useDebouncedValue } from "@/hooks/useDebouncedValue";
import { useSearchKeyword } from "@/hooks/useSearchKeyword";
import { explainError } from "@/lib/errors";
const PAGE_SIZE = 40;
interface SimilarSearchSnapshot {
  query: string;
  submittedQuery: string;
  imageQuery: string | null;
  results: SearchHit[];
  page: number;
  hasMore: boolean;
  filter: FilterState;
  scrollTop: number;
}
export function useVisualSearch({
  modeSwitch,
  active,
  docked,
  onSearchStart,
}: {
  modeSwitch: ReactNode;
  active: boolean;
  docked: boolean;
  onSearchStart: () => void;
}) {
  const { t } = useTranslation();
  const libraryId = useAppStore((state) => state.current?.id);
  const location = useLocation();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const scopeValue = Number(searchParams.get("assetId"));
  const scopeId = Number.isSafeInteger(scopeValue) && scopeValue > 0 ? scopeValue : undefined;
  const routeQuery = searchParams.get("q");
  const frameSearchIntent = location.state?.videoFrameSearchIntent;
  const { data: scopeAsset, isError: scopeError } = useQuery({
    queryKey: queryKeys.assetDetail.item(libraryId, scopeId),
    queryFn: () => api.assetDetail(scopeId!),
    enabled: scopeId != null,
  });
  const queryClient = useQueryClient();
  const [query, setQuery] = useSearchKeyword();
  const [submittedQuery, setSubmittedQuery] = useState("");
  const [searchMode, setSearchMode] = useState(false);
  const input = query;
  const setInput = setQuery;
  /** Selected or dropped image-search query path. */
  const [imageQuery, setImageQuery] = useState<string | null>(null);
  const [results, setResults] = useState<SearchHit[]>([]);
  const previewAssets = useMemo(() => {
    const seen = new Set<number>();
    return results.flatMap(({ asset }) => {
      if (seen.has(asset.id)) return [];
      seen.add(asset.id);
      return [asset];
    });
  }, [results]);
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(false);
  const [needsModels, setNeedsModels] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [filter, setFilter] = useState<FilterState>(EMPTY_FILTER);
  const [showFilters, setShowFilters] = useState(false);
  const [detailId, setDetailId] = useState<number | null>(null);
  const [previewTarget, setPreviewTarget] = useState<{
    asset: Asset;
    startMs: number;
  } | null>(null);
  const [tagTargets, setTagTargets] = useState<Asset[]>([]);
  const [moveTargets, setMoveTargets] = useState<Asset[]>([]);
  const [tagsOpen, setTagsOpen] = useState(false);
  const [moveOpen, setMoveOpen] = useState(false);
  const [moveLibOpen, setMoveLibOpen] = useState(false);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [selectMode, setSelectMode] = useState(false);
  const [similarHistory, setSimilarHistory] = useState<SimilarSearchSnapshot[]>([]);
  const resultScrollRef = useRef<HTMLDivElement>(null);
  const restoreScrollRef = useRef<number | null>(null);
  const latestRequestRef = useRef(0);
  const debouncedFilter = useDebouncedValue(filter, 180);
  useEffect(() => {
    latestRequestRef.current += 1;
    setResults([]);
    setSelected(new Set());
    setSelectMode(false);
    setSimilarHistory([]);
    setDetailId(null);
    setPreviewTarget(null);
    setTagTargets([]);
    setMoveTargets([]);
    setTagsOpen(false);
    setMoveOpen(false);
    setMoveLibOpen(false);
    setSearchMode(false);
    setImageQuery(null);
    setSubmittedQuery("");
    setHasMore(false);
    setLoading(false);
    return () => {
      latestRequestRef.current += 1;
    };
  }, [libraryId]);
  const searchDropPath = useImportStore((s) => s.searchDropPath);
  const setDropMode = useImportStore((s) => s.setDropMode);
  const setSearchDropPath = useImportStore((s) => s.setSearchDropPath);
  const buildAssetFilter = useMemo(
    () => (): AssetFilter => {
      const result = toAssetFilter(debouncedFilter);
      if (scopeId != null) result.assetId = scopeId;
      return result;
    },
    [debouncedFilter, scopeId],
  );
  const runSearch = useCallback(
    async (
      p: number,
      append: boolean,
      requestQuery: string = submittedQuery,
      requestImage: string | null = imageQuery,
      _trigger: string = "submit",
    ) => {
      const requestId = ++latestRequestRef.current;
      const valid = () =>
        requestId === latestRequestRef.current && libraryId === useAppStore.getState().current?.id;
      setLoading(true);
      try {
        const f = buildAssetFilter();
        const res = requestImage
          ? await api.searchImage(requestImage, f, p, PAGE_SIZE)
          : await api.searchText(requestQuery, f, p, PAGE_SIZE);
        if (!valid()) return;
        setResults((prev) => (append ? [...prev, ...res.items] : res.items));
        setPage(p);
        setHasMore(p * PAGE_SIZE < res.total);
      } catch (e) {
        if (valid()) {
          try {
            if (!(await api.modelStatus()).ready) {
              if (valid()) setNeedsModels(true);
              return;
            }
          } catch {
            /* Keep the original request error when model status is unavailable. */
          }
          if (valid()) toast.error(explainError(e));
        }
      } finally {
        if (valid()) setLoading(false);
      }
    },
    [imageQuery, submittedQuery, buildAssetFilter, libraryId],
  );
  const doTextSearch = useCallback(
    (q: string, trigger: string = "submit") => {
      const query = q.trim();
      if (!query) return;
      setSearchMode(true);
      onSearchStart();
      setQuery(query);
      setSubmittedQuery(query);
      setImageQuery(null);
      void runSearch(1, false, query, null, trigger);
    },
    [onSearchStart, runSearch, setQuery],
  );
  const doImageSearch = useCallback(
    (path: string) => {
      setSearchMode(true);
      onSearchStart();
      setImageQuery(path);
      setQuery("");
      setSubmittedQuery("");
      void runSearch(1, false, "", path);
    },
    [onSearchStart, runSearch, setQuery],
  );
  const lastImageIntent = useRef<string | null>(null);
  const lastTextIntent = useRef<{
    active: boolean;
    query: string | null;
    frame: unknown;
    libraryId?: string;
  } | null>(null);
  useEffect(() => {
    return () => {
      // Mount cleanup invalidates requests; replay must also be able to consume their intents.
      lastImageIntent.current = null;
      lastTextIntent.current = null;
    };
  }, []);
  // Consume each route intent once; callback dependencies may change after a search.
  useEffect(() => {
    if (searchDropPath) {
      doImageSearch(searchDropPath);
      setSearchDropPath(null);
    }
  }, [searchDropPath, doImageSearch, setSearchDropPath]);
  useEffect(() => {
    if (lastImageIntent.current === location.key) return;
    lastImageIntent.current = location.key;
    const path = (
      location.state as {
        imageSearchPath?: string;
      } | null
    )?.imageSearchPath;
    if (path) doImageSearch(path);
  }, [location.key, location.state, doImageSearch]);
  useEffect(() => {
    const previous = lastTextIntent.current;
    if (
      previous?.active === active &&
      previous.query === routeQuery &&
      previous.frame === frameSearchIntent &&
      previous.libraryId === libraryId
    )
      return;
    lastTextIntent.current = { active, query: routeQuery, frame: frameSearchIntent, libraryId };
    if (active && routeQuery?.trim()) doTextSearch(routeQuery, "route");
  }, [active, routeQuery, frameSearchIntent, doTextSearch, libraryId]);
  const findSimilar = (path: string) => {
    setSimilarHistory((history) => [
      ...history,
      {
        query,
        submittedQuery,
        imageQuery,
        results,
        page,
        hasMore,
        filter,
        scrollTop: resultScrollRef.current?.scrollTop ?? 0,
      },
    ]);
    setSelected(new Set());
    setSelectMode(false);
    setResults([]);
    doImageSearch(path);
  };
  const goBack = () => {
    const previous = similarHistory[similarHistory.length - 1];
    if (previous) {
      // Ignore any unfinished request from the similar-image search.
      latestRequestRef.current += 1;
      setSimilarHistory((history) => history.slice(0, -1));
      setQuery(previous.query);
      setSubmittedQuery(previous.submittedQuery);
      setImageQuery(previous.imageQuery);
      setResults(previous.results);
      setPage(previous.page);
      setHasMore(previous.hasMore);
      setFilter(previous.filter);
      setLoading(false);
      setSelected(new Set());
      setSelectMode(false);
      restoreScrollRef.current = previous.scrollTop;
    } else if (location.state?.backgroundLocation) {
      void navigate(-1);
    } else if (scopeId != null) {
      void navigate("/assets");
    } else {
      latestRequestRef.current += 1;
      setSearchMode(false);
      setImageQuery(null);
      setQuery("");
      setInput("");
      setResults([]);
      setLoading(false);
    }
  };
  useEffect(() => {
    if (restoreScrollRef.current == null || !resultScrollRef.current) return;
    resultScrollRef.current.scrollTop = restoreScrollRef.current;
    restoreScrollRef.current = null;
  }, [results]);
  const pickImage = async () => {
    const res = await openFileDialog({
      multiple: false,
      title: t("search.imageMode"),
      filters: [
        {
          name: t("assets.images"),
          extensions: ["jpg", "jpeg", "png", "webp", "bmp", "svg", "heic", "heif"],
        },
      ],
    });
    if (typeof res === "string" && libraryId === useAppStore.getState().current?.id)
      doImageSearch(res);
  };
  // Only a changed filter or scope triggers automatic search; callbacks still use current state.
  const previousAutomaticSearch = useRef({ filter: debouncedFilter, scopeId });
  useEffect(() => {
    const previous = previousAutomaticSearch.current;
    if (previous.filter === debouncedFilter && previous.scopeId === scopeId) return;
    previousAutomaticSearch.current = { filter: debouncedFilter, scopeId };
    if (submittedQuery || imageQuery)
      void runSearch(1, false, submittedQuery, imageQuery, "filter_change");
    setSelected(new Set());
    setSelectMode(false);
  }, [debouncedFilter, scopeId, submittedQuery, imageQuery, runSearch]);
  const searched = docked || searchMode;
  const scopeLabel = scopeAsset?.displayName || scopeAsset?.fileName || `#${scopeId}`;
  const scopeBadge = scopeId != null && (
    <div className="flex min-w-0 items-center gap-2 text-sm">
      <Film className="h-4 w-4 shrink-0 text-muted-foreground" />
      <button
        className="max-w-sm truncate text-primary hover:underline"
        onClick={() => setDetailId(scopeId)}
        title={t("search.onlyVideo", { name: scopeLabel })}
      >
        {t("search.onlyVideo", { name: scopeLabel })}
      </button>
      <button
        aria-label={t("search.allAssets")}
        title={t("search.allAssets")}
        className="rounded p-1 hover:bg-muted"
        onClick={() => {
          setSearchParams(
            (params) => {
              params.delete("assetId");
              return params;
            },
            { replace: true, state: location.state },
          );
        }}
      >
        <X className="h-4 w-4" />
      </button>
      {scopeError && (
        <span role="alert" className="text-destructive">
          {t("search.scopeUnavailable")}
        </span>
      )}
    </div>
  );
  const activeFilters = activeFilterCount(filter);
  const selectedAssets = useMemo(() => {
    const byAssetId = new Map<number, Asset>();
    results
      .filter((hit) => selected.has(hit.frameId))
      .forEach((hit) => byAssetId.set(hit.asset.id, hit.asset));
    return [...byAssetId.values()];
  }, [results, selected]);
  const trashAssets = async (assets: Asset[]) => {
    if (libraryId !== useAppStore.getState().current?.id) return;
    try {
      await api.assetTrash(assets.map((asset) => asset.id));
      if (libraryId !== useAppStore.getState().current?.id) return;
      setResults((items) =>
        items.filter((hit) => !assets.some((asset) => asset.id === hit.asset.id)),
      );
      setSelected(new Set());
      setSelectMode(false);
      void queryClient.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
      toast.success(t("assets.movedToTrash"));
    } catch (error) {
      toast.error(explainError(error));
    }
  };
  return {
    modeSwitch,
    active,
    t,
    location,
    scopeId,
    query,
    setQuery,
    searchMode,
    input,
    setInput,
    imageQuery,
    setImageQuery,
    results,
    setResults,
    previewAssets,
    page,
    loading,
    needsModels,
    setNeedsModels,
    hasMore,
    filter,
    setFilter,
    showFilters,
    setShowFilters,
    detailId,
    setDetailId,
    previewTarget,
    setPreviewTarget,
    tagTargets,
    setTagTargets,
    moveTargets,
    setMoveTargets,
    tagsOpen,
    setTagsOpen,
    moveOpen,
    setMoveOpen,
    moveLibOpen,
    setMoveLibOpen,
    selected,
    setSelected,
    selectMode,
    setSelectMode,
    similarHistory,
    resultScrollRef,
    setDropMode,
    runSearch,
    doTextSearch,
    findSimilar,
    goBack,
    pickImage,
    searched,
    scopeBadge,
    activeFilters,
    selectedAssets,
    trashAssets,
  };
}
