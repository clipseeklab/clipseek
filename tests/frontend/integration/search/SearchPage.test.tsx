/** Text/image search, selected/dropped queries, pagination, and bulk trash coverage. */
import userEvent from "@testing-library/user-event";
import { fireEvent, screen, waitFor, within } from "@test/helpers/render";
import { renderWithProviders } from "@test/helpers/render";
import { buildAsset } from "@test/fixtures/assets";
import { useAppStore } from "@/stores/app";
import { useImportStore } from "@/stores/import";
import type { SearchHit } from "@/types";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  track: vi.fn(),
  subtitleSearch: vi.fn(),
  searchText: vi.fn(),
  searchImage: vi.fn(),
  searchImagePreview: vi.fn(),
  assetDetail: vi.fn(),
  assetTrash: vi.fn(),
  openFileDialog: vi.fn(),
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));
vi.mock("@/lib/api", () => ({
  api: {
    subtitleSearch: mocks.subtitleSearch,
    folderList: async () => [],
    tagList: async () => [],
    searchText: mocks.searchText,
    searchImage: mocks.searchImage,
    searchImagePreview: mocks.searchImagePreview,
    assetTrash: mocks.assetTrash,
    assetDetail: mocks.assetDetail,
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.openFileDialog }));
vi.mock("@/components/Toaster", () => ({
  toast: { success: mocks.toastSuccess, error: mocks.toastError },
}));
vi.mock("@/components/assets/MasonryGrid", () => ({
  default: ({
    items,
    renderItem,
    onLoadMore,
    hasMore,
  }: {
    items: SearchHit[];
    renderItem: (hit: SearchHit) => React.ReactNode;
    onLoadMore?: () => void;
    hasMore?: boolean;
  }) => (
    <div data-testid="search-results">
      {items.map((hit) => renderItem(hit))}
      {onLoadMore && (
        <button onClick={onLoadMore} disabled={!hasMore}>
          load-more
        </button>
      )}
    </div>
  ),
}));
vi.mock("@/components/assets/GridSizeControl", () => ({ default: () => null }));
vi.mock("@/components/assets/HoverVideoPreview", () => ({ default: () => null }));
vi.mock("@/components/assets/AssetDialogs", () => ({
  AddTagsDialog: () => null,
  AssetDetailPanel: () => null,
  AssetSourcePreview: () => null,
  MoveToFolderDialog: () => null,
  MoveToLibraryDialog: () => null,
  VideoTrimDialog: () => null,
}));

import SearchPage from "@/pages/SearchPage";
import { stubConvertFileSrc } from "@test/helpers/tauri";

function buildHit(overrides: Partial<SearchHit> = {}): SearchHit {
  const id = overrides.frameId ?? 1;
  return {
    score: 0.87,
    frameId: id,
    frameUid: `frame-${id}`,
    seq: 0,
    timestampMs: 0,
    framePath: null,
    asset: buildAsset({ id, fileName: `result-${id}.jpg`, thumbPath: `/thumb-${id}.webp` }),
    ...overrides,
  };
}

describe("SearchPage", () => {
  it("shows folder and tag metadata on search result cards", async () => {
    mocks.searchText.mockResolvedValue({
      items: [
        buildHit({ asset: buildAsset({ folderName: "References", tags: ["concept", "green"] }) }),
      ],
      total: 1,
    });
    renderWithProviders(<SearchPage />, { route: "/search?q=concept" });
    expect(await screen.findByTitle("References")).toBeVisible();
    const tags = screen.getByRole("list", { name: "Tags" });
    expect(within(tags).getByText("concept")).toBeVisible();
    expect(within(tags).getByText("green")).toBeVisible();
  });

  it("does not fall back to an original when a result has no thumbnail", async () => {
    mocks.searchText.mockResolvedValue({
      items: [
        buildHit({
          asset: buildAsset({ fileName: "missing.HEIC", thumbPath: null }),
        }),
      ],
      total: 1,
    });
    const user = userEvent.setup();
    renderWithProviders(<SearchPage />);
    await user.type(
      screen.getByPlaceholderText("Describe what you want, or drop an image…"),
      "photo{Enter}",
    );
    fireEvent.contextMenu(
      (await screen.findByText("missing.HEIC")).closest('[class*="rounded-lg"]')!,
    );
    await user.click(await screen.findByRole("menuitem", { name: "Find Similar" }));
    expect(mocks.searchImage).not.toHaveBeenCalled();
    expect(mocks.toastError).toHaveBeenCalled();
  });

  it("does not use the video file as a poster when neither a frame nor thumbnail exists", async () => {
    mocks.searchText.mockResolvedValue({
      items: [
        buildHit({ asset: buildAsset({ kind: "video", thumbPath: null, absPath: "C:/big.mkv" }) }),
      ],
      total: 1,
    });
    const user = userEvent.setup();
    const { container } = renderWithProviders(<SearchPage />);
    await user.type(
      screen.getByPlaceholderText("Describe what you want, or drop an image…"),
      "video{Enter}",
    );
    await screen.findByTestId("search-results");
    expect(container.querySelector('img[src*="big.mkv"]')).toBeNull();
  });

  it("paginates the submitted query even after editing or clearing the input", async () => {
    const user = userEvent.setup();
    mocks.searchText.mockResolvedValue({ items: [buildHit()], total: 150 });
    renderWithProviders(<SearchPage />);
    await user.type(
      screen.getByPlaceholderText("Describe what you want, or drop an image…"),
      "sunset{Enter}",
    );
    await screen.findByText("result-1.jpg");
    const input = screen.getByPlaceholderText("Describe what you want, or drop an image…");
    await user.clear(input);
    await user.type(input, "mountains");
    await user.click(screen.getByRole("button", { name: "load-more" }));
    expect(mocks.searchText).toHaveBeenLastCalledWith("sunset", expect.anything(), 2, 40);
  });

  beforeEach(() => {
    useAppStore.setState(useAppStore.getInitialState(), true);
    URL.createObjectURL = vi.fn(() => "blob:query-preview");
    URL.revokeObjectURL = vi.fn();
    mocks.searchImagePreview.mockResolvedValue([137, 80, 78, 71]);
    mocks.assetDetail.mockResolvedValue(
      buildAsset({ id: 42, kind: "video", fileName: "movie.mkv" }),
    );
    stubConvertFileSrc();
    useImportStore.setState(useImportStore.getInitialState(), true);
    mocks.subtitleSearch.mockResolvedValue({
      items: [],
      hasMore: false,
      nextCursor: null,
      partial: false,
    });
    mocks.searchText.mockResolvedValue({ items: [buildHit()], total: 1 });
    mocks.searchImage.mockResolvedValue({ items: [buildHit({ frameId: 9 })], total: 1 });
    mocks.assetTrash.mockResolvedValue(undefined);
  });

  it("starts with the centered search entry", () => {
    renderWithProviders(<SearchPage />);

    expect(screen.getByRole("heading", { name: "Search visuals" })).toBeVisible();
    expect(
      screen.getByPlaceholderText("Describe what you want, or drop an image…"),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Drop an image to search" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Search" })).toBeInTheDocument();
  });

  it("renders a converted HEIC query thumbnail instead of sending HEIC to the browser", async () => {
    mocks.openFileDialog.mockResolvedValue("C:/photos/IMG_1452.HEIC");
    const user = userEvent.setup();
    const view = renderWithProviders(<SearchPage />);
    await user.click(screen.getByRole("button", { name: "Drop an image to search" }));
    await waitFor(() =>
      expect(mocks.searchImagePreview).toHaveBeenCalledWith("C:/photos/IMG_1452.HEIC"),
    );
    expect(await screen.findByRole("img", { name: "Search by Image" })).toHaveAttribute(
      "src",
      "blob:query-preview",
    );
    expect(view.container.querySelector('img[src*="IMG_1452.HEIC"]')).toBeNull();
    expect(mocks.searchImage).toHaveBeenCalledWith("C:/photos/IMG_1452.HEIC", {}, 1, 40);
    view.unmount();
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:query-preview");
  });

  it("keeps image search usable when the HEIF query preview fails", async () => {
    mocks.searchImagePreview.mockRejectedValueOnce(new Error("preview failed"));
    mocks.openFileDialog.mockResolvedValue("C:/photos/query.heif");
    const user = userEvent.setup();
    const { container } = renderWithProviders(<SearchPage />);
    await user.click(screen.getByRole("button", { name: "Drop an image to search" }));
    expect(await screen.findByText("result-9.jpg")).toBeInTheDocument();
    expect(container.querySelector('img[src*="query.heif"]')).toBeNull();
    expect(screen.queryByRole("img", { name: "Search by Image" })).toBeNull();
  });

  it("converts a query preview when its browser image cannot load", async () => {
    mocks.openFileDialog.mockResolvedValue("C:/photos/query.jpg");
    const user = userEvent.setup();
    renderWithProviders(<SearchPage />);
    await user.click(screen.getByRole("button", { name: "Drop an image to search" }));
    fireEvent.error(await screen.findByRole("img", { name: "Search by Image" }));
    await waitFor(() =>
      expect(mocks.searchImagePreview).toHaveBeenCalledWith("C:/photos/query.jpg"),
    );
    expect(await screen.findByRole("img", { name: "Search by Image" })).toHaveAttribute(
      "src",
      "blob:query-preview",
    );
  });

  it("returns from a similar-image search to the previous search results", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SearchPage />);
    await user.type(
      screen.getByPlaceholderText("Describe what you want, or drop an image…"),
      "sunset{Enter}",
    );
    const result = await screen.findByText("result-1.jpg");
    fireEvent.contextMenu(result.closest('[class*="rounded-lg"]')!);
    await user.click(await screen.findByRole("menuitem", { name: "Find Similar" }));
    expect(await screen.findByText("result-9.jpg")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByText("result-1.jpg")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Describe what you want, or drop an image…")).toHaveValue(
      "sunset",
    );
  });

  it("scopes text and image searches to the video and removes the scope for global search", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SearchPage />, { route: "/search?assetId=42" });
    expect(await screen.findByRole("button", { name: "Only in: movie.mkv" })).toBeInTheDocument();
    await user.type(
      screen.getByPlaceholderText("Describe a scene in this video…"),
      "sunset{Enter}",
    );
    await waitFor(() =>
      expect(mocks.searchText).toHaveBeenLastCalledWith(
        "sunset",
        expect.objectContaining({ assetId: 42 }),
        1,
        40,
      ),
    );
    mocks.openFileDialog.mockResolvedValue("/query.jpg");
    await user.click(screen.getByTitle("Search by Image"));
    await waitFor(() =>
      expect(mocks.searchImage).toHaveBeenLastCalledWith(
        "/query.jpg",
        expect.objectContaining({ assetId: 42 }),
        1,
        40,
      ),
    );
    await user.click(screen.getByRole("button", { name: "Search all assets" }));
    await waitFor(() =>
      expect(mocks.searchImage).toHaveBeenLastCalledWith("/query.jpg", {}, 1, 40),
    );
  });

  it("keeps the video scope when loading another result page", async () => {
    mocks.searchText.mockResolvedValue({ items: [buildHit()], total: 80 });
    const user = userEvent.setup();
    renderWithProviders(<SearchPage />, { route: "/search?assetId=42" });
    await user.type(
      screen.getByPlaceholderText("Describe a scene in this video…"),
      "sunset{Enter}",
    );
    await screen.findByTestId("search-results");
    await user.click(screen.getByRole("button", { name: "load-more" }));
    await waitFor(() =>
      expect(mocks.searchText).toHaveBeenLastCalledWith(
        "sunset",
        expect.objectContaining({ assetId: 42 }),
        2,
        40,
      ),
    );
  });

  it("searches the submitted route query immediately and places an icon back button before the input", async () => {
    renderWithProviders(<SearchPage />, { route: "/search?assetId=42&q=street%20car" });
    await waitFor(() =>
      expect(mocks.searchText).toHaveBeenCalledWith(
        "street car",
        expect.objectContaining({ assetId: 42 }),
        1,
        40,
      ),
    );
    const back = screen.getByRole("button", { name: "Back" });
    const input = screen.getByPlaceholderText("Describe a scene in this video…");
    expect(back.querySelector("svg")).not.toBeNull();
    expect(back.textContent).toBe("");
    expect(back.compareDocumentPosition(input) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(input).toHaveValue("street car");
  });

  it("runs a text search on Enter and renders hits without a match-percentage badge", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SearchPage />);

    await user.type(
      screen.getByPlaceholderText("Describe what you want, or drop an image…"),
      "sunset{enter}",
    );

    await waitFor(() =>
      expect(mocks.searchText).toHaveBeenCalledWith("sunset", expect.anything(), 1, 40),
    );
    expect(await screen.findByText("result-1.jpg")).toBeInTheDocument();
    expect(screen.queryByText("87%")).not.toBeInTheDocument();
  });

  it("shows the empty state and surfaces backend errors", async () => {
    const user = userEvent.setup();
    mocks.searchText.mockResolvedValue({ items: [], total: 0 });
    renderWithProviders(<SearchPage />);
    await user.type(
      screen.getByPlaceholderText("Describe what you want, or drop an image…"),
      "zzz{enter}",
    );
    expect(await screen.findByText("No matching assets")).toBeInTheDocument();

    mocks.searchText.mockRejectedValue(new Error("model missing"));
    const input = screen.getByRole("textbox");
    await user.clear(input);
    await user.type(input, "retry{enter}");
    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledOnce());
  });

  it("searches by an image picked from the dialog and clears back to empty results", async () => {
    const user = userEvent.setup();
    mocks.openFileDialog.mockResolvedValue("/media/query.jpg");
    renderWithProviders(<SearchPage />);

    await user.click(screen.getByRole("button", { name: "Drop an image to search" }));

    await waitFor(() =>
      expect(mocks.searchImage).toHaveBeenCalledWith("/media/query.jpg", expect.anything(), 1, 40),
    );
    expect(await screen.findByText("result-9.jpg")).toBeInTheDocument();
    expect(screen.getByText("Search by Image")).toBeInTheDocument();

    const imageChip = screen.getByText("Search by Image").parentElement!;
    await user.click(within(imageChip).getByRole("button"));
    await waitFor(() => expect(screen.getByText("No matching assets")).toBeInTheDocument());
  });

  it("starts an image search from a dropped file recorded by the import store", async () => {
    useImportStore.setState({ searchDropPath: "/media/dropped.png" });
    renderWithProviders(<SearchPage />);

    await waitFor(() =>
      expect(mocks.searchImage).toHaveBeenCalledWith(
        "/media/dropped.png",
        expect.anything(),
        1,
        40,
      ),
    );
    expect(useImportStore.getState().searchDropPath).toBeNull();
  });

  it("appends the next page when more results exist", async () => {
    const user = userEvent.setup();
    mocks.searchText.mockResolvedValue({ items: [buildHit()], total: 50 });
    renderWithProviders(<SearchPage />);
    await user.type(
      screen.getByPlaceholderText("Describe what you want, or drop an image…"),
      "sunset{enter}",
    );
    expect(await screen.findByText("result-1.jpg")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "load-more" }));

    await waitFor(() =>
      expect(mocks.searchText).toHaveBeenLastCalledWith("sunset", expect.anything(), 2, 40),
    );
  });

  it("trashes selected search results and drops them from the list", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SearchPage />);
    await user.type(
      screen.getByPlaceholderText("Describe what you want, or drop an image…"),
      "sunset{enter}",
    );
    expect(await screen.findByText("result-1.jpg")).toBeInTheDocument();

    fireEvent.contextMenu(screen.getByText("result-1.jpg").closest('[class*="rounded-lg"]')!);
    const menu = await screen.findByRole("menu");
    await user.click(within(menu).getByText("Batch actions"));

    expect(screen.getByText("1 selected")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Move to Trash" }));

    await waitFor(() => expect(mocks.assetTrash).toHaveBeenCalledWith([1]));
    expect(mocks.toastSuccess).toHaveBeenCalledWith("Moved to Trash");
    await waitFor(() => expect(screen.queryByText("result-1.jpg")).not.toBeInTheDocument());
    expect(screen.getByText("No matching assets")).toBeInTheDocument();
  });
});
