/** Import event synchronization owns listeners, buffered progress and cache refreshes. */
import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { CancelledError, useQueryClient, type QueryKey } from "@tanstack/react-query";
import { toast } from "@/components/Toaster";
import { explainError } from "@/lib/errors";
import { FrameCoalescer } from "@/lib/frameCoalescer";
import { queryKeys } from "@/lib/queryKeys";
import { useAppStore } from "@/stores/app";
import { MAX_LIVE_ASSETS, useImportStore } from "@/stores/import";
import type { Asset, ImportProgress } from "@/types";
interface ImportDone {
  libraryId?: string;
  batchId?: string;
  total: number;
  done: number;
  failed: number;
  duplicates: number;
}
export function useImportEvents() {
  const client = useQueryClient();
  useEffect(() => {
    let disposed = false;
    let generation = 0;
    let assetRevision = 0;
    let refreshTimer: ReturnType<typeof setTimeout> | undefined;
    let assetTimer: ReturnType<typeof setTimeout> | undefined;
    const pendingAssets = new Map<number, Asset>();
    const flushAssets = () => {
      clearTimeout(assetTimer);
      assetTimer = undefined;
      if (!pendingAssets.size) return;
      const batch = [...pendingAssets.values()];
      pendingAssets.clear();
      useImportStore.getState().applyAssets(batch);
    };
    const discardAssets = () => {
      clearTimeout(assetTimer);
      assetTimer = undefined;
      pendingAssets.clear();
    };
    const stops: (() => void)[] = [];
    const currentLibrary = () => useAppStore.getState().current?.id;
    const isCurrent = (libraryId: string | undefined, version: number) =>
      !disposed && version === generation && libraryId === currentLibrary();
    const accepts = (libraryId: string | undefined) =>
      !disposed && !!currentLibrary() && (!libraryId || libraryId === currentLibrary());
    const refresh = (queryKey: QueryKey) => {
      const libraryId = currentLibrary();
      const version = generation;
      return client
        .invalidateQueries({ queryKey }, { throwOnError: true })
        .then(() => true)
        .catch((error: unknown) => {
          if (!(error instanceof CancelledError) && isCurrent(libraryId, version))
            toast.error(explainError(error));
          return false;
        });
    };
    const progress = new FrameCoalescer<ImportProgress>(
      (callback) => requestAnimationFrame(callback),
      (handle) => cancelAnimationFrame(handle),
      (value) => {
        if (!disposed && (!value || value.libraryId === currentLibrary())) {
          useImportStore.getState().setProgress(value);
        }
      },
    );
    const resetLibrary = () => {
      generation += 1;
      discardAssets();
      clearTimeout(refreshTimer);
      refreshTimer = undefined;
      progress.clear();
      const store = useImportStore.getState();
      store.clearLiveAssets();
      store.clearQueue();
      store.setSummary(null);
    };
    // Drain patches with cache publication so snapshot reconciliation can retire
    // them, rather than letting buffered older state reappear afterward.
    const stopSnapshots = client.getQueryCache().subscribe((event) => {
      if (event.type !== "updated" || event.action.type !== "success") return;
      const key = queryKeys.assets.library(currentLibrary());
      if (key.every((part, index) => event.query.queryKey[index] === part)) flushAssets();
    });
    const unsubscribe = useAppStore.subscribe((state, previous) => {
      if (state.current?.id !== previous.current?.id) resetLibrary();
    });
    const bind = <T>(name: string, consume: (payload: T) => void) => {
      void listen<T>(name, ({ payload }) => {
        if (!disposed) consume(payload);
      })
        .then((stop) => {
          if (disposed) stop();
          else stops.push(stop);
        })
        .catch((error: unknown) => {
          if (!disposed) toast.error(explainError(error));
        });
    };
    bind<ImportProgress>("import://progress", (payload) => {
      if (!accepts(payload.libraryId)) return;
      if (payload.stage === "finish" || payload.stage === "error") {
        progress.clear();
        void refresh(queryKeys.importNotices.library(payload.libraryId ?? currentLibrary()));
      } else progress.push(payload);
    });
    bind<{
      libraryId: string;
      sourcePaths: string[];
    }>("import://registered-batch", (payload) => {
      if (!accepts(payload.libraryId)) return;
      useImportStore.getState().markRegisteredBatch(payload.sourcePaths);
      void refresh(queryKeys.importNotices.library(payload.libraryId ?? currentLibrary()));
    });
    bind<string>("import://maintenance-done", (libraryId) => {
      if (!accepts(libraryId)) return;
      flushAssets();
      progress.clear();
      void refresh(queryKeys.assets.library(libraryId));
    });
    bind<string>("folders://changed", (libraryId) => {
      if (accepts(libraryId)) void refresh(queryKeys.folders.library(libraryId));
    });
    bind<
      Asset & {
        libraryId?: string;
      }
    >("import://asset", (payload) => {
      if (!accepts(payload.libraryId)) return;
      pendingAssets.set(payload.id, payload);
      while (pendingAssets.size > MAX_LIVE_ASSETS) {
        const oldest = pendingAssets.keys().next().value;
        if (oldest == null) break;
        pendingAssets.delete(oldest);
      }
      assetRevision += 1;
      if (assetTimer === undefined) {
        const libraryId = currentLibrary();
        const version = generation;
        // One bounded publication per frame-sized task, also while the window is hidden.
        assetTimer = setTimeout(() => {
          assetTimer = undefined;
          if (isCurrent(libraryId, version)) flushAssets();
        }, 16);
      }
      if (refreshTimer !== undefined) return;
      const libraryId = currentLibrary();
      const version = generation;
      refreshTimer = setTimeout(() => {
        refreshTimer = undefined;
        if (isCurrent(libraryId, version)) void refresh(queryKeys.assets.library(libraryId));
      }, 300);
    });
    bind<ImportDone>("import://done", (payload) => {
      if (!accepts(payload.libraryId)) return;
      const libraryId = currentLibrary();
      const version = generation;
      const revision = assetRevision;
      flushAssets();
      progress.clear();
      const store = useImportStore.getState();
      store.setSummary(payload);
      if (payload.batchId) store.finishTask(payload.batchId, payload);
      for (const key of [
        queryKeys.tags.library(libraryId),
        queryKeys.assetDetail.library(libraryId),
        queryKeys.importNotices.library(libraryId),

        queryKeys.folders.library(libraryId),
        queryKeys.storageStats.library(libraryId),
      ])
        void refresh(key);
      void refresh(queryKeys.assets.library(libraryId)).then((refreshed) => {
        if (!refreshed || !isCurrent(libraryId, version) || revision !== assetRevision) return;
        const state = useImportStore.getState();
        if (
          !state.tasks.some((task) => task.status === "pending" || task.status === "processing")
        ) {
          state.clearLiveAssets();
        }
      });
    });
    bind<unknown>("library://switched", resetLibrary);
    return () => {
      disposed = true;
      generation += 1;
      discardAssets();
      clearTimeout(refreshTimer);
      progress.dispose();
      unsubscribe();
      stopSnapshots();
      for (const stop of stops) stop();
    };
  }, [client]);
}
