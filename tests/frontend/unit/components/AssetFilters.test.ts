import { describe, expect, it } from "vitest";
import { EMPTY_FILTER, activeFilterCount, toAssetFilter } from "@/components/assets/AssetFilters";

describe("AssetFilters asset ID", () => {
  it("converts a positive integer into an exact asset filter", () => {
    const value = { ...EMPTY_FILTER, assetId: "42" };
    expect(toAssetFilter(value)).toMatchObject({ assetId: 42 });
    expect(activeFilterCount(value)).toBe(1);
  });

  it("ignores empty or invalid asset IDs", () => {
    expect(toAssetFilter({ ...EMPTY_FILTER, assetId: "" }).assetId).toBeUndefined();
    expect(toAssetFilter({ ...EMPTY_FILTER, assetId: "0" }).assetId).toBeUndefined();
  });
});
