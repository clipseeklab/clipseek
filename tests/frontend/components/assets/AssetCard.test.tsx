import { useAppStore } from "@/stores/app";
import { queryKeys } from "@/lib/queryKeys";
/** Asset card interactions: opening, selection, previews, video badges, fallback, and retry. */
import userEvent from "@testing-library/user-event";
import { useVideoFrameSearchStore } from "@/stores/videoFrameSearch";
import { act, fireEvent, screen, waitFor, within } from "@test/helpers/render";
import { renderWithProviders } from "@test/helpers/render";
import { buildAsset } from "@test/fixtures/assets";
import type { Asset } from "@/types";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  assetPreview: vi.fn(),
  assetPlaybackCodec: vi.fn(),
  importRetry: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  api: {
    assetPlaybackCodec: mocks.assetPlaybackCodec,
    assetPreview: mocks.assetPreview,
    importRetry: mocks.importRetry,
  },
}));
vi.mock("@/components/Toaster", () => ({ toast: { error: mocks.toastError } }));
// Stub the shared cross-card video player because its lifecycle is independent of card
// layout.
vi.mock("@/components/assets/HoverVideoPreview", () => ({ default: () => null }));

import AssetCard from "@/components/assets/AssetCard";
import { stubConvertFileSrc } from "@test/helpers/tauri";

function renderCard(asset: Asset, overrides: Partial<Parameters<typeof AssetCard>[0]> = {}) {
  const props = {
    asset,
    selected: false,
    selectMode: false,
    onSelect: vi.fn(),
    onOpen: vi.fn(),
    onPreview: vi.fn(),
    onAddTags: vi.fn(),
    onMove: vi.fn(),
    onBatch: vi.fn(),
    onTrash: vi.fn(),
    onSimilar: vi.fn(),
    ...overrides,
  };
  return { props, ...renderWithProviders(<AssetCard {...props} />) };
}

describe("AssetCard", () => {
  it("shows the folder and compact tags without changing card click behavior", async () => {
    const asset = buildAsset({ folderName: "Illustrations", tags: ["猫", "绿色", "角色", "参考"] });
    const { props } = renderCard(asset);
    expect(screen.getByTitle("Illustrations")).toBeVisible();
    const tags = screen.getByRole("list", { name: "Tags" });
    expect(within(tags).getAllByRole("listitem")).toHaveLength(3);
    expect(tags).toHaveAttribute("title", "猫, 绿色, 角色, 参考");
    expect(within(tags).getByText("猫")).toBeVisible();
    expect(within(tags).getByText("绿色")).toBeVisible();
    expect(within(tags).queryByText("角色")).not.toBeInTheDocument();
    expect(within(tags).queryByText("参考")).not.toBeInTheDocument();
    expect(within(tags).getByTitle("角色, 参考")).toHaveTextContent("+2");
    await userEvent.setup().click(screen.getByText("猫"));
    expect(props.onOpen).toHaveBeenCalledWith(asset);
  });

  it("shows Uncategorized without an empty tag row", () => {
    renderCard(buildAsset());
    expect(screen.getByTitle("Uncategorized")).toBeVisible();
    expect(screen.queryByRole("list", { name: "Tags" })).not.toBeInTheDocument();
  });

  it("shows independent indexing even after base import is done", () => {
    const asset: Asset = {
      ...buildAsset({ importStage: "done", visualIndexed: false }),
      processingTasks: [
        {
          id: 1,
          kind: "visual",
          status: "running",
          phase: "visual",
          error: null,
          attempt: 1,
          indexTaskId: 1,
        },
      ],
    };
    renderCard(asset);
    expect(screen.getByRole("status")).toHaveTextContent("Visual index");
    expect(screen.getByRole("status")).toHaveTextContent("Building");
  });
  it.each(["done", "error:timeout", "copied"])(
    "never loads an unthumbnailed video as a cover at stage %s",
    (importStage) => {
      const { container } = renderCard(
        buildAsset({
          kind: "video",
          mimeType: "video/x-matroska",
          fileName: "big.mkv",
          absPath: "C:/big.mkv",
          thumbPath: null,
          importStage,
        }),
      );
      expect(container.querySelector("img")).toBeNull();
      expect(mocks.assetPreview).not.toHaveBeenCalled();
    },
  );
  beforeEach(() => {
    useVideoFrameSearchStore.getState().close();
    vi.clearAllMocks();
    mocks.assetPlaybackCodec.mockResolvedValue("h264");
    stubConvertFileSrc();
    URL.createObjectURL = vi.fn(() => "blob:preview");
    URL.revokeObjectURL = vi.fn();
  });

  it("opens details on a plain click outside select mode", async () => {
    const user = userEvent.setup();
    const asset = buildAsset({ id: 4, fileName: "art.png" });
    const { props } = renderCard(asset);

    await user.click(screen.getByAltText("art.png"));

    expect(props.onOpen).toHaveBeenCalledWith(asset);
    expect(props.onSelect).not.toHaveBeenCalled();
  });

  it("selects instead of opening on meta-click or inside select mode", async () => {
    const user = userEvent.setup();
    const asset = buildAsset({ id: 4, fileName: "art.png" });
    const meta = renderCard(asset);
    // Dispatch browser events directly because user-event does not synthesize modified clicks
    // here.
    fireEvent.click(within(meta.container).getByAltText("art.png"), { metaKey: true });
    expect(meta.props.onSelect).toHaveBeenCalledWith(4, true);
    expect(meta.props.onOpen).not.toHaveBeenCalled();

    const selectMode = renderCard(asset, { selectMode: true });
    await user.click(within(selectMode.container).getByAltText("art.png"));
    expect(selectMode.props.onSelect).toHaveBeenCalledWith(4, true);
    expect(selectMode.props.onOpen).not.toHaveBeenCalled();
    // Selection checkboxes must not also trigger opening.
    await user.click(within(selectMode.container).getByRole("checkbox"));
    expect(selectMode.props.onSelect).toHaveBeenCalledTimes(2);
  });

  it("shows duration and format markers on video cards", () => {
    const asset = buildAsset({
      id: 5,
      fileName: "trailer.MP4",
      kind: "video",
      mimeType: "video/mp4",
      durationMs: 65_000,
    });
    renderCard(asset);

    expect(screen.getByText("1:05")).toBeInTheDocument();
    expect(screen.getByText("MP4")).toBeInTheDocument();
  });

  it("previews the original file without opening details", async () => {
    const user = userEvent.setup();
    const asset = buildAsset({ id: 6 });
    const { props } = renderCard(asset);

    await user.click(screen.getByTitle("Preview original"));

    expect(props.onPreview).toHaveBeenCalledWith(asset);
    expect(props.onOpen).not.toHaveBeenCalled();
  });

  it("falls back to a fetched preview when the thumbnail fails to load", async () => {
    mocks.assetPreview.mockResolvedValue([1, 2, 3]);
    const asset = buildAsset({ id: 7, thumbPath: "/library/.clipstudio/thumbs/missing.webp" });
    renderCard(asset);

    fireEvent.error(screen.getByAltText("photo-7.jpg"));

    await waitFor(() =>
      expect(screen.getByAltText("photo-7.jpg")).toHaveAttribute("src", "blob:preview"),
    );
    expect(mocks.assetPreview).toHaveBeenCalledWith(7);
  });

  it("recovers a thumbnail that was temporarily unavailable during indexing at the same path", async () => {
    mocks.assetPreview.mockRejectedValue(new Error("thumbnail is being regenerated"));
    const asset: Asset = buildAsset({
      id: 23,
      thumbPath: "/library/thumbs/photo.webp",
      visualIndexed: false,
      processingTasks: [
        {
          id: 1,
          kind: "visual",
          status: "running",
          phase: "visual",
          error: null,
          attempt: 1,
          indexTaskId: 1,
        },
      ],
    });
    const { props, rerender, container } = renderCard(asset);
    const original = screen.getByAltText("photo-23.jpg").getAttribute("src");
    fireEvent.error(screen.getByAltText("photo-23.jpg"));
    await waitFor(() => expect(mocks.assetPreview).toHaveBeenCalledTimes(1));
    expect(container.querySelector("img")).toBeNull();
    expect(mocks.toastError).not.toHaveBeenCalled();
    rerender(
      <AssetCard
        {...props}
        asset={{
          ...asset,
          visualIndexed: true,
          processingTasks: [{ ...asset.processingTasks![0], status: "completed" }],
        }}
      />,
    );
    const recovered = await screen.findByAltText("photo-23.jpg");
    expect(recovered.getAttribute("src")).not.toBe(original);
    expect(recovered.getAttribute("src")).toContain("photo.webp");
    expect(mocks.assetPreview).toHaveBeenCalledTimes(1);
  });

  it("does not let an old in-flight fallback overwrite the completed index thumbnail", async () => {
    let resolve!: (bytes: number[]) => void;
    mocks.assetPreview.mockReturnValue(
      new Promise<number[]>((done) => {
        resolve = done;
      }),
    );
    const asset: Asset = buildAsset({
      id: 24,
      thumbPath: "/library/thumbs/photo.webp",
      visualIndexed: false,
      processingTasks: [
        {
          id: 2,
          kind: "visual",
          status: "running",
          phase: "visual",
          error: null,
          attempt: 1,
          indexTaskId: 2,
        },
      ],
    });
    const { props, rerender } = renderCard(asset);
    fireEvent.error(screen.getByAltText("photo-24.jpg"));
    rerender(
      <AssetCard
        {...props}
        asset={{
          ...asset,
          visualIndexed: true,
          processingTasks: [{ ...asset.processingTasks![0], status: "completed" }],
        }}
      />,
    );
    const recovered = await screen.findByAltText("photo-24.jpg");
    const completedSource = recovered.getAttribute("src");
    await act(async () => {
      resolve([1, 2, 3]);
    });
    await waitFor(() =>
      expect(screen.getByAltText("photo-24.jpg")).toHaveAttribute("src", completedSource),
    );
    expect(URL.createObjectURL).not.toHaveBeenCalled();
  });

  it("shows a placeholder until a HEIC thumbnail is ready", async () => {
    const asset = buildAsset({
      id: 17,
      fileName: "photo.HEIC",
      mimeType: "image/heic",
      importStage: "thumb",
      thumbPath: null,
    });
    const { container, props, rerender } = renderCard(asset);

    expect(container.querySelector("img")).not.toBeInTheDocument();
    expect(screen.getByText("Thumbnailing")).toBeInTheDocument();
    expect(mocks.assetPreview).not.toHaveBeenCalled();

    rerender(
      <AssetCard
        {...props}
        asset={{ ...asset, importStage: "embedding", thumbPath: "/library/thumbs/photo.webp" }}
      />,
    );
    await waitFor(() =>
      expect(screen.getByAltText("photo.HEIC")).toHaveAttribute(
        "src",
        expect.stringContaining("photo.webp"),
      ),
    );
  });

  it("keeps a failed HEIF import free of a broken image", () => {
    const asset = buildAsset({
      id: 18,
      fileName: "photo.HEIF",
      mimeType: "image/heif",
      importStage: "error:decode",
      thumbPath: null,
    });
    const { container } = renderCard(asset);

    expect(container.querySelector("img")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Failed/ })).toBeInTheDocument();
    expect(mocks.assetPreview).not.toHaveBeenCalled();
  });

  it("hides a thumbnail when both its URL and fetched bytes fail to display", async () => {
    mocks.assetPreview.mockResolvedValue([1, 2, 3]);
    const asset = buildAsset({ id: 19, thumbPath: "/library/thumbs/photo.webp" });
    const { container } = renderCard(asset);

    fireEvent.error(screen.getByAltText("photo-19.jpg"));
    const fetched = await screen.findByAltText("photo-19.jpg");
    await waitFor(() => expect(fetched).toHaveAttribute("src", "blob:preview"));
    fireEvent.error(fetched);

    expect(container.querySelector("img")).not.toBeInTheDocument();
    expect(mocks.assetPreview).toHaveBeenCalledTimes(1);
  });

  it("reports a failed import and retries it on demand", async () => {
    const user = userEvent.setup();
    mocks.importRetry.mockResolvedValue(undefined);
    const asset = buildAsset({ id: 8, importStage: "error:thumb" });
    const { queryClient } = renderCard(asset);
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");

    const retry = await screen.findByRole("button", { name: /Failed/ });
    expect(retry).toHaveTextContent("Failed · Retry");

    await user.click(retry);
    await waitFor(() => expect(mocks.importRetry).toHaveBeenCalledWith([8]));
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.assets.library(useAppStore.getState().current?.id),
    });
  });

  it("keeps the failed import visible when a retry request fails", async () => {
    const user = userEvent.setup();
    mocks.importRetry.mockRejectedValue(new Error("backend busy"));
    renderCard(buildAsset({ id: 8, importStage: "error:embed" }));

    await user.click(await screen.findByRole("button", { name: /Failed/ }));

    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledOnce());
    expect(await screen.findByRole("button", { name: /Failed/ })).toBeInTheDocument();
  });
  it("shows a RAW badge while retaining the container format", async () => {
    mocks.assetPlaybackCodec.mockResolvedValue("prores_raw");
    renderCard(buildAsset({ kind: "video", fileName: "camera.mov" }));
    expect(await screen.findByText("ProRes RAW")).toBeInTheDocument();
    expect(screen.getByText("MOV")).toBeInTheDocument();
  });
});
