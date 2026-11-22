import { expect, it } from "vitest";
import { formatBitRate } from "@/lib/mediaMetadata";
import { queryKeys } from "@/lib/queryKeys";
import { buildAsset } from "@test/fixtures/assets";

it.each([
  [800, "800 b/s"],
  [128000, "128 kb/s"],
  [1_500_000, "1.5 Mb/s"],
  [12_345_678, "12.35 Mb/s"],
] as const)("formats %s bits per second as %s", (rate, label) => {
  expect(formatBitRate(rate)).toBe(label);
});

it("refreshes media metadata when the source fingerprint changes", () => {
  const asset = buildAsset();
  const base = queryKeys.assetMetadata.asset("a", asset);
  for (const changed of [
    { absPath: "/new/movie.mp4" },
    { fileSize: 3000 },
    { sourceMtime: 12345 },
    { importStage: "error:probe" },
  ])
    expect(queryKeys.assetMetadata.asset("a", { ...asset, ...changed })).not.toEqual(base);
  expect(queryKeys.assetMetadata.asset("b", asset)).not.toEqual(base);
});
