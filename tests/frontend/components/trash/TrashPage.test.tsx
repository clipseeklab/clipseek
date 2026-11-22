import type { ReactNode } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen, waitFor } from "@test/helpers/render";
import { buildAsset } from "@test/fixtures/assets";
import type { Asset } from "@/types";
import { useAppStore } from "@/stores/app";
import { queryKeys } from "@/lib/queryKeys";
import TrashPage from "@/pages/TrashPage";

vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (path: string) => path }));

const mocks = vi.hoisted(() => ({ remove: vi.fn(), purgeAll: vi.fn(), error: vi.fn() }));
vi.mock("@/lib/api", () => ({
  api: { assetDeletePermanent: mocks.remove, trashPurgeAll: mocks.purgeAll },
}));
vi.mock("@/components/Toaster", () => ({ toast: { error: mocks.error, success: vi.fn() } }));
vi.mock("@/hooks/useAssetPages", () => ({
  useAssetPages: () => ({
    items: [buildAsset({ deletedAt: 1 })],
    total: 1,
    isLoading: false,
    hasNextPage: false,
    isFetchingNextPage: false,
    fetchNextPage: vi.fn(),
  }),
}));
vi.mock("@/components/assets/MasonryGrid", () => ({
  default: ({ items, renderItem }: { items: Asset[]; renderItem: (asset: Asset) => ReactNode }) => (
    <div>
      {items.map((asset) => (
        <div key={asset.id}>{renderItem(asset)}</div>
      ))}
    </div>
  ),
}));

beforeEach(() => {
  mocks.remove
    .mockReset()
    .mockRejectedValueOnce({ code: "asset_cleanup_failed", message: "locked file; retry deletion" })
    .mockResolvedValue(undefined);
  mocks.purgeAll
    .mockReset()
    .mockRejectedValueOnce({ code: "asset_cleanup_failed", message: "locked file; retry deletion" })
    .mockResolvedValue(undefined);
  mocks.error.mockClear();
  useAppStore.setState({ current: { id: "a", name: "a", path: "/a", createdAt: 1 } });
});
afterEach(() => useAppStore.setState({ current: null }));

it.each(["single", "all"])(
  "refreshes actual library state after failed %s deletion and allows retry",
  async (kind) => {
    const user = userEvent.setup();
    const { queryClient } = renderWithProviders(<TrashPage />);
    const own = queryKeys.assets.library("a");
    const other = queryKeys.assets.library("b");
    queryClient.setQueryData(own, { total: 1 });
    queryClient.setQueryData(other, { total: 1 });
    const submit = async () => {
      if (kind === "all") {
        await user.click(screen.getByRole("button", { name: "Empty Trash" }));
        await user.click(screen.getByRole("button", { name: /^Delete$/ }));
      } else {
        await user.click(screen.getByRole("button", { name: "Delete Permanently" }));
      }
    };
    await submit();
    await waitFor(() => expect(mocks.error).toHaveBeenCalledTimes(1));
    expect(queryClient.getQueryState(own)?.isInvalidated).toBe(true);
    expect(queryClient.getQueryState(other)?.isInvalidated).toBe(false);
    await submit();
    const action = kind === "all" ? mocks.purgeAll : mocks.remove;
    await waitFor(() => expect(action).toHaveBeenCalledTimes(2));
    expect(mocks.error).toHaveBeenCalledTimes(1);
  },
);
