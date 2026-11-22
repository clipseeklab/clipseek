/** Types mirror backend DTOs with serde camelCase fields. */
export interface LibraryMeta {
  id: string;
  name: string;
  path: string;
  createdAt: number;
  /** Library badge palette key. */
  color?: string | null;
}
export interface Settings {
  language: "auto" | "zh" | "en";
  theme: "auto" | "light" | "dark";
  trashRetentionDays: number;

  mcpEnabled?: boolean;
  mcpWriteEnabled?: boolean;
  mcpPort?: number;
  mcpListenMode?: "loopback" | "lan";
  importMode: "reference";
  autoVisualIndex?: boolean;
  autoSubtitleIndex?: boolean;
}
export interface McpStatus {
  enabled: boolean;
  running: boolean;
  writeEnabled: boolean;
  port: number;
  url: string | null;
  localUrl: string | null;
  lanUrl: string | null;
  listenMode: "loopback" | "lan";
  error: string | null;
}
export interface OnboardingState {
  version: number;
  currentStep: 1 | 2 | 3;
  completed: boolean;
}
export type LicenseState = "free" | "active" | "grace" | "expired" | "revoked";
export interface LicenseStatus {
  status: LicenseState;
  issuedAt: number | null;
  leaseExpiresAt: number | null;
  deviceSuffix: string;
  usedBytes: number;
  limitBytes: number;
  remainingBytes: number;
}
export interface LicenseQuota {
  usedBytes: number;
  limitBytes: number;
  remainingBytes: number;
  licensed: boolean;
}
export type VideoTrimMode = "lossless" | "accurate";
export type AssetKind = "image" | "video" | "gif";
export interface Asset {
  id: number;
  uid: string;
  folderId: number | null;
  folderName: string | null;
  fileName: string;
  /** Optional display title, falling back to fileName. */
  displayName: string | null;
  relPath: string;
  absPath: string;
  /** Reference storage for new assets; copy remains for legacy managed records. */
  storage: "copy" | "reference";
  sourcePath: string | null;
  /** Source modification time exposed by the backend (Unix seconds). */
  sourceMtime?: number | null;
  sourceMissing: boolean;
  /** Legacy web source URL and title. */
  webSourceUrl: string | null;
  webSourceTitle: string | null;
  fileSize: number;
  mimeType: string;
  kind: AssetKind;
  width: number | null;
  height: number | null;
  durationMs: number | null;
  color: string | null;
  importedAt: number;
  deletedAt: number | null;
  importStage: string;
  processingTasks?: ProcessingTask[];
  visualIndexed?: boolean;
  subtitleIndexed?: boolean;
  subtitleAvailable?: boolean;
  tags: string[];
  thumbPath: string | null;
}
export interface FrameInfo {
  id: number;
  uid: string;
  seq: number;
  timestampMs: number;
  absPath: string | null;
}
export interface AssetDetail extends Asset {
  frames: FrameInfo[];
}
export interface LibraryStorageStats {
  assetCount: number;
  libraryBytes: number;
  diskAvailableBytes: number;
  diskTotalBytes: number;
}
export interface PlaybackCompatibilityPreflight {
  sourceBytes: number;
  estimatedTemporaryBytes: number;
  availableBytes: number;
  canCreate: boolean;
  modifiesExternalSource: boolean;
}
export interface AssetFilter {
  assetId?: number;
  query?: string;
  folderId?: number;
  folderIds?: number[];
  uncategorized?: boolean;
  tags?: string[];
  kinds?: AssetKind[];
  colors?: string[];
  minWidth?: number;
  minHeight?: number;
  minAspect?: number;
  maxAspect?: number;
  minSize?: number;
  maxSize?: number;
  dateFrom?: number;
  dateTo?: number;
  inTrash?: boolean;
  failedOnly?: boolean;
  visualIndexed?: boolean;
  subtitleIndexed?: boolean;
  sourceMissing?: boolean;
  page?: number;
  pageSize?: number;
  cursorImportedAt?: number;
  cursorId?: number;
  sort?: "imported_desc" | "imported_asc" | "name_asc" | "name_desc" | "size_desc";
}
export interface Paged<T> {
  items: T[];
  total: number;
  page: number;
  pageSize: number;
  hasMore: boolean;
}
export type FolderSort = "name_asc" | "name_desc" | "created_asc" | "created_desc";
export interface Folder {
  id: number;
  name: string;
  createdAt: number;
  assetCount: number;
}
export interface Tag {
  id: number;
  name: string;
  assetCount: number;
  createdAt: number;
}
/** Import progress event payload. */
export interface ImportProgress {
  libraryId: string;
  folderId: number | null;
  total: number;
  done: number;
  currentFile: string;
  stage: "scan" | "copy" | "probe" | "thumb" | "frames" | "embed" | "finish" | "error";
  failed: number;
}
export interface ImportRecoveryJob {
  id: string;
  mode: "paths" | "folders";
  paths: string[];
  folderId: number | null;
  status: "interrupted";
  total: number;
  done: number;
  failed: number;
  duplicates: number;
  createdAt: number;
  tags?: string[];
}
/** Vector search hit with asset details. */
export interface SearchHit {
  score: number;
  frameId: number;
  frameUid: string;
  seq: number;
  timestampMs: number;
  framePath: string | null;
  asset: Asset;
}
export interface AppErrorShape {
  code: string;
  message: string;
}
export interface OperationProgress {
  operationId: string;
  operationType: "merge_library" | "migrate_library" | "move_assets";
  status: "planning" | "running" | "paused" | "completed" | "failed";
  phase: "checking_space" | "moving_directory" | "copying_files" | "writing_vectors" | "completed";
  totalItems: number;
  completedItems: number;
  totalBytes: number;
  completedBytes: number;
  currentItem: string | null;
  failed: number;
  skipped: number;
  message: string | null;
}
export interface ModelStatus {
  execution?: { backend: "cpu" | "metal" | "coreml"; fallbackReason: string | null } | null;
  precision: "q8" | "q4" | "fp32";
  modelDirectory: string;
  ready: boolean;
  version: string;
  downloadedBytes: number;
  totalBytes: number;
}
export interface ModelProgress {
  downloadedBytes: number;
  totalBytes: number;
  currentFile: string;
  speedBps: number;
  etaSeconds: number;
}
export interface VectorStatus {
  libraryId: string;
  state: "ready" | "rebuild_pending" | "rebuilding";
  code: string | null;
  message: string | null;
  rebuildId: string | null;
  failedFrames: number | null;
}
export interface VectorRebuildPreflight {
  libraryId: string;
  frameCount: number;
  currentIndexBytes: number;
  estimatedRequiredBytes: number;
  availableBytes: number;
  canStart: boolean;
  reasonCode: string | null;
}
export interface VectorRebuildProgress {
  libraryId: string;
  rebuildId: string;
  phase: string;
  completed: number;
  total: number;
}
export interface SubtitleCue {
  startMs: number;
  endMs: number;
  text: string;
}
export interface SubtitleInfo {
  assetId: number;
  version: string;
  sourceName: string;
  encoding: string;
  cueCount: number;
  warnings: string[];
}
export interface ImportNotice {
  id: number;
  assetId: number | null;
  fileName: string;
  message: string;
  code?: string | null;
  directory?: string | null;
}
export interface SubtitleHit {
  documentId: number;
  version: string;
  asset: Asset;
  startMs: number;
  endMs: number;
  cues: (SubtitleCue & {
    highlights: {
      start: number;
      end: number;
    }[];
  })[];
}
export interface SubtitleSearchPage {
  items: SubtitleHit[];
  hasMore: boolean;
  nextCursor: string | null;
  scanned: number;
  partial: boolean;
}
export interface IndexScope {
  assetIds?: number[];
  folderIds?: number[];
  tagIds?: number[];
  all?: boolean;
  filter?: AssetFilter;
}
export interface IndexTask {
  id: number;
  assetId: number;
  fileName: string;
  kind: "visual" | "subtitles";
  status: "queued" | "running" | "completed" | "failed" | "cancelled" | "no_source";
  error: string | null;
}
export interface ProcessingTask {
  id: number;
  kind: "import" | "visual" | "subtitles" | "warning";
  status: string;
  phase: string;
  error: string | null;
  attempt: number;
  indexTaskId: number | null;
  createdAt?: number;
  updatedAt?: number;
}
export interface ProcessingItem {
  key: string;
  batchId: string;
  assetId: number | null;
  fileName: string;
  sourcePath: string;
  status: string;
  createdAt: number;
  tasks: ProcessingTask[];
  warningCount?: number;
}
export interface ProcessingSummary {
  counts: Record<string, number>;
  activeCount: number;
  revision: number;
}
export interface ProcessingHistoryPage {
  tasks: ProcessingTask[];
  nextCursor: number | null;
}
export interface ProcessingFilter {
  status?: string;
  kind?: string;
  page?: number;
  pageSize?: number;
}
export interface ProcessingPage {
  items: ProcessingItem[];
  activeCount?: number;
  total: number;
  page: number;
  pageSize: number;
  counts: Record<string, number>;
  revision?: number;
}
