import type { Asset } from "@/types";
export const GRID_GAP = 10;
export function assetAspectRatio(asset: Pick<Asset, "width" | "height" | "kind">) {
  return asset.width && asset.height && asset.width > 0 && asset.height > 0
    ? asset.width / asset.height
    : asset.kind === "image"
      ? 4 / 3
      : 16 / 9;
}
export interface GridCell {
  index: number;
  top: number;
  height: number;
}
/** Keep the existing round-robin column order, including when pages are appended. */
export function layoutAssets(items: Asset[], width: number, columnCount: number) {
  const columnWidth = Math.max(1, (width - GRID_GAP * (columnCount - 1)) / columnCount);
  const columns: GridCell[][] = Array.from({ length: columnCount }, () => []);
  const heights = Array<number>(columnCount).fill(0);
  items.forEach((asset, index) => {
    const column = index % columnCount;
    // Border (2px), padding (12px), filename (16px), shared folder/tag row (24px).
    const height = (columnWidth - 2) / assetAspectRatio(asset) + 54;
    columns[column].push({ index, top: heights[column], height });
    heights[column] += height + GRID_GAP;
  });
  return { columns, columnWidth, height: Math.max(0, ...heights) - (items.length ? GRID_GAP : 0) };
}
/** Binary search avoids scanning every loaded asset on each scroll frame. */
export function visibleCells(column: GridCell[], start: number, end: number) {
  let low = 0;
  let high = column.length;
  while (low < high) {
    const mid = (low + high) >>> 1;
    if (column[mid].top + column[mid].height < start) low = mid + 1;
    else high = mid;
  }
  const cells: GridCell[] = [];
  for (let index = low; index < column.length && column[index].top <= end; index++) {
    cells.push(column[index]);
  }
  return cells;
}
