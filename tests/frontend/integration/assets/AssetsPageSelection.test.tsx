import { useAppStore } from "@/stores/app";
import { queryKeys } from "@/lib/queryKeys";
/** Asset-page selection, select-all, and batch toolbar coverage. */
import userEvent from "@testing-library/user-event";
import { useLocation } from "react-router-dom";
import { screen, waitFor, within } from "@test/helpers/render";
import { renderWithProviders } from "@test/helpers/render";
import { buildAsset } from "@test/fixtures/assets";
import { useImportStore } from "@/stores/import";
import type { Asset } from "@/types";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  useAssetPages: vi.fn(),
  assetTrash: vi.fn(),
  aiBatch: vi.fn(),
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock("@/hooks/useAssetPages", () => ({ useAssetPages: mocks.useAssetPages }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@/components/Toaster", () => ({
  toast: { success: mocks.toastSuccess, error: mocks.toastError },
}));
vi.mock("@/lib/api", () => ({
  api: {
    assetStorageStats: vi.fn(),
    assetTrash: mocks.assetTrash,
    aiSubtitleSubmitBatch: mocks.aiBatch,
  },
}));
vi.mock("@/components/assets/VirtualAssetGrid", () => ({
  default: ({
    items,
    renderItem,
  }: {
    items: Asset[];
    renderItem: (asset: Asset) => React.ReactNode;
  }) => <div data-testid="asset-grid">{items.map((asset) => renderItem(asset))}</div>,
}));
vi.mock("@/components/assets/MasonryGrid", () => ({ default: () => null }));
vi.mock("@/components/assets/GridSizeControl", () => ({ default: () => null }));
vi.mock("@/components/tasks/TaskQueueButton", () => ({ default: () => null }));
vi.mock("@/components/assets/AssetFilters", async (importOriginal) => {
  const original = await importOriginal<typeof import("@/components/assets/AssetFilters")>();
  return { ...original, AssetFilters: () => null };
});
vi.mock("@/components/assets/AssetCard", () => ({
  default: ({
    asset,
    onSelect,
    onOpen,
    onPreview,
    onAddTags,
    onMove,
    onBatch,
    onTrash,
    onSimilar,
  }: {
    asset: Asset;
    onSelect: (id: number, additive: boolean) => void;
    onOpen: (asset: Asset) => void;
    onPreview: (asset: Asset) => void;
    onAddTags: (asset: Asset) => void;
    onMove: (asset: Asset) => void;
    onBatch: (asset: Asset) => void;
    onTrash: (asset: Asset) => void;
    onSimilar: (asset: Asset) => void;
  }) => (
    <div>
      <button onClick={() => onSelect(asset.id, true)}>select-{asset.fileName}</button>
      <button onClick={() => onOpen(asset)}>open-{asset.fileName}</button>
      <button onClick={() => onPreview(asset)}>preview-{asset.fileName}</button>
      <button onClick={() => onAddTags(asset)}>tags-{asset.fileName}</button>
      <button onClick={() => onMove(asset)}>move-{asset.fileName}</button>
      <button onClick={() => onBatch(asset)}>batch-{asset.fileName}</button>
      <button onClick={() => onTrash(asset)}>trash-{asset.fileName}</button>
      <button onClick={() => onSimilar(asset)}>similar-{asset.fileName}</button>
    </div>
  ),
}));
vi.mock("@/components/assets/AssetDialogs", () => ({
  AddTagsDialog: ({ assets, open }: { assets: Asset[]; open: boolean }) =>
    open ? <div>tags-dialog:{assets.map((a) => a.fileName).join(",")}</div> : null,
  AssetDetailPanel: () => null,
  AssetSourcePreview: ({
    asset,
    assets: previewAssets,
    open,
  }: {
    asset: Asset | null;
    assets: Asset[];
    open: boolean;
  }) =>
    open ? (
      <div>
        <div>source-preview:{asset?.fileName}</div>
        <output aria-label="preview order">
          {previewAssets.map((item) => item.fileName).join(",")}
        </output>
      </div>
    ) : null,
  MoveToFolderDialog: ({ assets, open }: { assets: Asset[]; open: boolean }) =>
    open ? <div>move-dialog:{assets.map((a) => a.fileName).join(",")}</div> : null,
  MoveToLibraryDialog: ({ assets, open }: { assets: Asset[]; open: boolean }) =>
    open ? <div>move-library:{assets.length}</div> : null,
}));

import AssetsPage from "@/pages/AssetsPage";
import { stubConvertFileSrc } from "@test/helpers/tauri";

const assets = [
  buildAsset({ id: 1, fileName: "cover.jpg", thumbPath: "/library/cover_thumbnail.webp" }),
  buildAsset({ id: 2, fileName: "trailer.mp4", kind: "video", mimeType: "video/mp4" }),
  buildAsset({ id: 3, fileName: "poster.png" }),
];

function SearchRouteObserver() {
  const location = useLocation();
  return (
    <output aria-label="search route">
      {location.pathname}:{location.state?.backgroundLocation?.pathname}:
      {location.state?.imageSearchPath}
    </output>
  );
}

function pageResult(overrides: Record<string, unknown> = {}) {
  return {
    items: assets,
    total: assets.length,
    isLoading: false,
    isError: false,
    isFetchingNextPage: false,
    fetchNextPage: vi.fn(),
    hasNextPage: false,
    refetch: vi.fn(),
    ...overrides,
  };
}

describe("AssetsPage selection", () => {
  it("keeps the source page in the similar-image search navigation", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <>
        <AssetsPage />
        <SearchRouteObserver />
      </>,
      { route: "/assets" },
    );
    await user.click(screen.getByRole("button", { name: "similar-cover.jpg" }));
    expect(screen.getByLabelText("search route")).toHaveTextContent(
      `/search:/assets:${assets[0].thumbPath}`,
    );
  });
  beforeEach(() => {
    stubConvertFileSrc();
    useImportStore.setState(useImportStore.getInitialState(), true);
    mocks.useAssetPages.mockReturnValue(pageResult());
    mocks.assetTrash.mockResolvedValue(undefined);
  });

  it("entering select mode from a card shows the batch toolbar", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AssetsPage />);

    await user.click(screen.getByRole("button", { name: "select-cover.jpg" }));

    expect(screen.getByText("1 selected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add Tags" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Move to Folder" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Selected assets (1)" })).toHaveTextContent(
      "1 selected",
    );
    expect(screen.queryByRole("button", { name: "View selected" })).not.toBeInTheDocument();
  });

  it("select all toggles every loaded asset and back to none", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AssetsPage />);
    await user.click(screen.getByRole("button", { name: "select-cover.jpg" }));

    await user.click(screen.getByRole("button", { name: "Select all shown" }));
    expect(screen.getByText("3 selected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Clear selection" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Clear selection" }));
    expect(screen.getByText("0 selected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Selected assets (0)" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Add Tags" })).toBeDisabled();
  });

  it("batch trash removes the selection, refreshes queries and reports success", async () => {
    const user = userEvent.setup();
    const { queryClient } = renderWithProviders(<AssetsPage />);
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");
    await user.click(screen.getByRole("button", { name: "select-cover.jpg" }));
    await user.click(screen.getByRole("button", { name: "select-trailer.mp4" }));

    await user.click(screen.getByRole("button", { name: "Batch actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Move to Trash" }));

    await waitFor(() => expect(mocks.assetTrash).toHaveBeenCalledWith([1, 2]));
    expect(mocks.toastSuccess).toHaveBeenCalledWith("Done");
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.assets.library(useAppStore.getState().current?.id),
    });
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.folders.library(useAppStore.getState().current?.id),
    });
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.storageStats.library(useAppStore.getState().current?.id),
    });
    expect(screen.getByText("0 selected")).toBeInTheDocument();
  });

  it("batch trash failure shows an error and keeps the selection", async () => {
    const user = userEvent.setup();
    mocks.assetTrash.mockRejectedValue(new Error("db locked"));
    renderWithProviders(<AssetsPage />);
    await user.click(screen.getByRole("button", { name: "select-poster.png" }));

    await user.click(screen.getByRole("button", { name: "Batch actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Move to Trash" }));

    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledOnce());
    expect(screen.getByText("1 selected")).toBeInTheDocument();
  });

  it("batch dialogs receive every selected asset", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AssetsPage />);
    await user.click(screen.getByRole("button", { name: "select-cover.jpg" }));
    await user.click(screen.getByRole("button", { name: "select-poster.png" }));

    await user.click(screen.getByRole("button", { name: "Add Tags" }));
    expect(screen.getByText("tags-dialog:cover.jpg,poster.png")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Move to Folder" }));
    expect(screen.getByText("move-dialog:cover.jpg,poster.png")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Batch actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Move to Library" }));
    expect(screen.getByText("move-library:2")).toBeInTheDocument();
  });

  it("single-card actions target only that asset", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AssetsPage />);

    await user.click(screen.getByRole("button", { name: "tags-trailer.mp4" }));
    expect(screen.getByText("tags-dialog:trailer.mp4")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "move-cover.jpg" }));
    expect(screen.getByText("move-dialog:cover.jpg")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "preview-poster.png" }));
    expect(screen.getByText("source-preview:poster.png")).toBeInTheDocument();
    expect(screen.getByLabelText("preview order")).toHaveTextContent(
      "cover.jpg,trailer.mp4,poster.png",
    );
  });

  it("the selected list supports filtering and removing one asset", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AssetsPage />);
    for (const name of ["select-cover.jpg", "select-trailer.mp4", "select-poster.png"]) {
      await user.click(screen.getByRole("button", { name: name }));
    }

    await user.click(screen.getByRole("button", { name: "Selected assets (3)" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("cover.jpg")).toBeInTheDocument();
    expect(within(dialog).getByText("trailer.mp4")).toBeInTheDocument();
    expect(within(dialog).getByText("poster.png")).toBeInTheDocument();

    await user.type(within(dialog).getByPlaceholderText("Search by file name"), "trail");
    expect(within(dialog).getByText("trailer.mp4")).toBeInTheDocument();
    expect(within(dialog).queryByText("cover.jpg")).not.toBeInTheDocument();

    await user.clear(within(dialog).getByPlaceholderText("Search by file name"));
    const coverRow = within(dialog).getByText("cover.jpg").parentElement!;
    await user.click(within(coverRow).getByTitle("Remove from selection"));
    expect(within(dialog).queryByText("cover.jpg")).not.toBeInTheDocument();
    expect(screen.getByText("2 selected")).toBeInTheDocument();
  });

  it("cancel leaves select mode and hides the toolbar", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AssetsPage />);
    await user.click(screen.getByRole("button", { name: "select-cover.jpg" }));

    await user.click(screen.getByRole("button", { name: "Close" }));

    expect(screen.queryByText("1 selected")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Add Tags" })).not.toBeInTheDocument();
  });
});
