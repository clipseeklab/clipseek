import { Database, Ellipsis, List, ScanSearch, Trash2, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
interface Props {
  count: number;
  hasItems: boolean;
  allSelected: boolean;
  onToggleAll: () => void;
  onShowSelected: () => void;
  onAddTags: () => void;
  onMoveToFolder: () => void;
  onBuildIndex: () => void;
  onMoveToLibrary: () => void;
  onTrash: () => void;
  onClose: () => void;
}
export default function AssetSelectionToolbar({
  count,
  hasItems,
  allSelected,
  onToggleAll,
  onShowSelected,
  onAddTags,
  onMoveToFolder,
  onBuildIndex,
  onMoveToLibrary,
  onTrash,
  onClose,
}: Props) {
  const { t } = useTranslation();
  return (
    <div className="flex items-center gap-2 border-t bg-card px-6 py-2.5">
      <Button
        variant="ghost"
        size="sm"
        disabled={count === 0}
        aria-label={t("assets.selectedList", { count })}
        title={t("assets.viewSelected")}
        onClick={onShowSelected}
      >
        {t("assets.selection", { count })}
        <List className="h-4 w-4" />
      </Button>
      <Button variant="ghost" size="sm" disabled={!hasItems} onClick={onToggleAll}>
        <Checkbox
          checked={allSelected ? true : count > 0 ? "indeterminate" : false}
          className="pointer-events-none"
        />
        {t(allSelected ? "assets.clearSelection" : "assets.selectAll")}
      </Button>
      <div className="flex-1" />
      <Button variant="outline" size="sm" disabled={count === 0} onClick={onAddTags}>
        {t("assets.addTags")}
      </Button>
      <Button variant="outline" size="sm" disabled={count === 0} onClick={onMoveToFolder}>
        {t("assets.moveToFolder")}
      </Button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t("assets.batchActions")}
            title={t("assets.batchActions")}
            disabled={count === 0}
          >
            <Ellipsis />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onSelect={onBuildIndex}>
            <ScanSearch />
            {t("indexManagement.build")}
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={onMoveToLibrary}>
            <Database />
            {t("assets.moveToLibrary")}
          </DropdownMenuItem>
          <DropdownMenuItem variant="destructive" onSelect={onTrash}>
            <Trash2 />
            {t("assets.trash")}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={t("common.close")}
        title={t("common.close")}
        onClick={onClose}
      >
        <X />
      </Button>
    </div>
  );
}
