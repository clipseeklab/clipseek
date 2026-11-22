import { Folder } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Asset } from "@/types";
export default function AssetCardMetadata({ asset }: { asset: Asset }) {
  const { t } = useTranslation();
  const folder = asset.folderName ?? t("assets.uncategorized");
  const remainingTags = asset.tags.slice(2);
  return (
    <div className="mt-1 flex h-5 min-w-0 items-center gap-2 text-[10px] text-muted-foreground">
      <div className="flex min-w-0 max-w-[min(40%,8rem)] items-center gap-1">
        <Folder className="h-3 w-3 shrink-0" aria-hidden="true" />
        <span className="truncate" title={folder}>
          {folder}
        </span>
      </div>
      {asset.tags.length > 0 && (
        <ul
          aria-label={t("assets.tags")}
          title={asset.tags.join(", ")}
          className="ml-auto flex min-w-0 max-w-[60%] items-center justify-end gap-1 overflow-hidden"
        >
          {asset.tags.slice(0, 2).map((tag) => (
            <li
              key={tag}
              className="min-w-0 max-w-20 truncate rounded bg-background px-1.5 py-0.5"
              title={tag}
            >
              {tag}
            </li>
          ))}
          {remainingTags.length > 0 && (
            <li
              className="shrink-0 rounded bg-background px-1.5 py-0.5"
              title={remainingTags.join(", ")}
              aria-label={remainingTags.join(", ")}
            >
              +{remainingTags.length}
            </li>
          )}
        </ul>
      )}
    </div>
  );
}
