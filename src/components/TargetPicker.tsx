import { queryKeys } from "@/lib/queryKeys";
import { useState } from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Check, Search } from "lucide-react";
import { api } from "@/lib/api";
import { explainError } from "@/lib/errors";
import { useDebouncedValue } from "@/hooks/useDebouncedValue";
import { useAppStore } from "@/stores/app";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
export default function TargetPicker({
  kind,
  exclude,
  value,
  onChange,
  disabled,
  label,
}: {
  kind: "tags" | "folders";
  exclude: number[];
  value: string;
  onChange: (id: string) => void;
  disabled: boolean;
  label: string;
}) {
  const { t } = useTranslation();
  const libraryId = useAppStore((s) => s.current?.id);
  const [query, setQuery] = useState("");
  const debounced = useDebouncedValue(query, 200);
  const result = useInfiniteQuery({
    queryKey: queryKeys.targetOptions.search(libraryId, kind, debounced, exclude),
    initialPageParam: 0,
    queryFn: ({ pageParam }) => api.targetOptions(kind, debounced, pageParam, exclude),
    getNextPageParam: (last) => (last.hasMore ? last.items[last.items.length - 1]?.id : undefined),
    retry: false,
  });
  return (
    <div className="space-y-2">
      <div className="relative">
        <Search className="absolute left-3 top-3 h-4 w-4 text-muted-foreground" />
        <Input
          aria-label={label}
          placeholder={t("search.searchBtn") + "…"}
          className="pl-9"
          value={query}
          disabled={disabled}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>
      <div
        role="listbox"
        aria-label={label}
        className="max-h-60 overflow-y-auto rounded-md border p-1"
      >
        {result.data?.pages
          .flatMap((p) => p.items)
          .map((item) => (
            <button
              type="button"
              role="option"
              aria-selected={value === String(item.id)}
              key={item.id}
              disabled={disabled}
              className="flex w-full items-center justify-between gap-2 rounded p-2 text-left text-sm hover:bg-accent aria-selected:bg-accent"
              onClick={() => onChange(String(item.id))}
            >
              <span className="min-w-0 break-words">{item.name}</span>
              {value === String(item.id) && <Check className="h-4 w-4 shrink-0" />}
            </button>
          ))}
        {result.isPending && (
          <p role="status" className="p-3 text-sm">
            {t("common.loading")}
          </p>
        )}
        {result.isError && (
          <div role="alert" className="p-3 text-sm text-destructive">
            {explainError(result.error)}
            <Button variant="ghost" size="sm" onClick={() => void result.refetch()}>
              {t("common.retry")}
            </Button>
          </div>
        )}
        {result.data && !result.data.pages.some((p) => p.items.length) && (
          <p className="p-3 text-sm text-muted-foreground">{t("search.noResults")}</p>
        )}
        {result.hasNextPage && (
          <Button
            variant="ghost"
            size="sm"
            disabled={result.isFetchingNextPage || disabled}
            onClick={() => void result.fetchNextPage()}
            className="w-full"
          >
            {t("subtitles.loadMore")}
          </Button>
        )}
      </div>
    </div>
  );
}
