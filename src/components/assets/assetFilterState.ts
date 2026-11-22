import type { AssetFilter } from "@/types";
export const COLOR_SWATCH: Record<string, string> = {
  red: "#dc2626",
  orange: "#ea580c",
  yellow: "#eab308",
  green: "#16a34a",
  cyan: "#0891b2",
  blue: "#2563eb",
  purple: "#9333ea",
  magenta: "#db2777",
  brown: "#92400e",
  black: "#171717",
  white: "#f5f5f5",
  gray: "#a3a3a3",
};
export interface FilterState {
  assetId: string;
  visualIndex?: string;
  subtitleIndex?: string;
  tags: string[];
  folders: string[]; // none | <id>
  kinds: string[];
  colors: string[];
  shape: string;
  customAspectWidth: string;
  customAspectHeight: string;
  resolution: string;
  sizeRange: string;
  dateRange: string;
  nameQuery: string;
}
export const EMPTY_FILTER: FilterState = {
  visualIndex: "any",
  subtitleIndex: "any",
  assetId: "",
  tags: [],
  folders: [],
  kinds: [],
  colors: [],
  shape: "any",
  customAspectWidth: "",
  customAspectHeight: "",
  resolution: "any",
  sizeRange: "any",
  dateRange: "any",
  nameQuery: "",
};
export function activeFilterCount(filter: FilterState, fixedTags: string[] = []) {
  return (
    (filter.visualIndex && filter.visualIndex !== "any" ? 1 : 0) +
    (filter.subtitleIndex && filter.subtitleIndex !== "any" ? 1 : 0) +
    (filter.assetId.trim() ? 1 : 0) +
    new Set([...fixedTags, ...filter.tags]).size +
    filter.folders.length +
    filter.kinds.length +
    filter.colors.length +
    (filter.shape !== "any" ? 1 : 0) +
    (filter.resolution !== "any" ? 1 : 0) +
    (filter.sizeRange !== "any" ? 1 : 0) +
    (filter.dateRange !== "any" ? 1 : 0) +
    (filter.nameQuery.trim() ? 1 : 0)
  );
}
export function toAssetFilter(filter: FilterState): AssetFilter {
  const result: AssetFilter = {};
  if (filter.visualIndex && filter.visualIndex !== "any")
    result.visualIndexed = filter.visualIndex === "indexed";
  if (filter.subtitleIndex && filter.subtitleIndex !== "any")
    result.subtitleIndexed = filter.subtitleIndex === "indexed";
  const assetId = Number(filter.assetId);
  if (Number.isSafeInteger(assetId) && assetId > 0) result.assetId = assetId;
  if (filter.tags.length) result.tags = filter.tags;
  const ids = filter.folders
    .filter((v) => v !== "none")
    .map(Number)
    .filter(Number.isFinite);
  if (ids.length) result.folderIds = ids;
  if (filter.folders.includes("none")) result.uncategorized = true;
  if (filter.kinds.length) result.kinds = filter.kinds as AssetFilter["kinds"];
  if (filter.colors.length) result.colors = filter.colors;
  const ratios: Record<string, number> = {
    "16:9": 16 / 9,
    "4:3": 4 / 3,
    "1:1": 1,
    "3:4": 3 / 4,
    "9:16": 9 / 16,
  };
  if (filter.shape === "landscape") result.minAspect = 1.001;
  else if (filter.shape === "portrait") result.maxAspect = 0.999;
  else {
    const custom =
      filter.shape === "custom"
        ? Number(filter.customAspectWidth) / Number(filter.customAspectHeight)
        : ratios[filter.shape];
    if (Number.isFinite(custom) && custom > 0) {
      result.minAspect = custom * 0.97;
      result.maxAspect = custom * 1.03;
    }
  }
  const resolution = { "720": 720, "1080": 1080, "2k": 1920, "4k": 3840 }[filter.resolution];
  if (resolution) {
    result.minWidth = resolution;
    result.minHeight = resolution;
  }
  const sizes: Record<string, [number | undefined, number | undefined]> = {
    small: [undefined, 1024 * 1024],
    medium: [1024 * 1024, 50 * 1024 * 1024],
    large: [50 * 1024 * 1024, 500 * 1024 * 1024],
    huge: [500 * 1024 * 1024, undefined],
  };
  if (sizes[filter.sizeRange]) [result.minSize, result.maxSize] = sizes[filter.sizeRange];
  const seconds = { today: 86400, week: 604800, month: 2592000, quarter: 7776000 }[
    filter.dateRange
  ];
  if (seconds) result.dateFrom = Math.floor(Date.now() / 1000) - seconds;
  if (filter.nameQuery.trim()) result.query = filter.nameQuery.trim();
  return result;
}
export type PropertyKey =
  | "visualIndex"
  | "subtitleIndex"
  | "type"
  | "folders"
  | "tags"
  | "colors"
  | "shape"
  | "resolution"
  | "size"
  | "date"
  | "name"
  | "assetId";
