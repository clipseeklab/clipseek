import { describe, expect, it, vi } from "vitest";
import type { Asset, AssetFilter, Paged } from "@/types";
import { buildAsset } from "@test/fixtures/assets";

const assetList = vi.hoisted(() => vi.fn());

vi.mock("@/lib/api", () => ({
  api: { assetList },
}));

import { PAGE_SIZE, assetPagesOptions } from "@/hooks/useAssetPages";

function page(overrides: Partial<Paged<Asset>> = {}): Paged<Asset> {
  return {
    items: [buildAsset({ id: 8, importedAt: 900 })],
    total: 2,
    page: 1,
    pageSize: PAGE_SIZE,
    hasMore: true,
    ...overrides,
  };
}

function nextPage(filter: AssetFilter, last = page()) {
  const options = assetPagesOptions(filter, "library-1");
  return options.getNextPageParam?.(last, [last], { page: 1 }, [{ page: 1 }]);
}

describe("assetPagesOptions", () => {
  it("keys the cache by library and normalized tags", () => {
    expect(assetPagesOptions({ kinds: ["image"] }, "library-1").queryKey).toEqual([
      "assets",
      "library-1",
      { kinds: ["image"], tags: [] },
    ]);
  });

  it("uses a stable cursor for the default newest-first order", () => {
    expect(nextPage({ kinds: ["image"] })).toEqual({
      page: 2,
      cursorImportedAt: 900,
      cursorId: 8,
    });
  });

  it("uses page-only pagination for other sort orders", () => {
    expect(nextPage({ sort: "name_asc" })).toEqual({ page: 2 });
  });

  it("stops at an empty or final page", () => {
    expect(nextPage({}, page({ hasMore: false }))).toBeUndefined();
    expect(nextPage({}, page({ items: [] }))).toBeUndefined();
  });

  it("passes page size and cursor fields to the backend", async () => {
    assetList.mockResolvedValue(page());
    const options = assetPagesOptions({ folderId: 4 }, "library-1");

    await options.queryFn?.({
      queryKey: options.queryKey,
      pageParam: { page: 2, cursorImportedAt: 900, cursorId: 8 },
      direction: "forward",
      signal: new AbortController().signal,
      meta: undefined,
      client: undefined,
    } as never);

    expect(assetList).toHaveBeenCalledWith(
      {
        folderId: 4,
        page: 2,
        cursorImportedAt: 900,
        cursorId: 8,
        pageSize: PAGE_SIZE,
      },
      "library-1",
    );
  });
});
