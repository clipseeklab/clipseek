import { queryKeys } from "@/lib/queryKeys";
import { useState } from "react";
import { useInfiniteQuery, useQueries, useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Check, Search } from "lucide-react";
import { api } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import { useDebouncedValue } from "@/hooks/useDebouncedValue";
import { explainError } from "@/lib/errors";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

export function FolderName({ id }: { id: string }) {
  const library = useAppStore((s) => s.current?.id);
  const { data } = useQuery({
    queryKey: queryKeys.folders.detail(library, Number(id)),
    queryFn: () => api.folderGet(Number(id)),
    enabled: Number(id) > 0,
  });
  return <>{data?.name ?? id}</>;
}

interface FolderPickerProps {
  values: string[];
  onChange: (values: string[]) => void;
  label: string;
  multiple?: boolean;
  disabled?: boolean;
  showNone?: boolean;
}

export default function FolderPicker({
  values,
  onChange,
  label,
  multiple = false,
  disabled = false,
  showNone = true,
}: FolderPickerProps) {
  const { t } = useTranslation();
  const library = useAppStore((s) => s.current?.id);
  const [query, setQuery] = useState("");
  const debounced = useDebouncedValue(query, 200);
  const result = useInfiniteQuery({
    queryKey: queryKeys.folders.options(library, debounced),
    initialPageParam: 1,
    queryFn: ({ pageParam }) => api.folderPage(debounced, pageParam, "name_asc"),
    getNextPageParam: (last) => (last.hasMore ? last.page + 1 : undefined),
    retry: false,
  });
  const pins = useQuery({
    queryKey: queryKeys.folders.sidebar(library),
    queryFn: api.folderSidebar,
    enabled: !debounced,
    retry: false,
  });
  const items = result.data?.pages.flatMap((page) => page.items) ?? [];
  const pinned = !debounced ? (pins.data?.items.slice(0, pins.data.displayLimit ?? 10) ?? []) : [];
  const pinnedIds = new Set(pinned.map((item) => item.id));
  // Resolve only selected IDs outside the loaded pages, without fetching the full list.
  const missing = values.filter(
    (id) =>
      id !== "none" &&
      !items.some((item) => String(item.id) === id) &&
      !pinned.some((item) => String(item.id) === id),
  );
  const selected = useQueries({
    queries: missing.map((id) => ({
      queryKey: queryKeys.folders.detail(library, Number(id)),
      queryFn: () => api.folderGet(Number(id)),
      retry: false,
    })),
  });
  const choose = (id: string) =>
    onChange(
      multiple
        ? values.includes(id)
          ? values.filter((value) => value !== id)
          : [...values, id]
        : [id],
    );
  const row = (id: string, name: string) => (
    <button
      key={id}
      type="button"
      role="option"
      aria-selected={values.includes(id)}
      disabled={disabled}
      title={name}
      className="flex w-full items-center justify-between gap-2 rounded-md px-3 py-2 text-left text-sm hover:bg-accent aria-selected:bg-accent disabled:opacity-50"
      onClick={() => choose(id)}
    >
      <span className="truncate">{name}</span>
      {values.includes(id) && <Check className="h-4 w-4 shrink-0" />}
    </button>
  );

  return (
    <div className="space-y-2">
      <div className="relative">
        <Search className="absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
        <Input
          aria-label={t("search.searchFolders")}
          placeholder={t("search.searchFolders")}
          value={query}
          disabled={disabled}
          onChange={(event) => setQuery(event.target.value)}
          className="pl-9"
        />
      </div>
      <div
        key={debounced}
        role="listbox"
        aria-label={label}
        aria-multiselectable={multiple || undefined}
        className="max-h-60 overflow-y-auto rounded-md border p-1"
        onScroll={(event) => {
          const el = event.currentTarget;
          if (
            query === debounced &&
            el.scrollHeight - el.scrollTop - el.clientHeight < 32 &&
            result.hasNextPage &&
            !result.isFetchingNextPage &&
            !result.isFetchNextPageError
          ) {
            void result.fetchNextPage({ cancelRefetch: false });
          }
        }}
      >
        {showNone && row("none", t("assets.uncategorized"))}
        {!debounced &&
          selected.map((item) => (item.data ? row(String(item.data.id), item.data.name) : null))}
        {!!pinned.length && (
          <p className="px-3 py-1 text-xs text-muted-foreground">{t("folders.pinned")}</p>
        )}
        {pinned.map((item) => row(String(item.id), item.name))}
        {!!pinned.length && (
          <p className="border-t px-3 py-1 text-xs text-muted-foreground">{t("folders.all")}</p>
        )}
        {items
          .filter((item) => !pinnedIds.has(item.id))
          .map((item) => row(String(item.id), item.name))}
        {result.isPending && (
          <p role="status" className="p-3 text-xs text-muted-foreground">
            {t("common.loading")}
          </p>
        )}
        {result.isError && (
          <div role="alert" className="p-2 text-xs text-destructive">
            {explainError(result.error)}
            <Button variant="ghost" size="sm" onClick={() => void result.refetch()}>
              {t("common.retry")}
            </Button>
          </div>
        )}
        {result.data && !items.length && (
          <p className="p-3 text-xs text-muted-foreground">{t("search.noResults")}</p>
        )}
        {result.hasNextPage && (
          <Button
            variant="ghost"
            size="sm"
            className="w-full"
            disabled={disabled || result.isFetchingNextPage || query !== debounced}
            onClick={() => void result.fetchNextPage({ cancelRefetch: false })}
          >
            {t("subtitles.loadMore")}
          </Button>
        )}
      </div>
    </div>
  );
}
