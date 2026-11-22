import { useAppStore } from "@/stores/app";
import { queryKeys } from "@/lib/queryKeys";
import { FilterProperty } from "./FilterProperty";
import { FilterChip } from "./FilterWidgets";
import {
  COLOR_SWATCH,
  EMPTY_FILTER,
  activeFilterCount,
  type FilterState,
  type PropertyKey,
} from "./assetFilterState";
export { EMPTY_FILTER, activeFilterCount, toAssetFilter } from "./assetFilterState";
export type { FilterState } from "./assetFilterState";
import { FolderName } from "@/components/folders/FolderPicker";
import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import {
  CalendarDays,
  FileText,
  Folder,
  GripVertical,
  HardDrive,
  Hash,
  Image,
  Palette,
  Pin,
  PinOff,
  Plus,
  Search,
  Tag,
} from "lucide-react";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
const PROPERTY_STORAGE_KEY = "clip-studio.filter-properties.v1";
const ALL_PROPERTIES: PropertyKey[] = [
  "visualIndex",
  "type",
  "folders",
  "tags",
  "colors",
  "shape",
  "resolution",
  "size",
  "date",
  "name",
  "assetId",
];
const DEFAULT_PINNED: PropertyKey[] = ["type", "folders", "tags", "colors", "shape"];
function loadPropertySettings(
  storageKey = PROPERTY_STORAGE_KEY,
  defaultPinned = DEFAULT_PINNED,
): {
  order: PropertyKey[];
  pinned: PropertyKey[];
} {
  if (typeof window === "undefined") return { order: ALL_PROPERTIES, pinned: defaultPinned };
  try {
    const saved = JSON.parse(localStorage.getItem(storageKey) ?? "null") as {
      order?: string[];
      pinned?: string[];
    } | null;
    const order = (saved?.order ?? []).filter((key): key is PropertyKey =>
      ALL_PROPERTIES.includes(key as PropertyKey),
    );
    const pinned = (saved?.pinned ?? defaultPinned).filter((key): key is PropertyKey =>
      ALL_PROPERTIES.includes(key as PropertyKey),
    );
    return { order: [...order, ...ALL_PROPERTIES.filter((key) => !order.includes(key))], pinned };
  } catch {
    return { order: ALL_PROPERTIES, pinned: defaultPinned };
  }
}
export function AssetFilters({
  value,
  onChange,
  showFolder = true,
  fixedTags = [],
  hideFixedTags = false,
  properties = ALL_PROPERTIES,
  defaultPinned = DEFAULT_PINNED,
  storageKey = PROPERTY_STORAGE_KEY,
  leading,
  extraActiveCount = 0,
  onReset,
}: {
  value: FilterState;
  onChange: (value: FilterState) => void;
  showFolder?: boolean;
  fixedTags?: string[];
  hideFixedTags?: boolean;
  properties?: PropertyKey[];
  defaultPinned?: PropertyKey[];
  storageKey?: string;
  leading?: ReactNode;
  extraActiveCount?: number;
  onReset?: () => void;
}) {
  const { t } = useTranslation();
  const libraryId = useAppStore((state) => state.current?.id);
  const { data: tags = [] } = useQuery({
    queryKey: queryKeys.tags.library(libraryId),
    queryFn: api.tagList,
  });
  const [tagSearch, setTagSearch] = useState("");
  const [settings, setSettings] = useState(() => loadPropertySettings(storageKey, defaultPinned));
  const [propertySearch, setPropertySearch] = useState("");
  const [draggedProperty, setDraggedProperty] = useState<PropertyKey | null>(null);
  useEffect(() => {
    try {
      localStorage.setItem(storageKey, JSON.stringify(settings));
    } catch {
      /* Keep session-only state if storage is unavailable. */
    }
  }, [settings, storageKey]);
  useEffect(() => {
    if (!draggedProperty) return;
    const stop = () => setDraggedProperty(null);
    window.addEventListener("pointerup", stop, { once: true });
    window.addEventListener("pointercancel", stop, { once: true });
    return () => {
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
    };
  }, [draggedProperty]);
  const toggle = (items: string[], item: string) =>
    items.includes(item) ? items.filter((v) => v !== item) : [...items, item];
  const selectedTags = [...new Set([...fixedTags, ...value.tags])];
  const visibleTags = hideFixedTags
    ? selectedTags.filter((tag) => !fixedTags.includes(tag))
    : selectedTags;
  const activeCount =
    activeFilterCount(value, fixedTags) - (hideFixedTags ? new Set(fixedTags).size : 0);
  const filteredTags = tags.filter((tag) =>
    tag.name.toLowerCase().includes(tagSearch.trim().toLowerCase()),
  );
  const shapeLabel = (shape: string) =>
    shape === "any"
      ? t("search.shape.any")
      : shape === "custom" && value.customAspectWidth && value.customAspectHeight
        ? `${value.customAspectWidth}:${value.customAspectHeight}`
        : t(`search.shape.${shape.replace(":", "x")}`);
  const propertyLabel = (key: PropertyKey) =>
    key === "visualIndex" || key === "subtitleIndex"
      ? t(key === "visualIndex" ? "indexManagement.visual" : "indexManagement.subtitles")
      : t(`search.properties.${key}`);
  const propertyIcons: Record<PropertyKey, typeof Tag> = {
    visualIndex: Image,
    subtitleIndex: FileText,
    type: Image,
    folders: Folder,
    tags: Tag,
    colors: Palette,
    shape: Image,
    resolution: Image,
    size: HardDrive,
    date: CalendarDays,
    name: FileText,
    assetId: Hash,
  };
  const clearProperty = (key: PropertyKey) => {
    if (key === "visualIndex" || key === "subtitleIndex") onChange({ ...value, [key]: "any" });
    if (key === "type") onChange({ ...value, kinds: [] });
    if (key === "folders") onChange({ ...value, folders: [] });
    if (key === "tags") onChange({ ...value, tags: [] });
    if (key === "colors") onChange({ ...value, colors: [] });
    if (key === "shape")
      onChange({ ...value, shape: "any", customAspectWidth: "", customAspectHeight: "" });
    if (key === "resolution") onChange({ ...value, resolution: "any" });
    if (key === "size") onChange({ ...value, sizeRange: "any" });
    if (key === "date") onChange({ ...value, dateRange: "any" });
    if (key === "name") onChange({ ...value, nameQuery: "" });
    if (key === "assetId") onChange({ ...value, assetId: "" });
  };
  const togglePinned = (key: PropertyKey) => {
    const pinned = settings.pinned.includes(key);
    setSettings((current) => ({
      ...current,
      pinned: pinned ? current.pinned.filter((item) => item !== key) : [...current.pinned, key],
    }));
    if (pinned) clearProperty(key);
  };
  const moveProperty = (target: PropertyKey) => {
    if (!draggedProperty || draggedProperty === target) return;
    setSettings((current) => {
      const order = current.order.filter((key) => key !== draggedProperty);
      order.splice(order.indexOf(target), 0, draggedProperty);
      return { ...current, order };
    });
  };
  const renderProperty = (key: PropertyKey) => (
    <FilterProperty
      key={key}
      property={key}
      value={value}
      onChange={onChange}
      showFolder={showFolder}
      fixedTags={fixedTags}
      visibleTags={visibleTags}
      selectedTags={selectedTags}
      filteredTags={filteredTags}
      tagSearch={tagSearch}
      setTagSearch={setTagSearch}
      shapeLabel={shapeLabel}
    />
  );
  return (
    <div className="space-y-1.5">
      <div className="flex flex-wrap items-center gap-2">
        {leading}
        {settings.order
          .filter((key) => properties.includes(key) && settings.pinned.includes(key))
          .map(renderProperty)}
        <Popover>
          <PopoverTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 shrink-0"
              aria-label={t("search.addFilter")}
              title={t("search.addFilter")}
            >
              <Plus className="h-4 w-4" />
            </Button>
          </PopoverTrigger>
          <PopoverContent className="w-72 p-0" align="start">
            <div className="border-b p-2">
              <div className="relative">
                <Search className="absolute left-2 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  autoFocus
                  className="h-9 pl-8"
                  value={propertySearch}
                  onChange={(e) => setPropertySearch(e.target.value)}
                  placeholder={t("search.searchProperties")}
                />
              </div>
            </div>
            <div
              className="max-h-80 touch-none select-none overflow-y-auto p-1.5"
              onPointerMove={(event) => {
                if (!draggedProperty) return;
                const row = document
                  .elementFromPoint(event.clientX, event.clientY)
                  ?.closest<HTMLElement>("[data-property-key]");
                const key = row?.dataset.propertyKey as PropertyKey | undefined;
                if (key) moveProperty(key);
              }}
              onPointerUp={() => setDraggedProperty(null)}
              onPointerCancel={() => setDraggedProperty(null)}
            >
              {settings.order
                .filter(
                  (key) =>
                    properties.includes(key) &&
                    (showFolder || key !== "folders") &&
                    propertyLabel(key).toLowerCase().includes(propertySearch.trim().toLowerCase()),
                )
                .map((key) => {
                  const Icon = propertyIcons[key];
                  const pinned = settings.pinned.includes(key);
                  return (
                    <div
                      key={key}
                      data-property-key={key}
                      className={cn(
                        "flex h-10 items-center gap-2 rounded-md px-1.5 hover:bg-accent",
                        draggedProperty === key && "opacity-50",
                      )}
                    >
                      <button
                        className="touch-none cursor-grab rounded p-0.5 text-muted-foreground active:cursor-grabbing"
                        aria-label={t("search.reorderProperty")}
                        onPointerDown={(event) => {
                          event.preventDefault();
                          event.stopPropagation();
                          setDraggedProperty(key);
                        }}
                      >
                        <GripVertical className="h-4 w-4 shrink-0" />
                      </button>
                      <Icon className="h-4 w-4 shrink-0" />
                      <span className="min-w-0 flex-1 truncate text-sm">{propertyLabel(key)}</span>
                      <button
                        className="rounded p-1.5 text-muted-foreground hover:bg-background hover:text-foreground"
                        aria-label={pinned ? t("search.unpinProperty") : t("search.pinProperty")}
                        onClick={() => togglePinned(key)}
                      >
                        {pinned ? <Pin className="h-4 w-4" /> : <PinOff className="h-4 w-4" />}
                      </button>
                    </div>
                  );
                })}
            </div>
          </PopoverContent>
        </Popover>
      </div>

      {activeCount + extraActiveCount > 0 && (
        <div className="flex flex-wrap items-center gap-1 border-t pt-1.5">
          {value.kinds.map((kind) => (
            <FilterChip
              key={`k-${kind}`}
              label={t(`kind.${kind}`)}
              onRemove={() => onChange({ ...value, kinds: toggle(value.kinds, kind) })}
            />
          ))}
          {showFolder &&
            value.folders.map((id) => (
              <FilterChip
                key={`f-${id}`}
                label={
                  <>
                    {t("search.folder")}:{" "}
                    {id === "none" ? t("assets.uncategorized") : <FolderName id={id} />}
                  </>
                }
                onRemove={() => onChange({ ...value, folders: toggle(value.folders, id) })}
              />
            ))}
          {visibleTags.map((tag) => (
            <FilterChip
              key={`t-${tag}`}
              label={`# ${tag}`}
              disabled={fixedTags.includes(tag)}
              onRemove={() => onChange({ ...value, tags: toggle(value.tags, tag) })}
            />
          ))}
          {value.colors.map((color) => (
            <FilterChip
              key={`c-${color}`}
              label={t(`color.${color}`)}
              color={COLOR_SWATCH[color]}
              onRemove={() => onChange({ ...value, colors: toggle(value.colors, color) })}
            />
          ))}
          {value.shape !== "any" && (
            <FilterChip
              label={`${t("search.shape.label")}: ${shapeLabel(value.shape)}`}
              onRemove={() =>
                onChange({ ...value, shape: "any", customAspectWidth: "", customAspectHeight: "" })
              }
            />
          )}
          {value.resolution !== "any" && (
            <FilterChip
              label={t(`search.res${value.resolution}`)}
              onRemove={() => onChange({ ...value, resolution: "any" })}
            />
          )}
          {value.sizeRange !== "any" && (
            <FilterChip
              label={t(`search.imageSize.${value.sizeRange}`)}
              onRemove={() => onChange({ ...value, sizeRange: "any" })}
            />
          )}
          {value.dateRange !== "any" && (
            <FilterChip
              label={t(`search.dateRange.${value.dateRange}`)}
              onRemove={() => onChange({ ...value, dateRange: "any" })}
            />
          )}
          {value.nameQuery.trim() && (
            <FilterChip
              label={`${t("search.filterName")}: ${value.nameQuery.trim()}`}
              onRemove={() => onChange({ ...value, nameQuery: "" })}
            />
          )}
          {value.assetId.trim() && (
            <FilterChip
              label={`${t("search.filterAssetId")}: ${value.assetId.trim()}`}
              onRemove={() => onChange({ ...value, assetId: "" })}
            />
          )}
          {activeCount + extraActiveCount > (hideFixedTags ? 0 : fixedTags.length) && (
            <Button
              variant="ghost"
              size="sm"
              className="ml-auto h-6 px-2 text-xs"
              onClick={() => {
                onChange(EMPTY_FILTER);
                onReset?.();
              }}
            >
              {t("search.clearFilters")}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
