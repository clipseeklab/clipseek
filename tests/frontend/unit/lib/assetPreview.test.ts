import { describe, expect, it } from "vitest";
import { assetPosterPath, streamingPlaybackUrl } from "@/lib/assetPreview";

describe("asset preview boundaries", () => {
  it.each(["video", "image"] as const)("prefers a selected frame over the %s thumbnail", (kind) => {
    expect(
      assetPosterPath({ kind, thumbPath: "/thumb.webp", absPath: "/original" }, "/frame.webp"),
    ).toBe("/frame.webp");
  });

  it("never uses an original video as a poster when both previews are missing", () => {
    expect(
      assetPosterPath({ kind: "video", thumbPath: null, absPath: "/huge.mov" }, null),
    ).toBeNull();
    expect(assetPosterPath({ kind: "image", thumbPath: null, absPath: "/photo.png" })).toBe(
      "/photo.png",
    );
  });

  it("preserves the complete local streaming URL including its access token", () => {
    const url = "http://127.0.0.1:49152/playback/a%20b?token=secret&offset=1234";
    expect(streamingPlaybackUrl(url)).toBe(url);
  });

  it.each([
    "https://127.0.0.1:49152/video",
    "http://localhost:49152/video",
    "http://127.0.0.1.evil.example/video",
    "http://127.0.0.1@evil.example/video",
    "http://192.168.1.1/video",
    "file:///C:/video.mp4",
    "asset://localhost/video.mp4",
    "/relative/video.mp4",
  ])("rejects playback outside the streaming service: %s", (url) => {
    expect(() => streamingPlaybackUrl(url)).toThrow();
  });
});
