import { useTranslation } from "react-i18next";
import { Slider } from "@/components/ui/slider";
import { useAppStore } from "@/stores/app";
export default function GridSizeControl() {
  const { t } = useTranslation();
  const gridColumns = useAppStore((state) => state.gridColumns);
  const setGridColumns = useAppStore((state) => state.setGridColumns);
  return (
    <div className="flex items-center" title={t("layout.cardWidth")}>
      <Slider
        className="w-24"
        dir="rtl"
        min={2}
        max={6}
        step={1}
        value={[gridColumns]}
        onValueChange={([value]) => setGridColumns(value)}
        aria-label={t("layout.cardWidth")}
      />
    </div>
  );
}
