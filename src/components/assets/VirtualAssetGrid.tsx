import { useEffect, useMemo, useRef, useState, type RefObject, type ReactNode } from "react";
import { useAppStore } from "@/stores/app";
import { GRID_GAP, layoutAssets, visibleCells } from "@/lib/assetGridLayout";
import type { Asset } from "@/types";
interface Props {
  items: Asset[];
  scrollRef: RefObject<HTMLDivElement | null>;
  renderItem: (asset: Asset) => ReactNode;
  hasMore?: boolean;
  isLoadingMore: boolean;
  onLoadMore: () => void;
}
export default function VirtualAssetGrid({
  items,
  scrollRef,
  renderItem,
  hasMore,
  isLoadingMore,
  onLoadMore,
}: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const columnCount = useAppStore((state) => state.gridColumns);
  const [viewport, setViewport] = useState({ width: 0, top: 0, height: 0 });
  const layout = useMemo(
    () => layoutAssets(items, viewport.width, columnCount),
    [items, viewport.width, columnCount],
  );
  useEffect(() => {
    const grid = ref.current;
    const scroll = scrollRef.current;
    if (!grid || !scroll) return;
    let frame = 0;
    const measure = () => {
      frame = 0;
      const rect = grid.getBoundingClientRect();
      const top = scroll.getBoundingClientRect().top + scroll.clientTop - rect.top;
      const next = { width: rect.width, top, height: scroll.clientHeight };
      setViewport((previous) =>
        previous.width === next.width &&
        previous.top === next.top &&
        previous.height === next.height
          ? previous
          : next,
      );
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(measure);
    };
    measure();
    const observer = new ResizeObserver(schedule);
    observer.observe(scroll);
    observer.observe(grid);
    // Pending import tiles above the grid may change its offset without resizing it.
    if (grid.parentElement) observer.observe(grid.parentElement);
    scroll.addEventListener("scroll", schedule, { passive: true });
    return () => {
      observer.disconnect();
      scroll.removeEventListener("scroll", schedule);
      cancelAnimationFrame(frame);
    };
  }, [scrollRef, items]);
  // Keep at least a viewport of already mounted, decoding thumbnails on either side.
  const overscan = Math.max(1000, viewport.height);
  useEffect(() => {
    if (
      viewport.width &&
      hasMore &&
      !isLoadingMore &&
      layout.height - viewport.top - viewport.height < overscan * 2
    ) {
      onLoadMore();
    }
  }, [hasMore, isLoadingMore, layout.height, onLoadMore, overscan, viewport]);
  return (
    <div ref={ref} className="w-full">
      <div className="relative" style={{ height: viewport.width ? layout.height : 0 }}>
        {viewport.width > 0 &&
          layout.columns.flatMap((column, columnIndex) =>
            visibleCells(
              column,
              viewport.top - overscan,
              viewport.top + viewport.height + overscan,
            ).map((cell) => (
              <div
                key={items[cell.index].id}
                className="absolute"
                style={{
                  top: cell.top,
                  left: columnIndex * (layout.columnWidth + GRID_GAP),
                  width: layout.columnWidth,
                }}
              >
                {renderItem(items[cell.index])}
              </div>
            )),
          )}
      </div>
      <div className="flex h-10 items-center justify-center" aria-hidden={!isLoadingMore}>
        {isLoadingMore && (
          <div className="h-5 w-5 animate-spin rounded-full border-2 border-muted border-t-foreground" />
        )}
      </div>
    </div>
  );
}
