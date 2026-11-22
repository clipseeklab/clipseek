import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Film, Search, X } from "lucide-react";
import type { Asset } from "@/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
export default function SelectedAssetsDialog({
  assets,
  open,
  onOpenChange,
  onRemove,
}: {
  assets: Asset[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onRemove: (id: number) => void;
}) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const filtered = useMemo(() => {
    const keyword = query.trim().toLocaleLowerCase();
    return keyword
      ? assets.filter((asset) =>
          (asset.displayName ?? asset.fileName).toLocaleLowerCase().includes(keyword),
        )
      : assets;
  }, [assets, query]);
  useEffect(() => {
    if (!open) setQuery("");
  }, [open]);
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[min(720px,80vh)] max-w-2xl flex-col gap-3 overflow-hidden p-0">
        <DialogHeader className="px-5 pt-5">
          <DialogTitle>{t("assets.selectedList", { count: assets.length })}</DialogTitle>
        </DialogHeader>
        <div className="relative px-5">
          <Search className="pointer-events-none absolute left-8 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("assets.searchSelected")}
            className="pl-9"
          />
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto border-t px-3 py-2">
          {filtered.length === 0 ? (
            <div className="flex h-32 items-center justify-center text-sm text-muted-foreground">
              {t("assets.noSelectedResults")}
            </div>
          ) : (
            filtered.map((asset) => {
              const imageSource =
                asset.kind === "video" && !asset.thumbPath
                  ? null
                  : convertFileSrc(asset.thumbPath ?? asset.absPath);
              return (
                <div
                  key={asset.id}
                  className="flex items-center gap-3 rounded-md px-2 py-2 hover:bg-accent/50"
                >
                  <div className="flex h-12 w-16 shrink-0 items-center justify-center overflow-hidden rounded border bg-muted">
                    {imageSource ? (
                      <img src={imageSource} alt="" className="h-full w-full object-cover" />
                    ) : (
                      <Film className="h-5 w-5 text-muted-foreground" />
                    )}
                  </div>
                  <div
                    className="min-w-0 flex-1 truncate text-sm"
                    title={asset.displayName ?? asset.fileName}
                  >
                    {asset.displayName ?? asset.fileName}
                  </div>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-8 w-8 shrink-0 text-muted-foreground hover:text-destructive"
                    title={t("assets.removeSelection")}
                    onClick={() => onRemove(asset.id)}
                  >
                    <X className="h-4 w-4" />
                  </Button>
                </div>
              );
            })
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
