/** Infinite asset pagination uses 40 items per page by default. */
import { useEffect, useMemo } from "react";
import { infiniteQueryOptions, keepPreviousData, useInfiniteQuery } from "@tanstack/react-query";
import { useAppStore } from "@/stores/app";
import { api } from "@/lib/api";
import { queryKeys } from "@/lib/queryKeys";
import type { Asset, AssetFilter } from "@/types";
import { useDebouncedValue } from "@/hooks/useDebouncedValue";
import { useImportStore } from "@/stores/import";
export const PAGE_SIZE = 40;
interface AssetPageParam {
  page: number;
  cursorImportedAt?: number;
  cursorId?: number;
}
export function assetPagesOptions(filter: AssetFilter, libraryId?: string) {
  return infiniteQueryOptions({
    queryKey: queryKeys.assets.pages(libraryId, filter),
    initialPageParam: { page: 1 } as AssetPageParam,
    queryFn: ({ pageParam }) =>
      api.assetList(
        { ...filter, ...(pageParam as AssetPageParam), pageSize: PAGE_SIZE },
        libraryId,
      ),
    getNextPageParam: (last) => {
      if (!last.hasMore || last.items.length === 0) return undefined;
      const finalAsset = last.items[last.items.length - 1];
      return {
        page: last.page + 1,
        ...(filter.sort == null || filter.sort === "imported_desc"
          ? { cursorImportedAt: finalAsset.importedAt, cursorId: finalAsset.id }
          : {}),
      } satisfies AssetPageParam;
    },
  });
}
export function useAssetPages(filter: AssetFilter, enabled = true) {
  const libraryId = useAppStore((state) => state.current?.id);
  const debouncedFilter = useDebouncedValue(filter, 180);
  const query = useInfiniteQuery({
    ...assetPagesOptions(debouncedFilter, libraryId),
    enabled,
    placeholderData: (previous, previousQuery) =>
      previousQuery?.queryKey[1] === libraryId ? keepPreviousData(previous) : undefined,
  });
  const items: Asset[] = useMemo(
    () => query.data?.pages.flatMap((p) => p.items) ?? [],
    [query.data],
  );
  useEffect(() => {
    // A completed database snapshot takes ownership from transient event patches.
    // Otherwise a missed event could let an old live card mask every later refresh.
    if (query.data)
      useImportStore
        .getState()
        .reconcileLiveAssets(query.data.pages.flatMap((p) => p.items.map((a) => a.id)));
  }, [query.dataUpdatedAt, query.data]);
  return {
    ...query,
    items,
    total: query.data?.pages[0]?.total ?? 0,
    isFilterPending: JSON.stringify(filter) !== JSON.stringify(debouncedFilter),
  };
}
