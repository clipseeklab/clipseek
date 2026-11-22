import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Tag as TagIcon, X } from "lucide-react";
import type { Tag } from "@/types";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
const PAGE_SIZE = 20;
export default function SelectedTagsDialog({
  ids,
  tags,
  onRemove,
  onClear,
  onOpenChange,
}: {
  ids: number[];
  tags: Tag[];
  onRemove: (id: number) => void;
  onClear: () => void;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const [page, setPage] = useState(1);
  const pages = Math.max(1, Math.ceil(ids.length / PAGE_SIZE));
  const currentPage = Math.min(page, pages);
  useEffect(() => setPage((value) => Math.min(value, pages)), [pages]);
  const byId = useMemo(() => new Map(tags.map((tag) => [tag.id, tag])), [tags]);
  const visible = ids.slice((currentPage - 1) * PAGE_SIZE, currentPage * PAGE_SIZE);
  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent disableAnimation className="flex max-h-[85vh] flex-col sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("tags.selectedList", { count: ids.length })}</DialogTitle>
          <DialogDescription>{t("tags.selectedHelp")}</DialogDescription>
        </DialogHeader>
        <div key={currentPage} className="max-h-[50vh] min-h-0 overflow-y-auto rounded-md border">
          {!ids.length && (
            <p className="p-6 text-center text-sm text-muted-foreground">{t("tags.noSelection")}</p>
          )}
          {visible.map((id) => {
            const tag = byId.get(id);
            const name = tag?.name ?? `#${id}`;
            return (
              <div key={id} className="flex items-center gap-3 border-b px-3 py-2 last:border-b-0">
                <TagIcon className="h-4 w-4 shrink-0 text-muted-foreground" />
                <div className="min-w-0 flex-1 text-sm">
                  <div className="truncate" title={name}>
                    {name}
                  </div>
                  {!tag && (
                    <span className="text-xs text-muted-foreground">
                      {t("tags.missingSelected")}
                    </span>
                  )}
                </div>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t("tags.deselectNamed", { name })}
                  onClick={() => onRemove(id)}
                >
                  <X className="h-3.5 w-3.5" />
                </Button>
              </div>
            );
          })}
        </div>
        <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
          <span>{t("tags.pageInfo", { page: currentPage, pages, count: ids.length })}</span>
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              disabled={currentPage <= 1}
              onClick={() => setPage(currentPage - 1)}
            >
              {t("folders.previousPage")}
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={currentPage >= pages}
              onClick={() => setPage(currentPage + 1)}
            >
              {t("folders.nextPage")}
            </Button>
          </div>
        </div>
        <Button
          variant="ghost"
          size="sm"
          className="self-start"
          disabled={!ids.length}
          onClick={onClear}
        >
          {t("assets.clearSelection")}
        </Button>
      </DialogContent>
    </Dialog>
  );
}
