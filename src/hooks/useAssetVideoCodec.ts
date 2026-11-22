import { queryKeys } from "@/lib/queryKeys";
import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import type { Asset } from "@/types";
// Visible cards may mount together. Probe sequentially rather than starting a
// metadata probe for every card at once; cache results across hover/rerenders.
let pending: Promise<void> = Promise.resolve();
export function useAssetVideoCodec(asset: Asset) {
  const libraryId = useAppStore((state) => state.current?.id);
  return useQuery({
    queryKey: queryKeys.videoCodec.asset(libraryId, asset),
    enabled: asset.kind === "video" && asset.importStage === "done" && !asset.sourceMissing,
    staleTime: Infinity,
    retry: false,
    queryFn: ({ signal }) => {
      const result = pending.then(() => {
        if (signal.aborted || useAppStore.getState().current?.id !== libraryId) {
          throw new DOMException("Preview no longer active", "AbortError");
        }
        return api.assetPlaybackCodec(asset.id);
      });
      pending = result.then(
        () => undefined,
        () => undefined,
      );
      return result;
    },
  });
}
