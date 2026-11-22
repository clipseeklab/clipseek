import type { ReactNode } from "react";
import { act, fireEvent, renderWithProviders, screen, waitFor } from "@test/helpers/render";
import { stubConvertFileSrc } from "@test/helpers/tauri";
import { buildAsset } from "@test/fixtures/assets";
import { beforeEach, expect, it, vi } from "vitest";
import { useImportStore } from "@/stores/import";
import type { AssetFilter } from "@/types";

const mocks = vi.hoisted(() => ({ list: vi.fn(), error: vi.fn() }));
vi.mock("@/lib/api", () => ({
  api: {
    assetList: mocks.list,
    assetDetail: async (id: number) => ({ ...buildAsset({ id }), frames: [] }),
    tagList: async () => [],
    folderSidebar: async () => ({ items: [], total: 0 }),
    folderPage: async () => ({ items: [], total: 0, hasMore: false }),
  },
}));
vi.mock("@/components/Toaster", () => ({ toast: { error: mocks.error } }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => vi.fn() }));
vi.mock("@/components/tasks/TaskQueueButton", () => ({ default: () => null }));
vi.mock("@/components/indexing/IndexBuildButton", () => ({ default: () => null }));
vi.mock("@/components/assets/StorageStatsButton", () => ({ default: () => null }));
vi.mock("@/components/assets/VirtualAssetGrid", () => ({
  default: ({
    items,
    renderItem,
  }: {
    items: ReturnType<typeof buildAsset>[];
    renderItem: (asset: ReturnType<typeof buildAsset>) => ReactNode;
  }) => (
    <div>
      {items.map((asset) => (
        <div key={asset.id}>{renderItem(asset)}</div>
      ))}
    </div>
  ),
}));
vi.mock("@/components/assets/AssetCard", () => ({
  default: ({
    asset,
    onPreview,
    onOpen,
  }: {
    asset: ReturnType<typeof buildAsset>;
    onPreview: (asset: ReturnType<typeof buildAsset>) => void;
    onOpen: (asset: ReturnType<typeof buildAsset>) => void;
  }) => (
    <>
      <button onClick={() => onPreview(asset)}>Preview {asset.id}</button>
      <button onClick={() => onOpen(asset)}>Details {asset.id}</button>
    </>
  ),
}));

import { AssetBrowserPage } from "@/pages/AssetsPage";

const firstPage = Array.from({ length: 40 }, (_, index) => buildAsset({ id: 100 - index }));
const following = buildAsset({ id: 60 });

beforeEach(() => {
  vi.clearAllMocks();
  stubConvertFileSrc();
  useImportStore.setState({ tasks: [], progress: null, liveAssets: new Map() });
  mocks.list.mockImplementation(async (filter: AssetFilter) => ({
    items: filter.page === 2 ? [following] : firstPage,
    total: 41,
    page: filter.page ?? 1,
    hasMore: filter.page !== 2,
  }));
});

it.each(["card", "details"])(
  "continues from item 40 to 41 from the %s preview using the original folder filter and cursor",
  async (entry) => {
    renderWithProviders(
      <AssetBrowserPage filter={{ folderId: 7 }} emptyHint="assets" importFolder={7} />,
    );
    fireEvent.click(
      await screen.findByRole("button", { name: entry === "card" ? "Preview 61" : "Details 61" }),
    );
    if (entry === "details")
      fireEvent.click(await screen.findByRole("button", { name: "Enlarge preview" }));
    expect(await screen.findByText("40 / 41")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Next asset" }));
    expect(await screen.findByText("41 / 41")).toBeVisible();
    expect(screen.getByAltText(following.fileName)).toBeVisible();
    expect(mocks.list).toHaveBeenLastCalledWith(
      expect.objectContaining({
        folderId: 7,
        page: 2,
        pageSize: 40,
        cursorImportedAt: 61,
        cursorId: 61,
      }),
      undefined,
    );
    expect(screen.getByRole("button", { name: "Next asset" })).toBeDisabled();
  },
);

it("closes the old preview on scope change and does not open it when the old page resolves", async () => {
  let finish!: (value: unknown) => void;
  mocks.list.mockImplementation((filter: AssetFilter) =>
    filter.page === 2
      ? new Promise((resolve) => {
          finish = resolve;
        })
      : Promise.resolve({ items: firstPage, total: 41, page: 1, hasMore: true }),
  );
  const { rerender } = renderWithProviders(
    <AssetBrowserPage filter={{ folderId: 7 }} emptyHint="assets" importFolder={7} />,
  );
  fireEvent.click(await screen.findByRole("button", { name: "Preview 61" }));
  fireEvent.click(screen.getByRole("button", { name: "Next asset" }));
  await waitFor(() => expect(mocks.list).toHaveBeenCalledTimes(2));
  rerender(<AssetBrowserPage filter={{ folderId: 8 }} emptyHint="assets" importFolder={8} />);
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  await waitFor(() =>
    expect(mocks.list).toHaveBeenLastCalledWith(
      expect.objectContaining({ folderId: 8, page: 1 }),
      undefined,
    ),
  );
  await act(async () => finish({ items: [following], total: 41, page: 2, hasMore: false }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Preview 60" })).not.toBeInTheDocument();
});

it("can retry a failed page without losing the current image or pagination cursor", async () => {
  mocks.list.mockReset();
  mocks.list
    .mockResolvedValueOnce({ items: firstPage, total: 41, page: 1, hasMore: true })
    .mockRejectedValueOnce(new Error("page unavailable"))
    .mockResolvedValueOnce({ items: [following], total: 41, page: 2, hasMore: false });
  renderWithProviders(
    <AssetBrowserPage filter={{ folderId: 7 }} emptyHint="assets" importFolder={7} />,
  );
  fireEvent.click(await screen.findByRole("button", { name: "Preview 61" }));
  fireEvent.click(screen.getByRole("button", { name: "Next asset" }));
  await waitFor(() => expect(mocks.error).toHaveBeenCalledWith("page unavailable"));
  expect(screen.getByText("40 / 41")).toBeVisible();
  expect(screen.getByRole("button", { name: "Next asset" })).toBeEnabled();
  fireEvent.click(screen.getByRole("button", { name: "Next asset" }));
  expect(await screen.findByText("41 / 41")).toBeVisible();
  expect(mocks.list).toHaveBeenCalledTimes(3);
  expect(mocks.list.mock.calls[1]).toEqual(mocks.list.mock.calls[2]);
});
