import { act } from "@testing-library/react";
import { queryKeys } from "@/lib/queryKeys";
/** Asset dialogs cover tags, folder moves, source previews, details, and legacy/reference
 * differences.
 */
import i18n from "@/i18n";
import userEvent from "@testing-library/user-event";
import { screen, waitFor, within } from "@test/helpers/render";
import { renderWithProviders } from "@test/helpers/render";
import { buildAsset } from "@test/fixtures/assets";
import { useAppStore } from "@/stores/app";
import type { AssetDetail, LibraryMeta } from "@/types";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent } from "@testing-library/react";

const mocks = vi.hoisted(() => ({
  isWindows: false,
  tagList: vi.fn(),
  assetAddTags: vi.fn(),
  assetRemoveTag: vi.fn(),
  folderList: vi.fn(),
  assetMoveToFolder: vi.fn(),
  assetMoveToLibrary: vi.fn(),
  assetDetail: vi.fn(),
  assetMetadata: vi.fn(),
  assetFullPreview: vi.fn(),
  assetSetDisplayName: vi.fn(),
  assetSetCover: vi.fn(),
  assetPlaybackSource: vi.fn(),
  assetPlaybackCodec: vi.fn(),
  assetPlaybackCompatibilityNeeded: vi.fn(),
  assetPlaybackCompatibilityPreflight: vi.fn(),
  assetPlaybackCompatibilityApply: vi.fn(),
  frontendError: vi.fn(),
  setFullscreen: vi.fn(),
  revealItemInDir: vi.fn(),
  openUrl: vi.fn(),
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
  saveDialog: vi.fn(),
  openDialog: vi.fn(),
  subtitleInfo: vi.fn(),
  subtitleCues: vi.fn(),
  subtitleImport: vi.fn(),
  subtitleRemove: vi.fn(),
  indexTasks: vi.fn(),
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
}));
vi.mock("@/lib/platform", () => ({
  get isWindows() {
    return mocks.isWindows;
  },
}));

vi.mock("@/lib/api", () => ({
  api: {
    tagList: mocks.tagList,
    assetAddTags: mocks.assetAddTags,
    assetRemoveTag: mocks.assetRemoveTag,
    folderPage: async (query: string, page: number) => {
      const all = (await mocks.folderList()).filter((f: { id: number; name: string }) =>
        f.name.toLowerCase().includes(query.toLowerCase()),
      );
      return {
        items: all.slice((page - 1) * 40, page * 40),
        total: all.length,
        page,
        hasMore: page * 40 < all.length,
      };
    },
    folderGet: async (id: number) =>
      (await mocks.folderList()).find((f: { id: number; name: string }) => f.id === id) ?? null,
    folderSidebar: async () => ({ items: [], total: (await mocks.folderList()).length }),
    assetMoveToFolder: mocks.assetMoveToFolder,
    assetMoveToLibrary: mocks.assetMoveToLibrary,
    assetDetail: mocks.assetDetail,
    assetMetadata: mocks.assetMetadata,
    assetFullPreview: mocks.assetFullPreview,
    assetSetDisplayName: mocks.assetSetDisplayName,
    assetSetCover: mocks.assetSetCover,
    assetPlaybackSource: mocks.assetPlaybackSource,
    assetPlaybackCodec: mocks.assetPlaybackCodec,
    assetPlaybackCompatibilityNeeded: mocks.assetPlaybackCompatibilityNeeded,
    assetPlaybackCompatibilityPreflight: mocks.assetPlaybackCompatibilityPreflight,
    assetPlaybackCompatibilityApply: mocks.assetPlaybackCompatibilityApply,
    frontendError: mocks.frontendError,
    subtitleInfo: mocks.subtitleInfo,
    subtitleCues: mocks.subtitleCues,
    subtitleImport: mocks.subtitleImport,
    subtitleRemove: mocks.subtitleRemove,
    indexTasks: mocks.indexTasks,
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: mocks.saveDialog, open: mocks.openDialog }));
vi.mock("@tauri-apps/api/path", () => ({ downloadDir: async () => "/exports" }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  revealItemInDir: mocks.revealItemInDir,
  openUrl: mocks.openUrl,
}));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ setFullscreen: mocks.setFullscreen }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, callback: (event: { payload: unknown }) => void) => {
    mocks.listeners.set(event, callback);
    return Promise.resolve(vi.fn());
  },
}));
vi.mock("@/components/Toaster", () => ({
  toast: { success: mocks.toastSuccess, error: mocks.toastError },
}));

import {
  AddTagsDialog,
  AssetDetailPanel,
  AssetSourcePreview,
  MoveToFolderDialog,
  MoveToLibraryDialog,
} from "@/components/assets/AssetDialogs";
import PlaybackVideo from "@/components/assets/PlaybackVideo";
import { ManagedAssetRevealDialog } from "@/components/assets/ManagedAssetReveal";
import { stubConvertFileSrc } from "@test/helpers/tauri";

beforeEach(() => {
  mocks.assetMetadata.mockResolvedValue({
    format: null,
    bitRate: null,
    createdAt: null,
    modifiedAt: null,
    tags: [],
    streams: [],
    exif: [],
  });
  mocks.openDialog.mockReset().mockResolvedValue("/exports");
  mocks.subtitleInfo.mockResolvedValue(null);
  mocks.subtitleCues.mockResolvedValue([]);
  mocks.indexTasks.mockResolvedValue([]);
  mocks.isWindows = false;
  stubConvertFileSrc();
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  mocks.frontendError.mockResolvedValue(undefined);
  mocks.assetPlaybackSource.mockResolvedValue("http://127.0.0.1:43123/files/video.mp4");
  mocks.assetPlaybackCodec.mockResolvedValue("h264");
  mocks.assetPlaybackCompatibilityNeeded.mockResolvedValue(false);
  mocks.assetPlaybackCompatibilityPreflight.mockResolvedValue({
    sourceBytes: 2_000_000_000,
    estimatedTemporaryBytes: 2_000_000_000,
    availableBytes: 10_000_000_000,
    canCreate: true,
    modifiesExternalSource: false,
  });
  mocks.assetPlaybackCompatibilityApply.mockResolvedValue("http://127.0.0.1:43123/files/video.mp4");
});

const folders = [
  { id: 7, name: "Posters", createdAt: 1, assetCount: 3 },
  { id: 8, name: "Sketches", createdAt: 2, assetCount: 0 },
];

function buildDetail(overrides: Partial<AssetDetail> = {}): AssetDetail {
  return {
    ...buildAsset({
      id: 9,
      fileName: "holiday.jpg",
      absPath: "/library/images/holiday.jpg",
      thumbPath: "/library/.clipstudio/thumbs/holiday.webp",
      width: 1920,
      height: 1080,
      fileSize: 2048,
      importedAt: 1_700_000_000,
      tags: ["travel"],
    }),
    frames: [
      {
        id: 1,
        uid: "frame-1",
        seq: 0,
        timestampMs: 0,
        absPath: "/library/.clipstudio/frames/f1.jpg",
      },
      { id: 2, uid: "frame-2", seq: 1, timestampMs: 30_000, absPath: null },
    ],
    ...overrides,
  };
}

describe("AddTagsDialog", () => {
  const assets = [buildAsset({ id: 1 }), buildAsset({ id: 2, fileName: "photo-2.jpg" })];

  beforeEach(() => {
    mocks.tagList.mockResolvedValue([]);
    mocks.assetAddTags.mockResolvedValue(undefined);
  });

  it("shows existing tags and removes one without closing the dialog", async () => {
    const user = userEvent.setup();
    mocks.assetRemoveTag.mockResolvedValue(undefined);
    const onOpenChange = vi.fn();
    const { queryClient } = renderWithProviders(
      <AddTagsDialog
        assets={[buildAsset({ tags: ["hello", "portrait"] })]}
        open
        onOpenChange={onOpenChange}
      />,
    );
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");
    const existing = screen.getByRole("region", { name: "Existing tags" });
    expect(existing).toHaveTextContent("hello");
    expect(existing).toHaveTextContent("portrait");
    await user.click(within(existing).getByRole("button", { name: "Delete hello" }));
    await waitFor(() =>
      expect(
        within(existing).queryByRole("button", { name: "Delete hello" }),
      ).not.toBeInTheDocument(),
    );
    expect(mocks.assetRemoveTag).toHaveBeenCalledExactlyOnceWith(1, "hello");
    expect(existing).toHaveTextContent("portrait");
    expect(onOpenChange).not.toHaveBeenCalled();
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.assets.library(useAppStore.getState().current?.id),
    });
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.tags.library(useAppStore.getState().current?.id),
    });
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.assetDetail.item(useAppStore.getState().current?.id, 1),
    });
  });

  it("keeps an existing tag visible when removal fails", async () => {
    const user = userEvent.setup();
    mocks.assetRemoveTag.mockRejectedValue(new Error("db locked"));
    renderWithProviders(
      <AddTagsDialog assets={[buildAsset({ tags: ["hello"] })]} open onOpenChange={vi.fn()} />,
    );
    await user.click(screen.getByRole("button", { name: "Delete hello" }));
    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledOnce());
    expect(screen.getByRole("button", { name: "Delete hello" })).toBeEnabled();
  });

  it("removes a tag only from selected assets that have it", async () => {
    const user = userEvent.setup();
    mocks.assetRemoveTag.mockResolvedValue(undefined);
    renderWithProviders(
      <AddTagsDialog
        assets={[buildAsset({ id: 1, tags: ["hello"] }), buildAsset({ id: 2, tags: ["other"] })]}
        open
        onOpenChange={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Delete hello" }));
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Delete hello" })).not.toBeInTheDocument(),
    );
    expect(mocks.assetRemoveTag).toHaveBeenCalledExactlyOnceWith(1, "hello");
    expect(screen.getByRole("button", { name: "Delete other" })).toBeVisible();
  });

  it("adds queued tags to every asset, refreshes queries and closes", async () => {
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    const { queryClient } = renderWithProviders(
      <AddTagsDialog assets={assets} open onOpenChange={onOpenChange} />,
    );
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");

    await user.type(screen.getByRole("combobox", { name: "Tags" }), "design{enter}");
    await user.type(screen.getByRole("combobox", { name: "Tags" }), "poster{enter}");
    await user.click(screen.getByRole("button", { name: "Add Tags" }));

    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(mocks.assetAddTags).toHaveBeenCalledWith(1, ["design", "poster"]);
    expect(mocks.assetAddTags).toHaveBeenCalledWith(2, ["design", "poster"]);
    expect(mocks.toastSuccess).toHaveBeenCalledWith("Done");
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.assets.library(useAppStore.getState().current?.id),
    });
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.tags.library(useAppStore.getState().current?.id),
    });
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.assetDetail.item(useAppStore.getState().current?.id, 1),
    });
  });

  it("keeps the dialog open and reports the error when adding fails", async () => {
    const user = userEvent.setup();
    mocks.assetAddTags.mockRejectedValue(new Error("db locked"));
    const onOpenChange = vi.fn();
    renderWithProviders(<AddTagsDialog assets={assets} open onOpenChange={onOpenChange} />);

    await user.type(screen.getByRole("combobox", { name: "Tags" }), "design{enter}");
    await user.click(screen.getByRole("button", { name: "Add Tags" }));

    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledOnce());
    expect(onOpenChange).not.toHaveBeenCalled();
    // Multiple selection appears in the subtitle while the dialog remains open.
    expect(screen.getByRole("dialog")).toHaveTextContent("2 selected");
  });
});

describe("MoveToFolderDialog", () => {
  const assets = [buildAsset({ id: 1 }), buildAsset({ id: 2, fileName: "photo-2.jpg" })];

  beforeEach(() => {
    mocks.folderList.mockResolvedValue(folders);
    mocks.assetMoveToFolder.mockResolvedValue(undefined);
  });

  it("moves all assets into the chosen folder and refreshes queries", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const onOpenChange = vi.fn();
    const { queryClient } = renderWithProviders(
      <MoveToFolderDialog assets={assets} open onOpenChange={onOpenChange} />,
    );
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");

    await user.click(screen.getByRole("button", { name: "Folder" }));
    await user.click(await screen.findByRole("option", { name: "Posters" }));
    await user.click(screen.getByRole("button", { name: "Confirm" }));

    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(mocks.assetMoveToFolder).toHaveBeenCalledWith([1, 2], 7);
    expect(mocks.toastSuccess).toHaveBeenCalledWith("Done");
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.assets.library(useAppStore.getState().current?.id),
    });
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.folders.library(useAppStore.getState().current?.id),
    });
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.assetDetail.item(useAppStore.getState().current?.id, 1),
    });
  });

  it("preserves a chosen destination when the parent recreates the asset array", async () => {
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    const view = renderWithProviders(
      <MoveToFolderDialog assets={[...assets]} open onOpenChange={onOpenChange} />,
    );
    await user.click(screen.getByRole("button", { name: "Folder" }));
    await user.click(await screen.findByRole("option", { name: "Posters" }));
    view.rerender(<MoveToFolderDialog assets={[...assets]} open onOpenChange={onOpenChange} />);
    await user.click(screen.getByRole("button", { name: "Folder" }));
    expect(screen.getByRole("option", { name: "Posters" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await user.click(screen.getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(mocks.assetMoveToFolder).toHaveBeenCalledWith([1, 2], 7));
  });

  it("defaults to Uncategorized and sends a null folder", async () => {
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    renderWithProviders(<MoveToFolderDialog assets={assets} open onOpenChange={onOpenChange} />);

    await user.click(screen.getByRole("button", { name: "Confirm" }));

    await waitFor(() => expect(mocks.assetMoveToFolder).toHaveBeenCalledWith([1, 2], null));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("keeps the dialog open when the move fails", async () => {
    const user = userEvent.setup();
    mocks.assetMoveToFolder.mockRejectedValue(new Error("disk full"));
    const onOpenChange = vi.fn();
    renderWithProviders(<MoveToFolderDialog assets={assets} open onOpenChange={onOpenChange} />);

    await user.click(screen.getByRole("button", { name: "Confirm" }));

    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledOnce());
    expect(onOpenChange).not.toHaveBeenCalled();
  });
});

describe("AssetSourcePreview", () => {
  beforeEach(() => {
    mocks.setFullscreen.mockResolvedValue(undefined);
  });

  it("renders an image with zoom controls that clamp and reset", async () => {
    const user = userEvent.setup();
    const asset = buildAsset({ id: 5, fileName: "art.png", mimeType: "image/png" });
    renderWithProviders(<AssetSourcePreview asset={asset} open onOpenChange={vi.fn()} />);

    const image = screen.getByAltText("art.png");
    expect(image).toHaveAttribute("src", "asset://localhost//library/images/photo-5.jpg");
    expect(screen.getByText("100%")).toBeInTheDocument();

    await user.click(screen.getByTitle("Zoom in"));
    expect(screen.getByText("125%")).toBeInTheDocument();

    // Double-click toggles between zoomed and original size.
    await user.dblClick(image);
    expect(screen.getByText("100%")).toBeInTheDocument();

    await user.click(screen.getByTitle("Reset view"));
    expect(screen.getByText("100%")).toBeInTheDocument();
  });

  it("browses the current list with buttons and arrow keys, resets zoom, and respects boundaries", async () => {
    const user = userEvent.setup();
    const assets = [
      buildAsset({ id: 31, fileName: "first.png" }),
      buildAsset({ id: 32, fileName: "second.png" }),
      buildAsset({ id: 33, fileName: "last.png" }),
    ];
    renderWithProviders(
      <AssetSourcePreview asset={assets[1]} assets={assets} open onOpenChange={vi.fn()} />,
    );

    expect(screen.getByText("2 / 3")).toBeInTheDocument();
    await user.click(screen.getByTitle("Zoom in"));
    await user.click(screen.getByRole("button", { name: "Next asset" }));
    expect(screen.getByAltText("last.png")).toBeInTheDocument();
    expect(screen.getByText("100%")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Next asset" })).toBeDisabled();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByAltText("last.png")).toBeInTheDocument();
    await user.keyboard("{ArrowLeft}{ArrowLeft}");
    expect(screen.getByAltText("first.png")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Previous asset" })).toBeDisabled();
    expect(screen.getByText("1 / 3")).toBeInTheDocument();
  });

  it("omits navigation for a standalone preview and starts from the original asset after reopening", async () => {
    const user = userEvent.setup();
    const assets = [
      buildAsset({ id: 31, fileName: "first.png" }),
      buildAsset({ id: 32, fileName: "second.png" }),
    ];
    const { rerender } = renderWithProviders(
      <AssetSourcePreview asset={assets[0]} assets={assets} open onOpenChange={vi.fn()} />,
    );
    await user.click(screen.getByRole("button", { name: "Next asset" }));
    rerender(
      <AssetSourcePreview asset={assets[0]} assets={assets} open={false} onOpenChange={vi.fn()} />,
    );
    rerender(<AssetSourcePreview asset={assets[0]} assets={assets} open onOpenChange={vi.fn()} />);
    expect(screen.getByAltText("first.png")).toBeInTheDocument();
    rerender(<AssetSourcePreview asset={assets[0]} open onOpenChange={vi.fn()} />);
    expect(screen.queryByRole("button", { name: "Next asset" })).not.toBeInTheDocument();
  });

  it("keeps video arrow keys for playback and starts a different video from zero", async () => {
    const user = userEvent.setup();
    vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue(undefined);
    const assets = [
      buildAsset({ id: 31, fileName: "first.mp4", kind: "video" }),
      buildAsset({ id: 32, fileName: "second.mp4", kind: "video" }),
    ];
    renderWithProviders(
      <AssetSourcePreview
        asset={assets[0]}
        assets={assets}
        initialTimeMs={12000}
        open
        onOpenChange={vi.fn()}
      />,
    );
    const first = document.querySelector("video")!;
    fireEvent.loadedMetadata(first);
    expect(first.currentTime).toBe(12);
    await user.click(screen.getByRole("button", { name: "Next asset" }));
    const second = document.querySelector("video")!;
    expect(second).not.toBe(first);
    fireEvent.loadedMetadata(second);
    expect(second.currentTime).toBe(0);
    fireEvent.keyDown(second, { key: "ArrowLeft" });
    expect(screen.getByRole("heading", { name: "second.mp4" })).toBeInTheDocument();
    expect(screen.getByText("2 / 2")).toBeInTheDocument();
  });

  it("renders a video player with its poster and toggles window fullscreen", async () => {
    const user = userEvent.setup();
    const asset = buildAsset({
      id: 6,
      fileName: "trailer.mp4",
      kind: "video",
      mimeType: "video/mp4",
      thumbPath: "/library/.clipstudio/thumbs/trailer.webp",
    });
    mocks.assetPlaybackSource.mockResolvedValue("http://127.0.0.1:43123/files/trailer.mp4");
    renderWithProviders(<AssetSourcePreview asset={asset} open onOpenChange={vi.fn()} />);

    const video = document.querySelector("video");
    expect(video).not.toBeNull();
    expect(video).toHaveAttribute(
      "poster",
      "asset://localhost//library/.clipstudio/thumbs/trailer.webp",
    );

    await user.click(screen.getByRole("button", { name: "Fullscreen" }));
    await waitFor(() => expect(mocks.setFullscreen).toHaveBeenCalledWith(true));
    expect(screen.getByRole("status")).toHaveTextContent("Press Esc to exit fullscreen");
    expect(screen.queryByRole("button", { name: "Close" })).not.toBeInTheDocument();
  });

  it("expands the video to the viewport without reserving header or content padding, then restores preview", async () => {
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    renderWithProviders(
      <AssetSourcePreview
        asset={buildAsset({ id: 88, kind: "video" })}
        open
        onOpenChange={onOpenChange}
      />,
    );
    await waitFor(() =>
      expect(document.querySelector("video")).toHaveAttribute(
        "src",
        expect.stringContaining("video.mp4"),
      ),
    );
    const video = document.querySelector("video")!;
    await user.click(screen.getByRole("button", { name: "Fullscreen" }));
    await waitFor(() => expect(mocks.setFullscreen).toHaveBeenLastCalledWith(true));
    expect(screen.getByRole("status")).toHaveTextContent("Press Esc to exit fullscreen");
    expect(video).toHaveClass("w-full", "h-full", "object-contain");
    expect(video.parentElement).not.toHaveClass("pb-20", "sm:px-20");
    expect(screen.getByRole("heading").parentElement).toHaveClass("sr-only");
    expect(screen.getByRole("dialog")).toHaveClass("bg-black");
    expect(screen.queryByRole("button", { name: "Exit fullscreen" })).not.toBeInTheDocument();
    expect(screen.getByRole("dialog")).toHaveClass("z-[80]");
    await user.keyboard("{Escape}");
    expect(onOpenChange).not.toHaveBeenCalled();
    await screen.findByRole("button", { name: "Fullscreen" });
    expect(mocks.setFullscreen).toHaveBeenLastCalledWith(false);
    expect(document.querySelector("video")).toBe(video);
    expect(video).not.toHaveClass("w-full", "h-full");
    expect(video.parentElement).toHaveClass("pb-20", "sm:px-20");
    expect(screen.getByRole("heading").parentElement).not.toHaveClass("sr-only");
    expect(screen.queryByText("Press Esc to exit fullscreen")).not.toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("loads a full-resolution HEIC preview when the WebView cannot display the source", async () => {
    const asset = buildAsset({
      id: 7,
      fileName: "IMG_1452.HEIC",
      absPath: "/library/images/IMG_1452.HEIC",
      thumbPath: "/library/.clipstudio/thumbs/IMG_1452.webp",
      mimeType: "image/heic",
    });
    const createObjectURL = vi.fn(() => "blob:full-heic");
    URL.createObjectURL = createObjectURL;
    const revokeObjectURL = vi.fn();
    URL.revokeObjectURL = revokeObjectURL;
    mocks.assetFullPreview.mockResolvedValue(new Uint8Array([137, 80, 78, 71]));
    const { unmount } = renderWithProviders(
      <AssetSourcePreview asset={asset} open onOpenChange={vi.fn()} />,
    );

    const image = screen.getByAltText("IMG_1452.HEIC");
    fireEvent.error(image);

    await waitFor(() => expect(mocks.assetFullPreview).toHaveBeenCalledWith(7));
    await waitFor(() => expect(image).toHaveAttribute("src", "blob:full-heic"));
    expect(createObjectURL).toHaveBeenCalledOnce();
    unmount();
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:full-heic");
  });

  it("retains the thumbnail when full-size HEIC decoding fails", async () => {
    const asset = buildAsset({
      id: 8,
      fileName: "broken.heif",
      absPath: "/library/images/broken.heif",
      thumbPath: "/library/.clipstudio/thumbs/broken.webp",
      mimeType: "image/heif",
    });
    mocks.assetFullPreview.mockRejectedValue(new Error("decode failed"));
    renderWithProviders(<AssetSourcePreview asset={asset} open onOpenChange={vi.fn()} />);

    const image = screen.getByAltText("broken.heif");
    fireEvent.error(image);

    await waitFor(() => expect(mocks.assetFullPreview).toHaveBeenCalledWith(8));
    expect(image).toHaveAttribute(
      "src",
      "asset://localhost//library/.clipstudio/thumbs/broken.webp",
    );
  });

  it("uses the local HTTP media URL without converting it back to asset protocol", async () => {
    mocks.assetPlaybackSource.mockResolvedValue(
      "http://127.0.0.1:43123/files/video/sample.mp4?token=secret",
    );
    renderWithProviders(<PlaybackVideo assetId={107} sourcePath="/external/sample.mp4" />);

    await waitFor(() =>
      expect(document.querySelector("video")?.getAttribute("src")).toMatch(
        "http://127.0.0.1:43123/files/video/sample.mp4?token=secret",
      ),
    );
  });
});

describe("AssetDetailPanel", () => {
  it("shows the image index state on its preview without a visual-index prefix", async () => {
    mocks.assetDetail.mockResolvedValue(buildDetail({ visualIndexed: true }));
    renderPanel();
    const preview = await screen.findByRole("button", { name: "Enlarge preview" });
    expect(within(preview).getByText("Indexed")).toBeVisible();
    expect(screen.queryByText(/Visual index/)).not.toBeInTheDocument();
  });

  it("opens the original image by clicking the detail preview and closes back to details", async () => {
    const user = userEvent.setup();
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Enlarge preview" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("img", { name: "holiday.jpg" })).toHaveAttribute(
      "src",
      "asset://localhost//library/images/holiday.jpg",
    );
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Enlarge preview" })).toBeVisible();
  });

  it("opens video in the large viewer while retaining its playback controls and cover action", async () => {
    const user = userEvent.setup();
    mocks.assetDetail.mockResolvedValue(buildDetail({ kind: "video", fileName: "video.mp4" }));
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Enlarge preview" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog.querySelector("video")).toHaveAttribute("controls");
    expect(within(dialog).getByRole("button", { name: "Fullscreen" })).toBeVisible();
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Replace cover" })).toBeVisible();
  });

  it("replaces a video's poster immediately and refreshes the asset list", async () => {
    const detail = buildDetail({ kind: "video", fileName: "video.mp4" });
    const updated = { ...detail, thumbPath: detail.frames[0].absPath };
    mocks.assetDetail.mockResolvedValue(detail);
    mocks.assetSetCover.mockResolvedValue(updated);
    const { container, queryClient } = renderPanel();
    const currentCache = queryKeys.assets.library(useAppStore.getState().current?.id);
    const otherCache = queryKeys.assets.library("other-library");
    queryClient.setQueryData(currentCache, { items: [detail] });
    queryClient.setQueryData(otherCache, { items: [detail] });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Replace cover" }));
    expect(screen.getAllByRole("radio")).toHaveLength(2);
    await user.click(screen.getByRole("button", { name: "Use as cover" }));
    await waitFor(() =>
      expect(container.querySelector("video")).toHaveAttribute(
        "poster",
        `asset://localhost/${detail.frames[0].absPath}`,
      ),
    );
    expect(mocks.assetSetCover).toHaveBeenCalledExactlyOnceWith(9, 1);
    expect(queryClient.getQueryState(currentCache)?.isInvalidated).toBe(true);
    expect(queryClient.getQueryState(otherCache)?.isInvalidated).toBe(false);
    expect(
      queryClient.getQueryData(queryKeys.assetDetail.item(useAppStore.getState().current?.id, 9)),
    ).toMatchObject({
      thumbPath: detail.frames[0].absPath,
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("does not offer video cover replacement for images", async () => {
    renderPanel();
    await screen.findAllByText("holiday.jpg");
    expect(screen.queryByRole("button", { name: "Replace cover" })).not.toBeInTheDocument();
  });

  it("does not make the Windows detail title a native window drag region", async () => {
    mocks.isWindows = true;
    const { container } = renderPanel();
    await screen.findAllByText("holiday.jpg");
    expect(container.querySelector("[data-tauri-drag-region]")).toBeNull();
  });
  beforeEach(() => {
    mocks.assetDetail.mockResolvedValue(buildDetail());
    mocks.assetRemoveTag.mockResolvedValue(undefined);
    mocks.assetSetDisplayName.mockResolvedValue(undefined);
    mocks.revealItemInDir.mockResolvedValue(undefined);
  });

  function renderPanel(overrides: Partial<Parameters<typeof AssetDetailPanel>[0]> = {}) {
    const props = {
      assetId: 9,
      open: true,
      onOpenChange: vi.fn(),
      onTrash: vi.fn(),
      ...overrides,
    };
    return {
      props,
      ...renderWithProviders(
        <>
          <AssetDetailPanel {...props} />
          <ManagedAssetRevealDialog />
        </>,
      ),
    };
  }

  it("marks reference assets and flags missing source files", async () => {
    mocks.assetDetail.mockResolvedValue(
      buildDetail({ storage: "reference", sourcePath: "/Users/me/art/holiday.jpg" }),
    );
    renderPanel();
    expect(await screen.findByText("Linked")).toBeInTheDocument();

    mocks.assetDetail.mockResolvedValue(
      buildDetail({
        storage: "reference",
        sourcePath: "/Users/me/art/holiday.jpg",
        sourceMissing: true,
      }),
    );
    renderPanel();
    expect(await screen.findByText("Source missing")).toBeInTheDocument();
  });

  it("shows a browser capture source and opens its page", async () => {
    const user = userEvent.setup();
    mocks.assetDetail.mockResolvedValue(
      buildDetail({
        webSourceUrl: "https://example.com/posts/42",
        webSourceTitle: "Example post",
      }),
    );
    renderPanel();

    await user.click(await screen.findByRole("button", { name: "example.com" }));
    expect(screen.getByText("Source page")).toBeInTheDocument();
    expect(mocks.openUrl).toHaveBeenCalledWith("https://example.com/posts/42");
  });

  it("edits the display title without changing the file name", async () => {
    const user = userEvent.setup();
    mocks.assetDetail.mockResolvedValue(buildDetail({ displayName: "Original page title" }));
    renderPanel();

    await user.click(await screen.findByTitle("Edit title"));
    const input = screen.getByRole("textbox", { name: "Edit title" });
    await user.clear(input);
    await user.type(input, "Renamed title{Enter}");

    await waitFor(() => expect(mocks.assetSetDisplayName).toHaveBeenCalledWith(9, "Renamed title"));
    expect(screen.getAllByText("holiday.jpg").length).toBeGreaterThan(0);
  });

  it("removes a tag through the API and refreshes the detail query", async () => {
    const user = userEvent.setup();
    const { queryClient } = renderPanel();
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");

    await user.click(await screen.findByText("travel ×"));

    await waitFor(() => expect(mocks.assetRemoveTag).toHaveBeenCalledWith(9, "travel"));
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.assetDetail.item(useAppStore.getState().current?.id, 9),
    });
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.assets.library(useAppStore.getState().current?.id),
    });
  });

  it("warns before revealing a copy asset but reveals a reference source directly", async () => {
    const user = userEvent.setup();
    const view = renderPanel();
    await screen.findAllByText("holiday.jpg");

    await user.click(screen.getByRole("button", { name: "Show in file manager" }));
    expect(await screen.findByRole("alertdialog")).toHaveTextContent("Do not modify library files");
    expect(mocks.revealItemInDir).not.toHaveBeenCalled();

    await user.click(
      within(screen.getByRole("alertdialog")).getByRole("button", { name: "Show in file manager" }),
    );
    expect(mocks.revealItemInDir).toHaveBeenCalledWith("/library/images/holiday.jpg");

    const reference = buildDetail({
      storage: "reference",
      sourcePath: "/Users/me/art/holiday.jpg",
      absPath: "/Users/me/art/holiday.jpg",
    });
    mocks.assetDetail.mockResolvedValue(reference);
    view.unmount();
    renderPanel();
    await screen.findAllByText("holiday.jpg");

    await user.click(screen.getByRole("button", { name: "Show in file manager" }));
    await waitFor(() =>
      expect(mocks.revealItemInDir).toHaveBeenCalledWith("/Users/me/art/holiday.jpg"),
    );
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it("trashes the asset and closes the panel", async () => {
    const user = userEvent.setup();
    const { props } = renderPanel();
    await screen.findAllByText("holiday.jpg");

    // The bottom delete control is an icon-only button.
    const unnamedButtons = screen.getAllByRole("button", { name: "Move to Trash" });
    const trashButton = unnamedButtons[unnamedButtons.length - 1];
    expect(trashButton).toBeDefined();
    await user.click(trashButton);

    await waitFor(() => expect(props.onTrash).toHaveBeenCalledTimes(1));
    expect(vi.mocked(props.onTrash).mock.calls[0][0]).toMatchObject({
      id: 9,
      fileName: "holiday.jpg",
    });
    expect(props.onOpenChange).toHaveBeenCalledWith(false);
  });
});

describe("MoveToLibraryDialog", () => {
  const libraries: LibraryMeta[] = [
    { id: "library-1", name: "Current", path: "/Lib1", createdAt: 1 },
    { id: "library-2", name: "Archive", path: "/Lib2", createdAt: 2 },
  ];

  beforeEach(() => {
    useAppStore.setState(
      { ...useAppStore.getInitialState(), libraries, current: libraries[0] },
      true,
    );
    mocks.assetMoveToLibrary.mockResolvedValue(undefined);
  });

  it("offers only other libraries and moves the selection on confirm", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const onOpenChange = vi.fn();
    const assets = [buildAsset({ id: 1 }), buildAsset({ id: 2 })];
    renderWithProviders(<MoveToLibraryDialog assets={assets} open onOpenChange={onOpenChange} />);

    await user.click(await screen.findByRole("combobox"));
    const options = await screen.findAllByRole("option");
    expect(options.map((option) => option.textContent)).toEqual(["Archive"]);

    await user.click(screen.getByRole("option", { name: "Archive" }));
    await user.click(screen.getByRole("button", { name: "Confirm" }));

    await waitFor(() => expect(mocks.assetMoveToLibrary).toHaveBeenCalledWith([1, 2], "library-2"));
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(mocks.toastSuccess).toHaveBeenCalledWith("Done");
  });

  it("requires a target and shows a retryable error on failure", async () => {
    const user = userEvent.setup();
    mocks.assetMoveToLibrary.mockRejectedValue(new Error("target library busy"));
    renderWithProviders(
      <MoveToLibraryDialog assets={[buildAsset()]} open onOpenChange={vi.fn()} />,
    );
    await screen.findByRole("combobox");

    expect(screen.getByRole("button", { name: "Confirm" })).toBeDisabled();

    await user.click(screen.getByRole("combobox"));
    await user.click(await screen.findByRole("option", { name: "Archive" }));
    await user.click(screen.getByRole("button", { name: "Confirm" }));

    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledOnce());
    expect(mocks.toastError.mock.calls[0][0]).toMatch(/retry/i);
  });
});

it("shows a retryable asset detail error instead of spinning forever", async () => {
  await i18n.changeLanguage("en");
  mocks.assetDetail
    .mockRejectedValueOnce(new Error("database unavailable"))
    .mockResolvedValue({ ...buildAsset({ id: 401, fileName: "recovered.jpg" }), frames: [] });
  renderWithProviders(<AssetDetailPanel assetId={401} open onOpenChange={vi.fn()} />);
  expect(await screen.findByRole("alert")).toHaveTextContent("database unavailable");
  await userEvent.setup().click(screen.getByRole("button", { name: /^Retry$/ }));
  expect(await screen.findByText("recovered.jpg", { selector: "div" })).toBeInTheDocument();
});

it("replaces detail data when another library contains the same numeric asset id", async () => {
  const previous = useAppStore.getState().current;
  const library = (id: string) => ({ id, name: id, path: `/${id}`, createdAt: 1 });
  mocks.assetDetail.mockImplementation(async () =>
    buildDetail({
      displayName:
        useAppStore.getState().current?.id === "detail-a" ? "Library A title" : "Library B title",
    }),
  );
  act(() => useAppStore.getState().setCurrent(library("detail-a")));
  const view = renderWithProviders(<AssetDetailPanel assetId={9} open onOpenChange={vi.fn()} />);
  try {
    expect(await screen.findByText("Library A title")).toBeVisible();
    act(() => useAppStore.getState().setCurrent(library("detail-b")));
    expect(screen.queryByText("Library A title")).not.toBeInTheDocument();
    expect(await screen.findByText("Library B title")).toBeVisible();
  } finally {
    view.unmount();
    act(() => useAppStore.getState().setCurrent(previous));
  }
});
