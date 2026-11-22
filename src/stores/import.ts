/** Import state combines progress events, the in-memory queue, and drop-target context. */
import { create } from "zustand";
import type { Asset, ImportProgress, ImportRecoveryJob } from "@/types";
import { removeRegisteredPaths } from "@/lib/importPerformance";
export interface ImportSummary {
  total: number;
  done: number;
  failed: number;
  duplicates: number;
}
export type ImportTaskStatus = "pending" | "processing" | "success" | "failed";
export interface ImportQueueTask {
  id: string;
  path: string;
  paths: string[];
  pendingPaths: string[];
  folderId: number | null;
  tags?: string[];
  mode: "paths" | "folders";
  status: ImportTaskStatus;
  createdAt: number;
  error?: string;
}
interface ImportStore {
  progress: ImportProgress | null;
  summary: ImportSummary | null;
  tasks: ImportQueueTask[];
  queuePaused: boolean;
  enqueue: (
    paths: string[],
    folderId: number | null,
    mode?: ImportQueueTask["mode"],
    tags?: string[],
  ) => void;
  restoreTask: (job: ImportRecoveryJob) => void;
  markDispatched: (id: string) => void;
  markRegisteredBatch: (paths: string[]) => void;
  finishTask: (batchId: string, summary: ImportSummary) => void;
  failTask: (batchId: string, error: string) => void;
  deleteTask: (id: string) => void;
  setQueuePaused: (paused: boolean) => void;
  clearQueue: () => void;
  dragging: boolean;
  dropFolderId: number | null | undefined;
  dropTags: string[];
  setDropTags: (tags: string[]) => void;
  dropMode: "import" | "folder-import" | "image-search";
  searchDropPath: string | null;
  setProgress: (p: ImportProgress | null) => void;
  setSummary: (s: ImportSummary | null) => void;
  setDragging: (v: boolean) => void;
  setDropFolderId: (v: number | null | undefined) => void;
  setDropMode: (v: "import" | "folder-import" | "image-search") => void;
  setSearchDropPath: (v: string | null) => void;
  liveAssets: Map<number, Asset>;
  applyAsset: (a: Asset) => void;
  applyAssets: (assets: Asset[]) => void;
  clearLiveAssets: () => void;
  reconcileLiveAssets: (ids: number[]) => void;
}
let taskSequence = 0;
export const MAX_LIVE_ASSETS = 500;
export const useImportStore = create<ImportStore>((set, get) => ({
  progress: null,
  summary: null,
  tasks: [],
  queuePaused: false,
  enqueue: (paths, folderId, mode = "paths", tags = []) => {
    const now = Date.now();
    const tasks: ImportQueueTask[] =
      paths.length === 0
        ? []
        : [
            {
              id: `${now}-${taskSequence++}`,
              path: paths[0],
              paths,
              pendingPaths:
                mode === "paths"
                  ? paths.filter((path) =>
                      /\.(mp4|m4v|mov|mkv|webm|avi|flv|wmv|mpeg|mpg|ts|mts|m2ts|jpg|jpeg|png|webp|gif|bmp|svg|heic|heif|tif|tiff|avif)$/i.test(
                        path,
                      ),
                    )
                  : [],
              folderId,
              ...(tags.length ? { tags: [...tags] } : {}),
              mode,
              status: "pending",
              createdAt: now,
            },
          ];
    set((state) => ({ tasks: [...tasks, ...state.tasks], summary: null }));
  },
  restoreTask: (job) =>
    set((state) => {
      if (state.tasks.some((task) => task.id === job.id)) return state;
      const task: ImportQueueTask = {
        id: job.id,
        path: job.paths[0] ?? "",
        paths: job.paths,
        pendingPaths: [],
        folderId: job.folderId,
        ...(job.tags?.length ? { tags: [...job.tags] } : {}),
        mode: job.mode,
        status: "processing",
        createdAt: job.createdAt,
      };
      return { tasks: [task, ...state.tasks], summary: null };
    }),
  markDispatched: (id) =>
    set((state) => ({
      tasks: state.tasks.map((task) =>
        task.id === id ? { ...task, status: "processing", error: undefined } : task,
      ),
    })),
  markRegisteredBatch: (paths) =>
    set((state) => ({
      tasks: state.tasks.map((task) => {
        const pendingPaths = removeRegisteredPaths(task.pendingPaths, paths);
        return pendingPaths === task.pendingPaths ? task : { ...task, pendingPaths };
      }),
    })),
  finishTask: (batchId, summary) =>
    set((state) => ({
      summary,
      tasks: state.tasks.map((task) =>
        task.id === batchId ? { ...task, status: summary.failed > 0 ? "failed" : "success" } : task,
      ),
    })),
  failTask: (batchId, error) =>
    set((state) => ({
      tasks: state.tasks.map((task) =>
        task.id === batchId ? { ...task, status: "failed", error } : task,
      ),
    })),
  deleteTask: (id) => set((state) => ({ tasks: state.tasks.filter((task) => task.id !== id) })),
  setQueuePaused: (queuePaused) => set({ queuePaused }),
  clearQueue: () => set({ tasks: [], progress: null, summary: null, queuePaused: false }),
  dragging: false,
  dropFolderId: undefined,
  dropTags: [],
  setDropTags: (dropTags) => set({ dropTags }),
  dropMode: "import",
  searchDropPath: null,
  setProgress: (progress) => set({ progress }),
  setSummary: (summary) => set({ summary }),
  setDragging: (dragging) => set({ dragging }),
  setDropFolderId: (dropFolderId) => set({ dropFolderId }),
  setDropMode: (dropMode) => set({ dropMode }),
  setSearchDropPath: (searchDropPath) => set({ searchDropPath }),
  liveAssets: new Map(),
  applyAsset: (asset) => get().applyAssets([asset]),
  applyAssets: (assets) =>
    set((state) => {
      let next: Map<number, Asset> | undefined;
      for (const asset of assets) {
        const previous = (next ?? state.liveAssets).get(asset.id);
        if (previous && JSON.stringify(previous) === JSON.stringify(asset)) continue;
        next ??= new Map(state.liveAssets);
        next.set(asset.id, asset);
        while (next.size > MAX_LIVE_ASSETS) {
          const oldest = next.keys().next().value;
          if (oldest == null) break;
          next.delete(oldest);
        }
      }
      return next ? { liveAssets: next } : state;
    }),
  clearLiveAssets: () => set({ liveAssets: new Map() }),
  reconcileLiveAssets: (ids) =>
    set((state) => {
      if (!ids.some((id) => state.liveAssets.has(id))) return state;
      const next = new Map(state.liveAssets);
      for (const id of ids) next.delete(id);
      return { liveAssets: next };
    }),
}));
