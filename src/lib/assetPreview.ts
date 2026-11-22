import type { Asset } from "@/types";
/** Poster images must never fall back to the original video file. */
export function assetPosterPath(
  asset: Pick<Asset, "kind" | "thumbPath" | "absPath">,
  framePath?: string | null,
): string | null {
  return framePath ?? asset.thumbPath ?? (asset.kind === "video" ? null : asset.absPath);
}
/** Video sources are issued by the local streaming service on every platform. */
export function streamingPlaybackUrl(path: string): string {
  const url = new URL(path);
  if (url.protocol !== "http:" || url.hostname !== "127.0.0.1") {
    throw new Error("Playback service did not return a local streaming URL");
  }
  return path;
}
