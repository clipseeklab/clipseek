/** Typed wrappers for backend Tauri commands. */
import { invoke as tauriInvoke, type InvokeArgs, type InvokeOptions } from "@tauri-apps/api/core";
import { useAppStore } from "@/stores/app";
function invoke<T>(command: string, args?: InvokeArgs, options?: InvokeOptions): Promise<T> {
  const libraryId = useAppStore.getState().current?.id;
  return tauriInvoke<T>(command, args, {
    ...options,
    headers: { ...(libraryId ? { "x-clipseek-library": libraryId } : {}), ...options?.headers },
  });
}
import type {
  AssetDetail,
  AssetFilter,
  Asset,
  Folder,
  FolderSort,
  LibraryMeta,
  Paged,
  SearchHit,
  Settings,
  LibraryStorageStats,
  Tag,
  ModelStatus,
  OnboardingState,
  ImportRecoveryJob,
  VectorStatus,
  VectorRebuildPreflight,
} from "@/types";
export const api = {
  modelSettings: (precision: "q8" | "q4" | "fp32", modelDirectory: string) =>
    invoke<ModelStatus>("model_settings", { precision, modelDirectory }),
  processingSummary: (libraryId: string) =>
    invoke<import("@/types").ProcessingSummary>("processing_summary", { libraryId }),
  processingHistory: (libraryId: string, key: string, before?: number) =>
    invoke<import("@/types").ProcessingHistoryPage>("processing_history", {
      libraryId,
      key,
      before: before ?? null,
    }),
  processingList: (libraryId: string, filter: import("@/types").ProcessingFilter) =>
    invoke<import("@/types").ProcessingPage>("processing_list", { libraryId, filter }),
  processingHide: (libraryId: string, key: string) =>
    invoke<void>("processing_hide", { libraryId, key }),
  processingClear: (libraryId: string) => invoke<number>("processing_clear", { libraryId }),
  processingRetry: (libraryId: string, id: number) =>
    invoke<void>("processing_retry", { libraryId, id }),
  processingCancel: (libraryId: string, id: number) =>
    invoke<void>("processing_cancel", { libraryId, id }),
  hoverVideoPlay: (request: string) => invoke<void>("hover_video_play", { request }),
  modelStatus: () => invoke<ModelStatus>("model_status"),
  modelDownload: () => invoke<ModelStatus>("model_download"),
  // Library management.
  libraryList: () => invoke<LibraryMeta[]>("library_list"),
  libraryCurrent: () => invoke<LibraryMeta | null>("library_current"),
  libraryVolumeWarning: () =>
    invoke<{
      libraryId: string;
      kind: string;
    } | null>("library_volume_warning"),
  libraryOperations: () => invoke<import("@/types").OperationProgress[]>("library_operations"),
  libraryCreate: (name: string, path: string, color?: string) =>
    invoke<LibraryMeta>("library_create", { name, path, color }),
  libraryImport: (path: string) => invoke<LibraryMeta>("library_import", { path }),
  librarySwitch: (id: string) => invoke<LibraryMeta>("library_switch", { id }),
  libraryReauthorize: (id: string, path: string) =>
    invoke<void>("library_reauthorize", { id, path }),
  libraryRename: (id: string, name: string, color?: string) =>
    invoke<void>("library_rename", { id, name, color }),
  libraryDelete: (id: string, deleteFiles: boolean) =>
    invoke<void>("library_delete", { id, deleteFiles }),
  libraryMigrate: (id: string, newPath: string) => invoke<void>("library_migrate", { id, newPath }),
  libraryMerge: (sourceId: string, targetId: string) =>
    invoke<{
      assets: number;
      failed: number;
      folders: number;
    }>("library_merge", {
      sourceId,
      targetId,
    }),

  assetList: (filter: AssetFilter, libraryId?: string) =>
    invoke<Paged<Asset>>(
      "asset_list",
      { filter },
      {
        headers: libraryId ? { "x-clipseek-library": libraryId } : {},
      },
    ),
  assetStorageStats: () => invoke<LibraryStorageStats>("asset_storage_stats"),
  assetDetail: (id: number) => invoke<AssetDetail>("asset_detail", { id }),
  assetMetadata: (id: number) =>
    invoke<import("@/lib/mediaMetadata").AssetMediaMetadata>("asset_metadata", { id }),
  assetSetDisplayName: (id: number, displayName: string | null) =>
    invoke<void>("asset_set_display_name", { id, displayName }),
  assetSetCover: (id: number, frameId: number) => invoke<Asset>("asset_set_cover", { id, frameId }),
  assetPreview: (id: number) => invoke<number[]>("asset_preview", { id }),
  assetFullPreview: (id: number) => invoke<Uint8Array>("asset_full_preview", { id }),
  framePreview: (id: number) => invoke<number[]>("frame_preview", { id }),
  assetPlaybackSource: (id: number) => invoke<string>("asset_playback_source", { id }),
  assetPlaybackCodec: (id: number) => invoke<string | null>("asset_playback_codec", { id }),
  assetExport: (ids: number[], targetDir: string) =>
    invoke<string[]>("asset_export", { ids, targetDir }),
  assetAddTags: (id: number, names: string[]) => invoke<string[]>("asset_add_tags", { id, names }),
  assetRemoveTag: (id: number, tagName: string) =>
    invoke<string[]>("asset_remove_tag", { id, tagName }),
  assetMoveToFolder: (ids: number[], folderId: number | null) =>
    invoke<void>("asset_move_to_folder", { ids, folderId }),
  assetTrash: (ids: number[]) => invoke<void>("asset_trash", { ids }),
  assetRestore: (ids: number[]) => invoke<void>("asset_restore", { ids }),
  assetDeletePermanent: (ids: number[]) => invoke<void>("asset_delete_permanent", { ids }),
  trashPurgeAll: () => invoke<void>("trash_purge_all"),
  assetMoveToLibrary: (ids: number[], targetLibraryId: string) =>
    invoke<number>("asset_move_to_library", { ids, targetLibraryId }),
  // Folders.
  folderList: () => invoke<Folder[]>("folder_list"),
  folderPage: (query: string, page: number, sort: FolderSort = "created_desc") =>
    invoke<{
      items: Folder[];
      total: number;
      page: number;
      hasMore: boolean;
    }>("folder_page", {
      query,
      page,
      sort,
    }),
  folderGet: (id: number) => invoke<Folder | null>("folder_get", { id }),
  folderSidebar: () =>
    invoke<{
      items: Folder[];
      total: number;
      displayLimit?: number;
    }>("folder_sidebar"),
  folderSidebarSet: (libraryId: string, ids: number[], displayLimit?: number) =>
    invoke<{
      items: Folder[];
      total: number;
      displayLimit?: number;
    }>("folder_sidebar_set", {
      libraryId,
      ids,
      ...(displayLimit === undefined ? {} : { displayLimit }),
    }),
  folderPin: (libraryId: string, id: number, pinned: boolean) =>
    invoke<{
      items: Folder[];
      total: number;
      displayLimit?: number;
    }>("folder_pin", {
      libraryId,
      id,
      pinned,
    }),
  folderCreate: (name: string) => invoke<Folder>("folder_create", { name }),
  folderRename: (id: number, newName: string) => invoke<void>("folder_rename", { id, newName }),
  folderBatch: (ids: number[], targetId: number | null) =>
    invoke<void>("folder_batch", { ids, targetId }),
  folderDelete: (id: number) => invoke<void>("folder_delete", { id }),
  // Tags.
  targetOptions: (kind: "tags" | "folders", query: string, after: number, exclude: number[]) =>
    invoke<{
      items: {
        id: number;
        name: string;
      }[];
      hasMore: boolean;
    }>("target_options", {
      kind,
      query,
      after,
      exclude,
    }),
  tagBatch: (ids: number[], targetId: number | null) =>
    invoke<void>("tag_batch", { ids, targetId }),
  tagCreateMany: (names: string[]) => invoke<number>("tag_create_many", { names }),
  tagList: () => invoke<Tag[]>("tag_list"),
  tagRename: (id: number, newName: string) => invoke<void>("tag_rename", { id, newName }),
  tagDelete: (id: number) => invoke<void>("tag_delete", { id }),
  // Settings.
  mcpStatus: () => invoke<import("@/types").McpStatus>("mcp_status"),
  mcpTokenReveal: () => invoke<string>("mcp_token_reveal"),
  mcpTokenRotate: () => invoke<string>("mcp_token_rotate"),
  settingsGet: () => invoke<Settings>("settings_get"),
  traySetLabels: (show: string, quit: string) => invoke<void>("tray_set_labels", { show, quit }),
  onboardingGet: () => invoke<OnboardingState>("onboarding_get"),
  onboardingSet: (currentStep: OnboardingState["currentStep"], completed: boolean) =>
    invoke<OnboardingState>("onboarding_set", { currentStep, completed }),
  settingsSet: (p: {
    mcpEnabled?: boolean;
    mcpWriteEnabled?: boolean;
    mcpListenMode?: "loopback" | "lan";
    language?: string;
    theme?: string;
    trashRetentionDays?: number;
    importMode?: "reference";
    autoVisualIndex?: boolean;
  }) => invoke<Settings>("settings_set", p),
  diagnosticsOpenLogDir: () => invoke<void>("diagnostics_open_log_dir"),
  diagnosticsExport: (targetPath: string) => invoke<string>("diagnostics_export", { targetPath }),
  frontendError: (kind: string, message: string) =>
    invoke<void>("frontend_error", { kind, message }),
  importPaths: (
    paths: string[],
    folderId: number | null,
    batchId?: string,
    tags?: string[],
    firstImport?: boolean,
  ) =>
    invoke<{
      queued: number;
    }>("import_paths", {
      paths,
      folderId,
      batchId,
      ...(tags?.length ? { tags } : {}),
      ...(firstImport ? { firstImport: true } : {}),
    }),
  importFolders: (paths: string[], batchId?: string) =>
    invoke<{
      queued: number;
    }>("import_folders", { paths, batchId }),
  importCancel: () => invoke<void>("import_cancel"),
  sourceCheck: (libraryId: string) =>
    invoke<{
      total: number;
      missing: number;
      cancelled: boolean;
    }>("source_check", { libraryId }),
  sourceCheckCancel: () => invoke<void>("source_check_cancel"),
  importRunning: () => invoke<boolean>("import_running"),
  importRetry: (ids?: number[]) =>
    invoke<{
      retried: number;
      failed: number;
    }>("import_retry", { ids }),
  importRecoveryList: () => invoke<ImportRecoveryJob[]>("import_recovery_list"),
  importRecoveryResume: (jobId: string) =>
    invoke<{
      queued: number;
      alreadyResolved: boolean;
    }>("import_recovery_resume", { jobId }),
  importRecoveryAbandon: (jobId: string) => invoke<void>("import_recovery_abandon", { jobId }),
  // Semantic search.
  searchImagePreview: (imagePath: string) =>
    invoke<number[]>("search_image_preview", { imagePath }),
  searchText: (query: string, filter: AssetFilter, page?: number, pageSize?: number) =>
    invoke<Paged<SearchHit>>("search_text", { query, filter, page, pageSize }),
  searchImage: (imagePath: string, filter: AssetFilter, page?: number, pageSize?: number) =>
    invoke<Paged<SearchHit>>("search_image", { imagePath, filter, page, pageSize }),
  indexSummary: () =>
    invoke<{
      assets: number;
      visual: number;
      subtitles: number;
      tasks: Record<string, number>;
      visualModelReady: boolean;
    }>("index_summary"),
  indexBuild: (scope: import("@/types").IndexScope, kinds: string[]) =>
    invoke<number>("index_build", { scope, kinds }),
  indexTasks: (assetId?: number) =>
    invoke<import("@/types").IndexTask[]>("index_tasks", { assetId }),
  indexTaskCancel: (id?: number) => invoke<void>("index_task_cancel", { id }),
  indexTaskRetry: (id: number) => invoke<void>("index_task_retry", { id }),
  vectorStatus: () => invoke<VectorStatus>("vector_status"),
  vectorRetryOpen: () => invoke<VectorStatus>("vector_retry_open"),
  vectorRebuildPreflight: () => invoke<VectorRebuildPreflight>("vector_rebuild_preflight"),
  vectorRebuildStart: (confirmed: boolean) => invoke<string>("vector_rebuild_start", { confirmed }),
  vectorRebuildCancel: () => invoke<void>("vector_rebuild_cancel"),
};
