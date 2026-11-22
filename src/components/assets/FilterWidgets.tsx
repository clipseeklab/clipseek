import { type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Search, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { FilterState } from "./assetFilterState";
export function ShapeFilter({
  value,
  onChange,
  label,
}: {
  value: FilterState;
  onChange: (value: FilterState) => void;
  label: string;
}) {
  const { t } = useTranslation();
  const presets = ["any", "landscape", "portrait", "16x9", "4x3", "1x1", "3x4", "9x16"];
  const selectShape = (shape: string) => onChange({ ...value, shape: shape.replace("x", ":") });
  const selectCustom = () =>
    onChange({
      ...value,
      shape: "custom",
      customAspectWidth: value.customAspectWidth || "5",
      customAspectHeight: value.customAspectHeight || "3",
    });
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="outline" size="sm" className="h-8 max-w-28 text-xs">
          <span className="truncate">{label}</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-64 p-2" align="start">
        <div className="grid grid-cols-2 gap-1">
          {presets.map((item) => {
            const shape = item.replace("x", ":");
            return (
              <button
                key={item}
                className={cn(
                  "flex h-8 items-center gap-2 rounded-md px-2 text-sm hover:bg-accent",
                  value.shape === shape && "bg-accent",
                )}
                onClick={() => selectShape(item)}
              >
                <span
                  className={cn(
                    "h-3.5 w-3.5 rounded border",
                    value.shape === shape && "border-primary bg-primary",
                  )}
                />
                {t(`search.shape.${item}`)}
              </button>
            );
          })}
        </div>
        <button
          className={cn(
            "mt-1 flex h-8 w-full items-center gap-2 rounded-md px-2 text-sm hover:bg-accent",
            value.shape === "custom" && "bg-accent",
          )}
          onClick={selectCustom}
        >
          <span
            className={cn(
              "h-3.5 w-3.5 rounded border",
              value.shape === "custom" && "border-primary bg-primary",
            )}
          />
          {t("search.shape.custom")}
        </button>
        {value.shape === "custom" && (
          <div className="mt-1 flex items-center justify-center gap-2 rounded-md bg-muted/40 p-2">
            <Input
              aria-label={t("search.shape.width")}
              inputMode="decimal"
              className="h-9 w-20 text-center"
              value={value.customAspectWidth}
              onChange={(e) =>
                onChange({ ...value, customAspectWidth: e.target.value.replace(/[^0-9.]/g, "") })
              }
            />
            <span className="text-sm text-muted-foreground">:</span>
            <Input
              aria-label={t("search.shape.height")}
              inputMode="decimal"
              className="h-9 w-20 text-center"
              value={value.customAspectHeight}
              onChange={(e) =>
                onChange({ ...value, customAspectHeight: e.target.value.replace(/[^0-9.]/g, "") })
              }
            />
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}
export function SearchMultiSelect({
  label,
  count,
  search,
  onSearch,
  searchPlaceholder,
  children,
}: {
  label: string;
  count: number;
  search: string;
  onSearch: (value: string) => void;
  searchPlaceholder: string;
  children: React.ReactNode;
}) {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="outline" size="sm" className="h-8 max-w-40 text-xs">
          <span className="truncate">{label}</span>
          {count > 0 && <span>({count})</span>}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-64 p-2" align="start">
        <div className="relative mb-2">
          <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            autoFocus
            className="h-8 pl-7 text-xs"
            value={search}
            onChange={(e) => onSearch(e.target.value)}
            placeholder={searchPlaceholder}
          />
        </div>
        <div className="max-h-56 overflow-y-auto">{children}</div>
      </PopoverContent>
    </Popover>
  );
}
export function OptionRow({
  label,
  suffix,
  checked,
  disabled,
  onToggle,
}: {
  label: string;
  suffix?: string;
  checked: boolean;
  disabled?: boolean;
  onToggle: () => void;
}) {
  return (
    <label
      className={cn(
        "flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-sm hover:bg-accent",
        disabled && "cursor-default opacity-60",
      )}
    >
      <Checkbox checked={checked} disabled={disabled} onCheckedChange={onToggle} />
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {suffix && <span className="text-xs text-muted-foreground">{suffix}</span>}
    </label>
  );
}
export function FilterChip({
  label,
  color,
  disabled,
  onRemove,
}: {
  label: ReactNode;
  color?: string;
  disabled?: boolean;
  onRemove: () => void;
}) {
  return (
    <span className="flex h-6 max-w-44 items-center gap-1 rounded bg-muted px-1.5 text-[11px]">
      {color && (
        <span
          className="h-2.5 w-2.5 shrink-0 rounded-full border"
          style={{ backgroundColor: color }}
        />
      )}
      <span className="truncate">{label}</span>
      {!disabled && (
        <button className="rounded p-0.5 hover:bg-background" onClick={onRemove}>
          <X className="h-2.5 w-2.5" />
        </button>
      )}
    </span>
  );
}
export function FilterSelect({
  value,
  onChange,
  options,
  label,
}: {
  value: string;
  onChange: (value: string) => void;
  options: string[][];
  label?: string;
}) {
  return (
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger aria-label={label} className="h-8 w-auto min-w-28 text-xs">
        {label && <span className="mr-1 text-muted-foreground">{label}</span>}
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {options.map(([key, label]) => (
          <SelectItem key={key} value={key}>
            {label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
