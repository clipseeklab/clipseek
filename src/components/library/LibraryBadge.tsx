/** Library badge icon and color selection. */
import { Database, Palette } from "lucide-react";
import { useTranslation } from "react-i18next";
const PALETTE = [
  { key: "indigo", hex: "#6366f1" },
  { key: "emerald", hex: "#10b981" },
  { key: "rose", hex: "#f43f5e" },
  { key: "amber", hex: "#f59e0b" },
  { key: "sky", hex: "#0ea5e9" },
  { key: "violet", hex: "#8b5cf6" },
  { key: "teal", hex: "#14b8a6" },
  { key: "orange", hex: "#f97316" },
] as const;
export const DEFAULT_LIBRARY_COLOR = "indigo";
export function libraryColorHex(color?: string | null): string {
  if (color && /^#[0-9a-f]{6}$/i.test(color)) return color;
  return PALETTE.find((c) => c.key === color)?.hex ?? PALETTE[0].hex;
}
interface Props {
  color?: string | null;
  size?: number;
  strokeWidth?: number;
  className?: string;
}
/** Tint the library database icon directly using its badge color, without a circular background. */
export function LibraryColorIcon({ color, size = 16, strokeWidth = 2, className }: Props) {
  return (
    <Database
      size={size}
      strokeWidth={strokeWidth}
      color={libraryColorHex(color)}
      className={`shrink-0 ${className ?? ""}`}
    />
  );
}
/** One row of color swatches with a ring around the selected color. */
export function LibraryColorPicker({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap items-center gap-2.5">
      {PALETTE.map((c) => (
        <button
          key={c.key}
          type="button"
          title={c.key}
          className={`rounded-full transition-transform ${
            value === c.key
              ? "scale-110 ring-2 ring-foreground ring-offset-2 ring-offset-background"
              : "hover:scale-105"
          }`}
          style={{ width: 26, height: 26, backgroundColor: c.hex }}
          onClick={() => onChange(c.key)}
        />
      ))}
      <label
        className={`relative flex h-[26px] w-[26px] cursor-pointer items-center justify-center overflow-hidden rounded-full border bg-background transition-transform hover:scale-105 ${
          value.startsWith("#")
            ? "scale-110 ring-2 ring-foreground ring-offset-2 ring-offset-background"
            : ""
        }`}
        title={t("common.customColor")}
        style={value.startsWith("#") ? { backgroundColor: value } : undefined}
      >
        {!value.startsWith("#") && <Palette className="h-4 w-4" />}
        <input
          type="color"
          className="absolute inset-0 cursor-pointer opacity-0"
          value={libraryColorHex(value)}
          onChange={(event) => onChange(event.target.value)}
        />
      </label>
    </div>
  );
}
export const LIBRARY_PALETTE = PALETTE;
