/** Distribute items across flex columns in input order, matching react-masonry-css layout. */
import { useEffect, useMemo, useRef } from "react";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/stores/app";
interface Props<T> {
  items: T[];
  renderItem: (item: T, index: number) => React.ReactNode;
  /** Stable identity; search results can distinguish internal units with frameId. */
  getKey?: (item: T, index: number) => string | number;
  /** Load another page near the scroll boundary. */
  onLoadMore?: () => void;
  hasMore?: boolean;
  isLoadingMore?: boolean;
  className?: string;
  /** Columns fill masonry vertically; rows preserve result order in a regular grid. */
  flow?: "columns" | "rows";
}
export default function MasonryGrid<T>({
  items,
  renderItem,
  getKey,
  onLoadMore,
  hasMore,
  isLoadingMore = false,
  className,
  flow = "columns",
}: Props<T>) {
  const sentinelRef = useRef<HTMLDivElement>(null);
  const gridColumns = useAppStore((state) => state.gridColumns);
  const columns = useMemo(() => {
    const next = Array.from(
      { length: gridColumns },
      () =>
        [] as Array<{
          item: T;
          index: number;
        }>,
    );
    items.forEach((item, index) => next[index % gridColumns].push({ item, index }));
    return next;
  }, [gridColumns, items]);
  // Trigger infinite loading when the sentinel enters the viewport.
  useEffect(() => {
    if (!onLoadMore || !hasMore || isLoadingMore) return;
    const el = sentinelRef.current;
    if (!el) return;
    const ob = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) onLoadMore();
      },
      { rootMargin: "600px 0px" },
    );
    ob.observe(el);
    return () => ob.disconnect();
  }, [onLoadMore, hasMore, isLoadingMore]);
  return (
    <div className={cn("w-full", className)}>
      {flow === "rows" ? (
        <div
          className="grid gap-[10px]"
          style={{ gridTemplateColumns: `repeat(${gridColumns}, minmax(0, 1fr))` }}
        >
          {items.map((item, index) => (
            <div key={getKey?.(item, index) ?? index} className="min-w-0">
              {renderItem(item, index)}
            </div>
          ))}
        </div>
      ) : (
        <div className="flex items-start gap-[10px]">
          {columns.map((column, columnIndex) => (
            <div key={columnIndex} className="min-w-0 flex-1 space-y-[10px]">
              {column.map(({ item, index }) => (
                <div key={getKey?.(item, index) ?? index} className="w-full">
                  {renderItem(item, index)}
                </div>
              ))}
            </div>
          ))}
        </div>
      )}
      <div
        ref={sentinelRef}
        className="flex h-10 items-center justify-center"
        aria-hidden={!isLoadingMore}
      >
        {isLoadingMore && (
          <div className="h-5 w-5 animate-spin rounded-full border-2 border-muted border-t-foreground" />
        )}
      </div>
    </div>
  );
}
