import type { Asset, AssetFilter, FolderSort, ProcessingFilter } from "@/types";
/** Shared cache identities; asset prefixes isolate invalidation by library. */
export const queryKeys = {
  embeddingModel: ["embedding-model"] as const,
  assets: {
    all: ["assets"] as const,
    library: (libraryId: string | undefined) => ["assets", libraryId] as const,
    pages: (libraryId: string | undefined, filter: AssetFilter) =>
      ["assets", libraryId, { ...filter, tags: filter.tags ?? [] }] as const,
  },
  folders: {
    all: ["folders"] as const,
    library: (libraryId: string | undefined) => ["folders", libraryId] as const,
    sidebar: (libraryId: string | undefined) => ["folders", libraryId, "sidebar"] as const,
    detail: (libraryId: string | undefined, id: number) =>
      ["folders", libraryId, "detail", id] as const,
    options: (libraryId: string | undefined, query: string) =>
      ["folders", libraryId, "options", query] as const,
    page: (libraryId: string | undefined, query: string, page: number, sort: FolderSort) =>
      ["folders", libraryId, "page", query, page, sort] as const,
  },
  tags: {
    all: ["tags"] as const,
    library: (libraryId: string | undefined) => ["tags", libraryId] as const,
  },
  assetDetail: {
    all: ["asset-detail"] as const,
    library: (libraryId: string | undefined) => ["asset-detail", libraryId] as const,
    item: (libraryId: string | undefined, id: number | null | undefined) =>
      ["asset-detail", libraryId, id] as const,
  },
  assetMetadata: {
    library: (libraryId: string | undefined) => ["asset-metadata", libraryId] as const,
    asset: (
      libraryId: string | undefined,
      asset: Pick<Asset, "id" | "absPath" | "fileSize" | "sourceMtime" | "importStage">,
    ) =>
      [
        "asset-metadata",
        libraryId,
        asset.id,
        asset.absPath,
        asset.fileSize,
        asset.sourceMtime,
        asset.importStage,
      ] as const,
  },
  processing: {
    library: (libraryId: string | undefined) => ["processing", libraryId] as const,
    summary: (libraryId: string | undefined) => ["processing", libraryId, "summary"] as const,
    lists: (libraryId: string | undefined) => ["processing", libraryId, "list"] as const,
    list: (libraryId: string | undefined, filter: ProcessingFilter) =>
      ["processing", libraryId, "list", filter] as const,
    histories: (libraryId: string | undefined) => ["processing", libraryId, "history"] as const,
    history: (libraryId: string | undefined, key: string) =>
      ["processing", libraryId, "history", key] as const,
  },
  indexTasks: {
    library: (libraryId: string | undefined) => ["indexTasks", libraryId] as const,
    asset: (libraryId: string | undefined, id: number) => ["indexTasks", libraryId, id] as const,
  },
  indexSummary: {
    library: (libraryId: string | undefined) => ["indexSummary", libraryId] as const,
  },
  targetOptions: {
    library: (libraryId: string | undefined) => ["target-options", libraryId] as const,
    search: (
      libraryId: string | undefined,
      kind: "tags" | "folders",
      query: string,
      exclude: number[],
    ) => ["target-options", libraryId, kind, query, exclude] as const,
  },
  videoCodec: {
    library: (libraryId: string | undefined) => ["asset-video-codec", libraryId] as const,
    asset: (
      libraryId: string | undefined,
      asset: Pick<Asset, "id" | "absPath" | "fileSize" | "sourceMtime" | "importStage">,
    ) =>
      [
        "asset-video-codec",
        libraryId,
        asset.id,
        asset.absPath,
        asset.fileSize,
        asset.sourceMtime,
        asset.importStage,
      ] as const,
  },
  importNotices: {
    library: (libraryId: string | undefined) => ["import-notices", libraryId] as const,
  },
  storageStats: {
    all: ["asset-storage-stats"] as const,
    library: (libraryId: string | undefined) => ["asset-storage-stats", libraryId] as const,
  },
};
