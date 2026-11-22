import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { Plus } from "lucide-react";
import { queryKeys } from "@/lib/queryKeys";
import { api } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import { toast } from "@/components/Toaster";
import { Button } from "@/components/ui/button";
import TagInput from "@/components/metadata/TagInput";
import { collectTagNames, type TagInputValue } from "@/lib/tagInput";
import type { Asset } from "@/types";
import { explainError } from "@/lib/errors";
export default function AssetTagEditor(props: {
  assets: Asset[];
  autoFocus?: boolean;
  onAdded?: () => void;
}) {
  const libraryId = useAppStore((state) => state.current?.id);
  return <AssetTagDraft key={libraryId} {...props} />;
}
function AssetTagDraft({
  assets,
  autoFocus = false,
  onAdded,
}: {
  assets: Asset[];
  autoFocus?: boolean;
  onAdded?: () => void;
}) {
  const { t } = useTranslation();
  const client = useQueryClient();
  const libraryId = useAppStore((state) => state.current?.id);
  const [value, setValue] = useState<TagInputValue>({ names: [], input: "" });
  const [busy, setBusy] = useState(false);
  const names = collectTagNames(value);
  const add = async () => {
    if (!names.length || !assets.length || busy) return;
    setBusy(true);
    try {
      for (const asset of assets) {
        if (useAppStore.getState().current?.id !== libraryId) return;
        await api.assetAddTags(asset.id, names);
      }
      void client.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
      void client.invalidateQueries({ queryKey: queryKeys.tags.library(libraryId) });
      for (const asset of assets)
        void client.invalidateQueries({
          queryKey: queryKeys.assetDetail.item(libraryId, asset.id),
        });
      if (useAppStore.getState().current?.id !== libraryId) return;
      useAppStore.getState().markRecentTags(names);
      setValue({ names: [], input: "" });
      onAdded?.();
    } catch (error: unknown) {
      if (useAppStore.getState().current?.id === libraryId) toast.error(explainError(error));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex items-start gap-2">
      <div className="min-w-0 flex-1">
        <TagInput value={value} onChange={setValue} autoFocus={autoFocus} disabled={busy} />
      </div>
      <Button
        size="icon"
        aria-label={t("assets.addTags")}
        disabled={busy || !names.length || !assets.length}
        onClick={() => void add()}
      >
        <Plus className="size-4" />
      </Button>
    </div>
  );
}
