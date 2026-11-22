// Stable complete asset fixture; tests override only fields relevant to the scenario.
import type { Asset } from "@/types";

export function buildAsset(overrides: Partial<Asset> = {}): Asset {
  const id = overrides.id ?? 1;
  return {
    id,
    uid: `asset-${id}`,
    folderId: null,
    folderName: null,
    fileName: `photo-${id}.jpg`,
    displayName: null,
    relPath: `images/photo-${id}.jpg`,
    absPath: `/library/images/photo-${id}.jpg`,
    storage: "copy",
    sourcePath: null,
    sourceMissing: false,
    webSourceUrl: null,
    webSourceTitle: null,
    fileSize: 1_024,
    mimeType: "image/jpeg",
    kind: "image",
    width: 640,
    height: 480,
    durationMs: null,
    color: null,
    importedAt: id,
    deletedAt: null,
    importStage: "done",
    tags: [],
    thumbPath: null,
    ...overrides,
  };
}
