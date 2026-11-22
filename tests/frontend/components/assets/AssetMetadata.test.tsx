import { beforeEach, expect, it, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import {
  act,
  createTestQueryClient,
  renderWithProviders,
  screen,
  waitFor,
  within,
} from "@test/helpers/render";
import { buildAsset } from "@test/fixtures/assets";
import AssetMetadata from "@/components/assets/AssetMetadata";
import { useAppStore } from "@/stores/app";
import type { AssetDetail } from "@/types";
import type { AssetMediaMetadata, MediaStreamMetadata } from "@/lib/mediaMetadata";
const mocks = vi.hoisted(() => ({ metadata: vi.fn() }));
vi.mock("@/lib/api", () => ({ api: { assetMetadata: mocks.metadata } }));

const stream: MediaStreamMetadata = {
  index: 0,
  kind: "video",
  codec: "h264",
  profile: "High",
  language: null,
  width: 1920,
  height: 1080,
  frameRate: 29.97,
  bitRate: 1_500_000,
  pixelFormat: "yuv420p(tv, bt709)",
  sampleRate: null,
  channelLayout: null,
  sampleFormat: null,
  description: "h264 (High), yuv420p, 1920x1080, 29.97 fps",
  tags: [],
};
const data: AssetMediaMetadata = {
  exif: [],
  format: "mov,mp4",
  bitRate: 1_700_000,
  createdAt: 1_700_000_000,
  modifiedAt: 1_700_001_000,
  tags: [{ name: "title", value: "示例视频" }],
  streams: [
    stream,
    {
      ...stream,
      index: 1,
      kind: "audio",
      codec: "aac",
      profile: "LC",
      width: null,
      height: null,
      frameRate: null,
      pixelFormat: null,
      bitRate: 128000,
      language: "eng",
      sampleRate: 48000,
      channelLayout: "stereo",
      sampleFormat: "fltp",
    },
  ],
};
function detail(overrides: Partial<AssetDetail> = {}): AssetDetail {
  return {
    ...buildAsset({ id: 7, kind: "video", mimeType: "video/mp4" }),
    frames: [],
    ...overrides,
  };
}
beforeEach(() => {
  vi.resetAllMocks();
  useAppStore.setState(useAppStore.getInitialState(), true);
  useAppStore.setState({ current: { id: "a", name: "A", path: "/a", createdAt: 1 } });
  mocks.metadata.mockResolvedValue(data);
});

it("keeps basic details visible while loading metadata, then groups technical fields and embedded tags", async () => {
  let finish: ((value: AssetMediaMetadata) => void) | undefined;
  mocks.metadata.mockReturnValueOnce(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  renderWithProviders(<AssetMetadata detail={detail()} err={null} />);
  expect(screen.getByText("video/mp4")).toBeVisible();
  expect(screen.getByRole("status")).toBeVisible();
  await act(async () => finish?.(data));
  const video = (await screen.findByRole("heading", { name: "Video stream #0" })).closest(
    "section",
  )!;
  expect(within(video).getByText("h264")).toBeVisible();
  expect(within(video).getByText("29.97 fps")).toBeVisible();
  expect(within(video).getByText("1.5 Mb/s")).toBeVisible();
  const audio = screen.getByRole("heading", { name: "Audio stream #1" }).closest("section")!;
  expect(within(audio).getByText("48 kHz")).toBeVisible();
  expect(within(audio).getByText("stereo")).toBeVisible();
  expect(screen.getByText("File modified")).toBeVisible();
  await userEvent.setup().click(screen.getByText("Embedded metadata (1)"));
  expect(screen.getByText("示例视频")).toBeVisible();
});

it("does not present the still-image demuxer's inferred frame rate as image metadata", async () => {
  renderWithProviders(<AssetMetadata detail={detail({ kind: "image" })} err={null} />);
  expect(await screen.findByRole("heading", { name: "Image" })).toBeVisible();
  expect(screen.queryByText("29.97 fps")).not.toBeInTheDocument();
});

it.each([{ sourceMissing: true }, { importStage: "copied" }])(
  "does not probe unavailable or unfinished source files: %j",
  async (overrides) => {
    renderWithProviders(<AssetMetadata detail={detail(overrides)} err={null} />);
    await act(async () => {});
    expect(mocks.metadata).not.toHaveBeenCalled();
    expect(screen.queryByRole("region", { name: "Media metadata" })).not.toBeInTheDocument();
  },
);

it("shows a recoverable metadata error while preserving the basic asset information", async () => {
  mocks.metadata.mockRejectedValueOnce({ code: "asset_metadata_failed" });
  renderWithProviders(<AssetMetadata detail={detail()} err={null} />);
  expect(await screen.findByRole("alert")).toHaveTextContent("Unable to read media metadata");
  expect(screen.getByText("video/mp4")).toBeVisible();
  await userEvent.setup().click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByText("mov,mp4")).toBeVisible();
});

it("separates same-id assets in different libraries and ignores late data from the old view", async () => {
  let finish: ((value: AssetMediaMetadata) => void) | undefined;
  mocks.metadata.mockReturnValueOnce(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  mocks.metadata.mockResolvedValue({ ...data, format: "webm" });
  renderWithProviders(<AssetMetadata detail={detail()} err={null} />);
  await waitFor(() => expect(mocks.metadata).toHaveBeenCalledTimes(1));
  act(() => useAppStore.setState({ current: { id: "b", name: "B", path: "/b", createdAt: 1 } }));
  expect(await screen.findByText("webm")).toBeVisible();
  await act(async () => finish?.(data));
  expect(screen.queryByText("mov,mp4")).not.toBeInTheDocument();
});

it("reuses a recent metadata result when the same source is reopened", async () => {
  const queryClient = createTestQueryClient();
  const first = renderWithProviders(<AssetMetadata detail={detail()} err={null} />, {
    queryClient,
  });
  expect(await screen.findByText("mov,mp4")).toBeVisible();
  first.unmount();
  renderWithProviders(<AssetMetadata detail={detail()} err={null} />, { queryClient });
  expect(await screen.findByText("mov,mp4")).toBeVisible();
  expect(mocks.metadata).toHaveBeenCalledTimes(1);
});

it("shows readable camera, lens, exposure and GPS fields together, with additional EXIF expandable", async () => {
  mocks.metadata.mockResolvedValue({
    ...data,
    exif: [
      { name: "Make", value: "ClipSeek Demo" },
      { name: "LensModel", value: "Demo 50mm F2.8" },
      { name: "ExposureTime", value: "1/125 s" },
      { name: "GPSLatitude", value: "31 deg 13 min 48 sec N" },
      { name: "GPSLongitude", value: "121 deg 28 min 12 sec E" },
      { name: "Software", value: "Demo generator" },
    ],
  });
  renderWithProviders(<AssetMetadata detail={detail({ kind: "image" })} err={null} />);
  const exif = (await screen.findByRole("heading", { name: "Photo EXIF" })).closest("section")!;
  expect(within(exif).getByText("Camera make")).toBeVisible();
  expect(within(exif).getAllByText("Demo 50mm F2.8")[0]).toBeVisible();
  expect(within(exif).getAllByText("1/125 s")[0]).toBeVisible();
  expect(within(exif).getAllByText("31 deg 13 min 48 sec N")[0]).toBeVisible();
  expect(within(exif).getAllByText("121 deg 28 min 12 sec E")[0]).toBeVisible();
  expect(within(exif).queryByText("Aperture")).not.toBeInTheDocument();
  await userEvent.setup().click(within(exif).getByText("All EXIF fields (6)"));
  expect(within(exif).getByText("Demo generator")).toBeVisible();
});

it("omits the EXIF section when no EXIF is available", async () => {
  renderWithProviders(<AssetMetadata detail={detail({ kind: "image" })} err={null} />);
  await screen.findByRole("heading", { name: "Image" });
  expect(screen.queryByRole("heading", { name: "Photo EXIF" })).not.toBeInTheDocument();
});
