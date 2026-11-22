import { useEffect, useRef } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { queryKeys } from "@/lib/queryKeys";
import { useAppStore } from "@/stores/app";
export function useProcessingQueueSummary(open: boolean) {
  const libraryId = useAppStore((state) => state.current?.id);
  const client = useQueryClient();
  const summary = useQuery({
    queryKey: queryKeys.processing.summary(libraryId),
    queryFn: () => api.processingSummary(libraryId!),
    enabled: !!libraryId,
    refetchInterval: (q) => ((q.state.data?.activeCount ?? 0) > 0 ? 1500 : open ? 5000 : 15000),
    retry: false,
  });
  const previousRevision = useRef<{
    libraryId: string;
    revision: number;
  } | null>(null);
  useEffect(() => {
    if (!summary.data || !libraryId) return;
    const previous = previousRevision.current;
    if (previous?.libraryId === libraryId && previous.revision !== summary.data.revision) {
      void client.invalidateQueries({ queryKey: queryKeys.processing.lists(libraryId) });
      void client.invalidateQueries({ queryKey: queryKeys.processing.histories(libraryId) });
      void client.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
      void client.invalidateQueries({ queryKey: queryKeys.assetDetail.library(libraryId) });
    }
    previousRevision.current = { libraryId, revision: summary.data.revision };
  }, [libraryId, client, summary.data]);
  return summary;
}
