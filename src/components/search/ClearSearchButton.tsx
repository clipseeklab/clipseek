import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
export default function ClearSearchButton({
  onClear,
  floating = false,
}: {
  onClear: () => void;
  floating?: boolean;
}) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      aria-label={t("search.clearInput")}
      title={t("search.clearInput")}
      className={
        (floating ? "absolute right-3 top-1/2 -translate-y-1/2 " : "shrink-0 ") +
        "flex h-6 w-6 items-center justify-center rounded-sm text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      }
      onClick={(e) => {
        const input = e.currentTarget.parentElement?.querySelector("input");
        onClear();
        input?.focus();
      }}
    >
      <X className="h-4 w-4" />
    </button>
  );
}
