import userEvent from "@testing-library/user-event";
import { screen, waitFor, within } from "@test/helpers/render";
import { renderWithProviders } from "@test/helpers/render";
import { stubConvertFileSrc } from "@test/helpers/tauri";
import { buildAsset } from "@test/fixtures/assets";
import { useImportStore } from "@/stores/import";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  isWindows: true,
  navigate: vi.fn(),
  toastError: vi.fn(),
  useAssetPages: vi.fn(),
  openFileDialog: vi.fn(),
  refetch: vi.fn(),
  indexBuild: vi.fn(),
  modelStatus: vi.fn(),
}));
vi.mock("react-router-dom", async (importOriginal) => ({
  ...(await importOriginal<typeof import("react-router-dom")>()),
  useNavigate: () => mocks.navigate,
}));
vi.mock("@/lib/platform", () => ({
  get isWindows() {
    return mocks.isWindows;
  },
}));

vi.mock("@/hooks/useAssetPages", () => ({
  useAssetPages: mocks.useAssetPages,
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.openFileDialog }));
vi.mock("@/components/assets/VirtualAssetGrid", () => ({
  default: ({
    items,
    renderItem,
  }: {
    items: ReturnType<typeof buildAsset>[];
    renderItem: (asset: ReturnType<typeof buildAsset>) => React.ReactNode;
  }) => (
    <div data-testid="asset-grid">
      {items.map((asset) => (
        <span key={asset.id}>
          {asset.fileName}
          {renderItem(asset)}
        </span>
      ))}
    </div>
  ),
}));
vi.mock("@/components/assets/MasonryGrid", () => ({ default: () => null }));
vi.mock("@/components/assets/AssetCard", () => ({
  default: ({
    asset,
    onSimilar,
    onSelect,
    onOpen,
  }: {
    asset: ReturnType<typeof buildAsset>;
    onSimilar: (asset: ReturnType<typeof buildAsset>) => void;
    onSelect: (id: number) => void;
    onOpen: (asset: ReturnType<typeof buildAsset>) => void;
  }) => (
    <>
      <button onClick={() => onSimilar(asset)}>Find Similar {asset.id}</button>
      <button onClick={() => onSelect(asset.id)}>Select asset {asset.id}</button>
      <button onClick={() => onOpen(asset)}>Open details {asset.id}</button>
    </>
  ),
}));
vi.mock("@/components/assets/GridSizeControl", () => ({ default: () => null }));
vi.mock("@/components/assets/AssetFilters", async (importOriginal) => {
  const original = await importOriginal<typeof import("@/components/assets/AssetFilters")>();
  return {
    ...original,
    AssetFilters: ({
      value,
      onChange,
    }: {
      value: typeof original.EMPTY_FILTER;
      onChange: (value: typeof original.EMPTY_FILTER) => void;
    }) => (
      <button onClick={() => onChange({ ...value, kinds: ["image"], nameQuery: "cover" })}>
        Apply image filter
      </button>
    ),
  };
});
vi.mock("@/components/assets/AssetDialogs", () => ({
  AddTagsDialog: () => null,
  AssetDetailPanel: ({
    open,
    onOpenChange,
  }: {
    open: boolean;
    onOpenChange: (open: boolean) => void;
  }) => (open ? <button onClick={() => onOpenChange(false)}>Close details</button> : null),
  AssetSourcePreview: () => null,
  MoveToFolderDialog: () => null,
  MoveToLibraryDialog: () => null,
}));
vi.mock("@/components/Toaster", () => ({
  toast: { success: vi.fn(), error: mocks.toastError },
}));
vi.mock("@/lib/api", () => ({
  api: {
    onlineJobs: async () => [],
    processingSummary: async () => ({ counts: {}, activeCount: 0, revision: 1 }),
    processingList: async () => ({ items: [], total: 0, page: 1, pageSize: 12, counts: {} }),
    assetStorageStats: vi.fn(),
    assetTrash: vi.fn(),
    indexBuild: mocks.indexBuild,
    modelStatus: mocks.modelStatus,
  },
}));

import AssetsPage, { TagAssetsRoute, AssetBrowserPage } from "@/pages/AssetsPage";

function pageResult(overrides: Record<string, unknown> = {}) {
  return {
    items: [],
    total: 0,
    isLoading: false,
    isError: false,
    isFetchingNextPage: false,
    fetchNextPage: vi.fn(),
    hasNextPage: false,
    refetch: mocks.refetch,
    ...overrides,
  };
}

describe("AssetsPage", () => {
  it("hides the toolbar import action while details are open and restores it after closing", async () => {
    mocks.useAssetPages.mockReturnValue(pageResult({ items: [buildAsset({ id: 7 })], total: 1 }));
    const user = userEvent.setup();
    renderWithProviders(<AssetsPage />);
    const toolbar = within(screen.getByRole("banner"));
    expect(toolbar.getByRole("button", { name: "Import Assets" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Open details 7" }));
    expect(toolbar.queryByRole("button", { name: "Import Assets" })).not.toBeInTheDocument();
    expect(toolbar.getByRole("button", { name: "Task Queue" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Close details" }));
    expect(toolbar.getByRole("button", { name: "Import Assets" })).toBeVisible();
  });
  it("replaces the three toolbar queues with one task queue", () => {
    renderWithProviders(<AssetsPage />);
    const toolbar = within(screen.getByRole("banner"));
    expect(toolbar.getByRole("button", { name: "Task Queue" })).toBeVisible();
    for (const name of ["Trim Queue", "Download Queue", "Import Queue"]) {
      expect(toolbar.queryByRole("button", { name })).not.toBeInTheDocument();
    }
  });

  it.each([
    { items: [buildAsset({ id: 7 }), buildAsset({ id: 8 })], selectedIds: [7], kinds: ["visual"] },
    { items: [buildAsset({ id: 7, kind: "gif" })], selectedIds: [7], kinds: ["visual"] },
    {
      items: [buildAsset({ id: 7, kind: "video" })],
      selectedIds: [7],
      kinds: ["visual"],
    },
    {
      items: [buildAsset({ id: 7 }), buildAsset({ id: 8, kind: "video" })],
      selectedIds: [7, 8],
      kinds: ["visual"],
    },
  ])(
    "builds applicable indexes for the selected assets: %j",
    async ({ items, selectedIds, kinds }) => {
      const user = userEvent.setup();
      mocks.useAssetPages.mockReturnValue(pageResult({ items, total: items.length }));
      mocks.modelStatus.mockResolvedValue({ ready: true });
      mocks.indexBuild.mockResolvedValue(2);
      renderWithProviders(<AssetsPage />);
      for (const id of selectedIds) {
        await user.click(screen.getByRole("button", { name: `Select asset ${id}` }));
      }
      expect(
        screen.queryByRole("button", { name: "Build search indexes" }),
      ).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Batch actions" }));
      await user.click(screen.getByRole("menuitem", { name: "Build search indexes" }));
      const dialog = screen.getByRole("dialog", { name: "Build search indexes" });
      expect(screen.queryByRole("menu")).not.toBeInTheDocument();
      if (kinds.length === 1) {
        expect(within(dialog).queryByRole("checkbox")).not.toBeInTheDocument();
        expect(within(dialog).queryByText(/subtitle|transcrib/i)).not.toBeInTheDocument();
      } else {
        expect(within(dialog).getByRole("checkbox", { name: "Visual index" })).toBeChecked();
        expect(within(dialog).getByRole("checkbox", { name: /Subtitle index/ })).toBeChecked();
      }
      await user.click(within(dialog).getByRole("button", { name: "Build indexes" }));
      await waitFor(() =>
        expect(mocks.indexBuild).toHaveBeenCalledWith({ assetIds: selectedIds }, kinds),
      );
      await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    },
  );

  it("searches the HEIC thumbnail even when its referenced original is missing", async () => {
    mocks.useAssetPages.mockReturnValue(
      pageResult({
        items: [
          buildAsset({
            fileName: "IMG_1451.HEIC",
            absPath: "/missing/IMG_1451.HEIC",
            sourceMissing: true,
            thumbPath: "/library/IMG_1451_thumbnail.webp",
          }),
        ],
        total: 1,
      }),
    );
    renderWithProviders(<AssetsPage />);
    await userEvent.setup().click(screen.getByRole("button", { name: "Find Similar 1" }));
    expect(mocks.navigate).toHaveBeenCalledWith(
      "/search",
      expect.objectContaining({
        state: expect.objectContaining({ imageSearchPath: "/library/IMG_1451_thumbnail.webp" }),
      }),
    );
  });

  it("reports a missing thumbnail instead of searching the original", async () => {
    mocks.useAssetPages.mockReturnValue(pageResult({ items: [buildAsset()], total: 1 }));
    renderWithProviders(<AssetsPage />);
    await userEvent.setup().click(screen.getByRole("button", { name: "Find Similar 1" }));
    expect(mocks.navigate).not.toHaveBeenCalled();
    expect(mocks.toastError).toHaveBeenCalled();
  });

  it("does not expose native window drag regions in the Windows content toolbar", () => {
    mocks.useAssetPages.mockReturnValue(pageResult({ items: [buildAsset()], total: 1 }));
    const { container } = renderWithProviders(<AssetsPage />);
    expect(container.querySelector("[data-tauri-drag-region]")).toBeNull();
    expect(screen.getByRole("button", { name: "Filters" })).toBeEnabled();
  });
  it("retains the content drag region on macOS", () => {
    mocks.isWindows = false;
    const { container } = renderWithProviders(<AssetsPage />);
    expect(container.querySelector("header[data-tauri-drag-region]")).not.toBeNull();
  });
  beforeEach(() => {
    stubConvertFileSrc();
    mocks.isWindows = true;
    useImportStore.setState(useImportStore.getInitialState(), true);
    mocks.useAssetPages.mockReturnValue(pageResult());
  });

  it("shows loading and empty states", () => {
    mocks.useAssetPages.mockReturnValue(pageResult({ isLoading: true }));
    const view = renderWithProviders(<AssetsPage />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();

    mocks.useAssetPages.mockReturnValue(pageResult());
    view.rerender(<AssetsPage />);
    expect(screen.getByText("No assets yet")).toBeInTheDocument();
    expect(screen.getByText(/Drop files or folders/)).toBeInTheDocument();
  });

  it("shows a retry action when the asset query fails", async () => {
    const user = userEvent.setup();
    mocks.useAssetPages.mockReturnValue(pageResult({ isError: true }));

    renderWithProviders(<AssetsPage />);

    expect(screen.getByRole("alert")).toHaveTextContent("Assets could not be loaded");
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(mocks.refetch).toHaveBeenCalledOnce();
  });

  it("renders loaded assets and their total", () => {
    const assets = [
      buildAsset({ id: 1, fileName: "cover.jpg" }),
      buildAsset({ id: 2, fileName: "trailer.mp4", kind: "video", mimeType: "video/mp4" }),
    ];
    mocks.useAssetPages.mockReturnValue(pageResult({ items: assets, total: 2 }));

    renderWithProviders(<AssetsPage />);

    expect(screen.getByText("2 total")).toBeInTheDocument();
    expect(screen.getByText("cover.jpg")).toBeInTheDocument();
    expect(screen.getByText("trailer.mp4")).toBeInTheDocument();
  });

  it("converts the filter controls into an asset query", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AssetsPage />);

    await user.click(screen.getByRole("button", { name: "Filters" }));
    await user.click(screen.getByRole("button", { name: "Apply image filter" }));

    await waitFor(() => {
      expect(mocks.useAssetPages).toHaveBeenLastCalledWith(
        expect.objectContaining({ kinds: ["image"], query: "cover" }),
      );
    });
  });

  it("queues files selected from the import dialog", async () => {
    const user = userEvent.setup();
    mocks.openFileDialog.mockResolvedValue(["/media/a.jpg", "/media/b.mp4"]);
    renderWithProviders(<AssetsPage />);

    await user.click(screen.getAllByRole("button", { name: "Import Assets" })[0]);

    await waitFor(() => expect(useImportStore.getState().tasks).toHaveLength(1));
    expect(useImportStore.getState().tasks[0]).toMatchObject({
      paths: ["/media/a.jpg", "/media/b.mp4"],
      folderId: null,
    });
  });
});

it("shows tag scope and returns to tag management", async () => {
  mocks.useAssetPages.mockReturnValue(pageResult());
  renderWithProviders(<TagAssetsRoute tagName="Travel" />);
  expect(screen.getByRole("heading", { name: "Travel", level: 1 })).toBeVisible();
  expect(screen.getByRole("button", { name: "Filters" })).toBeVisible();
  await userEvent.setup().click(screen.getByRole("button", { name: "Filters" }));
  expect(screen.queryByText("# Travel")).not.toBeInTheDocument();
  await userEvent.setup().click(screen.getByRole("button", { name: "Back" }));
  expect(mocks.navigate).toHaveBeenCalledWith("/tags");
});
it("shows folder scope and returns to folder management", async () => {
  mocks.useAssetPages.mockReturnValue(pageResult());
  renderWithProviders(
    <AssetBrowserPage
      filter={{ folderId: 7 }}
      emptyHint="folders"
      importFolder={7}
      backTo="/folders"
      scopeName="Posters"
    />,
  );
  expect(screen.getByRole("heading", { name: "Posters", level: 1 })).toBeVisible();
  expect(screen.getByText("This folder is empty")).toBeVisible();
  expect(
    screen.getByText(
      "Drop multiple images, videos or GIFs here to import them into this folder, or click Import assets.",
    ),
  ).toBeVisible();
  expect(screen.queryByText("No folders yet")).not.toBeInTheDocument();
  await userEvent.setup().click(screen.getByRole("button", { name: "Back" }));
  expect(mocks.navigate).toHaveBeenCalledWith("/folders");
});

it("clears the tag drop context when leaving the tag route", () => {
  mocks.useAssetPages.mockReturnValue(pageResult());
  const { rerender } = renderWithProviders(<TagAssetsRoute tagName="Travel" />);
  expect(useImportStore.getState().dropTags).toEqual(["Travel"]);
  rerender(<TagAssetsRoute tagName="Portrait" />);
  expect(useImportStore.getState().dropTags).toEqual(["Portrait"]);
  rerender(<AssetsPage />);
  expect(useImportStore.getState().dropTags).toEqual([]);
});
it("captures the tag in file-picker imports without applying unrelated filter choices", async () => {
  mocks.useAssetPages.mockReturnValue(pageResult());
  mocks.openFileDialog.mockResolvedValue(["/photo.jpg"]);
  renderWithProviders(<TagAssetsRoute tagName="Travel" />);
  await userEvent.setup().click(screen.getAllByRole("button", { name: "Import Assets" })[0]);
  await waitFor(() =>
    expect(useImportStore.getState().tasks[0]).toMatchObject({
      paths: ["/photo.jpg"],
      folderId: null,
      tags: ["Travel"],
    }),
  );
});

it("keeps live imports within the fixed tag scope", () => {
  mocks.useAssetPages.mockReturnValue(pageResult());
  useImportStore.setState({
    liveAssets: new Map([
      [71, buildAsset({ id: 71, fileName: "other-tag.jpg", tags: ["Portrait"] })],
      [72, buildAsset({ id: 72, fileName: "matching-tag.jpg", tags: ["Travel"] })],
    ]),
  });
  renderWithProviders(<TagAssetsRoute tagName="Travel" />);
  expect(screen.queryByText("other-tag.jpg")).not.toBeInTheDocument();
  expect(screen.getByText("matching-tag.jpg")).toBeVisible();
});
