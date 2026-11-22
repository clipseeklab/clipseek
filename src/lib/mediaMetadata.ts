export interface MetadataTag {
  name: string;
  value: string;
}
export interface MediaStreamMetadata {
  index: number;
  kind: "video" | "audio" | "subtitle" | "data" | "attachment";
  codec: string;
  profile: string | null;
  language: string | null;
  width: number | null;
  height: number | null;
  frameRate: number | null;
  bitRate: number | null;
  pixelFormat: string | null;
  sampleRate: number | null;
  channelLayout: string | null;
  sampleFormat: string | null;
  description: string;
  tags: MetadataTag[];
}
export interface AssetMediaMetadata {
  format: string | null;
  bitRate: number | null;
  createdAt: number | null;
  modifiedAt: number | null;
  tags: MetadataTag[];
  exif: MetadataTag[];
  streams: MediaStreamMetadata[];
}
export function formatBitRate(value: number): string {
  const unit = value >= 1000000 ? "Mb/s" : value >= 1000 ? "kb/s" : "b/s";
  const scale = value >= 1000000 ? 1000000 : value >= 1000 ? 1000 : 1;
  return `${Number((value / scale).toFixed(2))} ${unit}`;
}
