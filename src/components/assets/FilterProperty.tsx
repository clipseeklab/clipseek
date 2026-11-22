import FolderPicker from "@/components/folders/FolderPicker";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import type { Tag as TagModel } from "@/types";
import { COLOR_SWATCH, type FilterState, type PropertyKey } from "./assetFilterState";
import { ShapeFilter, SearchMultiSelect, OptionRow, FilterSelect } from "./FilterWidgets";
export function FilterProperty({
  property: key,
  value,
  onChange,
  showFolder,
  fixedTags,
  visibleTags,
  selectedTags,
  filteredTags,
  tagSearch,
  setTagSearch,
  shapeLabel,
}: {
  property: PropertyKey;
  value: FilterState;
  onChange: (value: FilterState) => void;
  showFolder: boolean;
  fixedTags: string[];
  visibleTags: string[];
  selectedTags: string[];
  filteredTags: TagModel[];
  tagSearch: string;
  setTagSearch: (value: string) => void;
  shapeLabel: (value: string) => string;
}) {
  const { t } = useTranslation();
  const toggle = (items: string[], item: string) =>
    items.includes(item) ? items.filter((v) => v !== item) : [...items, item];
  if (key === "visualIndex" || key === "subtitleIndex")
    return (
      <FilterSelect
        key={key}
        value={value[key] ?? "any"}
        label={t(key === "visualIndex" ? "indexManagement.visual" : "indexManagement.subtitles")}
        options={[
          ["any", t("indexManagement.all")],
          ["indexed", t("indexManagement.indexed")],
          ["unindexed", t("indexManagement.unindexed")],
        ]}
        onChange={(next) => onChange({ ...value, [key]: next })}
      />
    );
  if (key === "folders" && !showFolder) return null;
  if (key === "type")
    return (
      <div key={key} className="flex h-8 items-center gap-1 rounded-md border p-0.5">
        {(["image", "video", "gif"] as const).map((kind) => (
          <button
            key={kind}
            className={cn(
              "h-7 rounded px-2.5 text-xs transition-colors",
              value.kinds.includes(kind) && "bg-primary text-primary-foreground",
            )}
            onClick={() => onChange({ ...value, kinds: toggle(value.kinds, kind) })}
          >
            {t(`kind.${kind}`)}
          </button>
        ))}
      </div>
    );
  if (key === "folders")
    return (
      <Popover key={key}>
        <PopoverTrigger asChild>
          <Button variant="outline" size="sm" className="h-8 text-xs">
            {t("search.filterFolders")}
            {value.folders.length ? ` (${value.folders.length})` : ""}
          </Button>
        </PopoverTrigger>
        <PopoverContent className="w-80">
          <FolderPicker
            multiple
            values={value.folders}
            onChange={(folders) => onChange({ ...value, folders })}
            label={t("search.filterFolders")}
          />
        </PopoverContent>
      </Popover>
    );
  if (key === "tags")
    return (
      <SearchMultiSelect
        key={key}
        label={t("search.filterTags")}
        count={visibleTags.length}
        search={tagSearch}
        onSearch={setTagSearch}
        searchPlaceholder={t("search.searchTags")}
      >
        {filteredTags.map((tag) => (
          <OptionRow
            key={tag.id}
            label={tag.name}
            suffix={String(tag.assetCount)}
            checked={selectedTags.includes(tag.name)}
            disabled={fixedTags.includes(tag.name)}
            onToggle={() => onChange({ ...value, tags: toggle(value.tags, tag.name) })}
          />
        ))}
      </SearchMultiSelect>
    );
  if (key === "colors")
    return (
      <Popover key={key}>
        <PopoverTrigger asChild>
          <Button variant="outline" size="sm" className="h-8 text-xs">
            {t("search.filterColor")}
            {value.colors.length ? ` (${value.colors.length})` : ""}
          </Button>
        </PopoverTrigger>
        <PopoverContent className="w-64 p-3" align="start">
          <div className="grid grid-cols-6 gap-2">
            {Object.entries(COLOR_SWATCH).map(([name, color]) => {
              const label = t(`color.${name}`);
              const selected = value.colors.includes(name);
              return (
                <button
                  key={name}
                  type="button"
                  aria-label={label}
                  aria-pressed={selected}
                  title={label}
                  className={cn(
                    "flex h-8 w-8 items-center justify-center justify-self-center rounded-md border transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
                    selected && "border-primary bg-accent ring-1 ring-primary",
                  )}
                  onClick={() => onChange({ ...value, colors: toggle(value.colors, name) })}
                >
                  <span
                    className="h-5 w-5 rounded-full border shadow-sm"
                    style={{ backgroundColor: color }}
                  />
                </button>
              );
            })}
          </div>
        </PopoverContent>
      </Popover>
    );
  if (key === "shape")
    return (
      <ShapeFilter key={key} value={value} onChange={onChange} label={shapeLabel(value.shape)} />
    );
  if (key === "resolution")
    return (
      <FilterSelect
        key={key}
        value={value.resolution}
        onChange={(resolution) => onChange({ ...value, resolution })}
        options={[
          ["any", t("search.anyRes")],
          ["720", t("search.res720")],
          ["1080", t("search.res1080")],
          ["2k", t("search.res2k")],
          ["4k", t("search.res4k")],
        ]}
      />
    );
  if (key === "size")
    return (
      <FilterSelect
        key={key}
        value={value.sizeRange}
        onChange={(sizeRange) => onChange({ ...value, sizeRange })}
        options={[
          ["any", t("search.imageSize.any")],
          ["small", t("search.imageSize.small")],
          ["medium", t("search.imageSize.medium")],
          ["large", t("search.imageSize.large")],
          ["huge", t("search.imageSize.huge")],
        ]}
      />
    );
  if (key === "date")
    return (
      <FilterSelect
        key={key}
        value={value.dateRange}
        onChange={(dateRange) => onChange({ ...value, dateRange })}
        options={[
          ["any", t("search.dateRange.any")],
          ["today", t("search.dateRange.today")],
          ["week", t("search.dateRange.week")],
          ["month", t("search.dateRange.month")],
          ["quarter", t("search.dateRange.quarter")],
        ]}
      />
    );
  if (key === "assetId")
    return (
      <Input
        key={key}
        className="h-8 w-32 text-xs"
        inputMode="numeric"
        aria-label={t("search.filterAssetId")}
        placeholder={t("search.filterAssetId")}
        value={value.assetId}
        onChange={(e) => onChange({ ...value, assetId: e.target.value.replace(/\D/g, "") })}
      />
    );
  return (
    <Input
      key={key}
      className="h-8 w-36 text-xs"
      aria-label={t("search.filterName")}
      placeholder={t("search.filterName")}
      value={value.nameQuery}
      onChange={(e) => onChange({ ...value, nameQuery: e.target.value })}
    />
  );
}
