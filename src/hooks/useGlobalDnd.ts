/** Listen to Tauri drag/drop events and import into the current folder target, defaulting to
 * uncategorized. useImportEvents handles backend events and cache synchronization.
 */
import { useEffect } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { toast } from "@/components/Toaster";
import { useImportStore } from "@/stores/import";
import { useAppStore } from "@/stores/app";
import { useImportEvents } from "@/hooks/useImportEvents";
import { queryKeys } from "@/lib/queryKeys";
import { explainError } from "@/lib/errors";
export function useGlobalDnd() {
  const queryClient = useQueryClient();
  useImportEvents();
  const setDragging = useImportStore((s) => s.setDragging);
  const tasks = useImportStore((s) => s.tasks);
  const queuePaused = useImportStore((s) => s.queuePaused);
  // Dispatch pending registrations alongside existing batches; backend processing and
  // embedding remain serialized so new cards appear promptly.
  useEffect(() => {
    if (queuePaused) return;
    for (const task of tasks) {
      if (task.status !== "pending") continue;
      if (
        useImportStore.getState().tasks.find((entry) => entry.id === task.id)?.status !== "pending"
      )
        continue;
      useImportStore.getState().markDispatched(task.id);
      const request =
        task.mode === "folders"
          ? api.importFolders(task.paths, task.id)
          : task.tags?.length
            ? api.importPaths(task.paths, task.folderId, task.id, task.tags)
            : api.importPaths(task.paths, task.folderId, task.id);
      const libraryId = useAppStore.getState().current?.id;
      void request.catch((error: unknown) => {
        if (
          libraryId !== useAppStore.getState().current?.id ||
          !useImportStore.getState().tasks.some((entry) => entry.id === task.id)
        )
          return;
        useImportStore.getState().failTask(task.id, explainError(error));
        toast.error(explainError(error));
      });
    }
  }, [tasks, queuePaused]);
  useEffect(() => {
    let disposed = false;
    let fileDrag = false;
    let stopDrop: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (disposed) return;
        const type = event.payload.type;
        if (type === "enter") {
          fileDrag = event.payload.paths.length > 0;
          setDragging(fileDrag);
        } else if (type === "over") {
          // Native over events carry no paths; only continue a confirmed file drag.
          setDragging(fileDrag);
        } else if (type === "leave") {
          fileDrag = false;
          setDragging(false);
        } else if (type === "drop") {
          fileDrag = false;
          setDragging(false);
          const paths = event.payload.paths;
          if (paths.length === 0) return;
          // Delegate dropped query images to the image-search page.
          if (useImportStore.getState().dropMode === "image-search") {
            const img = paths.find((p) => /\.(jpe?g|png|webp|bmp|gif|svg|heic|heif)$/i.test(p));
            if (img) useImportStore.getState().setSearchDropPath(img);
            return;
          }
          const state = useImportStore.getState();
          state.enqueue(
            paths,
            state.dropFolderId ?? null,
            state.dropMode === "folder-import" ? "folders" : "paths",
            state.dropTags,
          );
          void queryClient
            .invalidateQueries({
              queryKey: queryKeys.folders.library(useAppStore.getState().current?.id),
            })
            .catch((error: unknown) => toast.error(explainError(error)));
        }
      })
      .then((stop) => {
        if (disposed) stop();
        else stopDrop = stop;
      })
      .catch((error: unknown) => {
        if (!disposed) toast.error(explainError(error));
      });
    return () => {
      disposed = true;
      stopDrop?.();
    };
  }, [setDragging, queryClient]);
}
