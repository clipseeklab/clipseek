import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "@test/helpers/render";
import { stubConvertFileSrc } from "@test/helpers/tauri";

const mocks = vi.hoisted(() => ({
  assetPlaybackSource: vi.fn(),
  assetPlaybackCodec: vi.fn(),
  frontendError: vi.fn(),
}));

vi.mock("@/lib/platform", () => ({ isWindows: true }));
vi.mock("@/lib/api", () => ({
  api: {
    assetPlaybackSource: mocks.assetPlaybackSource,
    assetPlaybackCodec: mocks.assetPlaybackCodec,
    frontendError: mocks.frontendError,
  },
}));

import HoverVideoPreview from "@/components/assets/HoverVideoPreview";

beforeEach(() => {
  vi.clearAllMocks();
  stubConvertFileSrc();
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  mocks.assetPlaybackSource.mockResolvedValue("http://127.0.0.1:43210/video.mp4");
  mocks.frontendError.mockResolvedValue(undefined);
});

describe("Windows hover video preview", () => {
  it("never falls back to a raw video path when streaming source resolution fails", async () => {
    mocks.assetPlaybackSource.mockRejectedValue(new Error("server unavailable"));
    renderWithProviders(<HoverVideoPreview active assetId={954} videoPath="C:/big.mkv" />);
    await waitFor(() => expect(mocks.frontendError).toHaveBeenCalled());
    expect(document.querySelector('video[data-hover-player="shared"]')).not.toHaveAttribute("src");
  });
  it("does not interrupt browsing when a card's hover video cannot play", async () => {
    renderWithProviders(
      <HoverVideoPreview active assetId={910} videoPath="C:/videos/sample.mp4" />,
    );
    const video = document.querySelector<HTMLVideoElement>('video[data-hover-player="shared"]')!;
    await waitFor(() => expect(video.getAttribute("src")).toContain("127.0.0.1"));
    Object.defineProperty(video, "error", {
      configurable: true,
      value: { code: 4, message: "unsupported" },
    });
    fireEvent.error(video);

    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(mocks.assetPlaybackCodec).not.toHaveBeenCalled();
  });
  it("never falls back to the original when preview preparation fails", async () => {
    let reject!: (error: Error) => void;
    mocks.assetPlaybackSource.mockReturnValueOnce(
      new Promise<string>((_, fail) => {
        reject = fail;
      }),
    );
    renderWithProviders(<HoverVideoPreview active assetId={911} videoPath="/external/raw.mov" />);
    await waitFor(() => expect(mocks.assetPlaybackSource).toHaveBeenCalledWith(911));
    await act(async () => {
      reject(new Error("proxy failed"));
    });
    expect(
      document.querySelector('video[data-hover-player="shared"]')?.getAttribute("src") ?? "",
    ).not.toContain("raw.mov");
  });
});
