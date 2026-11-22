import { StrictMode } from "react";
import { act, fireEvent, renderWithProviders, screen } from "@test/helpers/render";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ModelProgress, ModelStatus } from "@/types";

const mocks = vi.hoisted(() => ({
  download: vi.fn<() => Promise<ModelStatus>>(),
  listener: null as ((event: { payload: ModelProgress }) => void) | null,
  unlisten: vi.fn(),
}));
vi.mock("@/lib/api", () => ({ api: { modelDownload: mocks.download } }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_event, listener) => {
    mocks.listener = listener;
    return mocks.unlisten;
  }),
}));
import ModelDownloadGate from "@/components/ModelDownloadGate";

async function mount(showcase = false) {
  const onReady = vi.fn();
  const view = renderWithProviders(
    <ModelDownloadGate embedded showcase={showcase} onReady={onReady} />,
  );
  await act(async () => {});
  return { ...view, onReady };
}
function progress(bytes: number, total = 100_000_000) {
  act(() =>
    mocks.listener?.({
      payload: {
        downloadedBytes: bytes,
        totalBytes: total,
        currentFile: "text.onnx",
        speedBps: 1_000_000,
        etaSeconds: 100,
      },
    }),
  );
}
async function tick(ms: number) {
  await act(async () => {
    vi.advanceTimersByTime(ms);
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  mocks.listener = null;
  mocks.download.mockReset().mockImplementation(() => new Promise(() => {}));
});

describe("model preparation", () => {
  it("starts once in Strict Mode and ignores completion after unmount", async () => {
    let complete!: (status: ModelStatus) => void;
    mocks.download.mockImplementation(
      () =>
        new Promise((resolve) => {
          complete = resolve;
        }),
    );
    const onReady = vi.fn();
    const { unmount } = renderWithProviders(
      <StrictMode>
        <ModelDownloadGate embedded onReady={onReady} />
      </StrictMode>,
    );
    await act(async () => {});
    expect(mocks.download).toHaveBeenCalledOnce();
    unmount();
    await act(async () =>
      complete({
        ready: true,
        totalBytes: 100,
        downloadedBytes: 100,
        precision: "q8",
        modelDirectory: "/models",
        version: "1",
      }),
    );
    expect(onReady).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("keeps healthy downloads free of network advice", async () => {
    await mount();
    for (let second = 0; second < 70; second++) {
      progress(1_000_000 * second);
      await tick(1000);
    }
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("waits for verified readiness and releases listeners and timers on unmount", async () => {
    let complete!: (status: ModelStatus) => void;
    mocks.download.mockImplementation(
      () =>
        new Promise((resolve) => {
          complete = resolve;
        }),
    );
    const { onReady, unmount } = await mount();
    expect(mocks.download).toHaveBeenCalledOnce();
    progress(100_000_000);
    await tick(35_000);
    expect(screen.getByText("Download complete. Verifying models…")).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(onReady).not.toHaveBeenCalled();
    await act(async () =>
      complete({
        ready: true,
        totalBytes: 100_000_000,
        downloadedBytes: 100_000_000,
        precision: "q8",
        modelDirectory: "/models",
        version: "1",
      }),
    );
    expect(onReady).toHaveBeenCalledOnce();
    unmount();
    expect(mocks.unlisten).toHaveBeenCalledOnce();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("detects byte stalls despite heartbeat events and clears advice when bytes resume", async () => {
    await mount();
    progress(500);
    for (let second = 0; second < 30; second++) {
      await tick(1000);
      progress(500);
    }
    await tick(1000);
    expect(screen.getByRole("status")).toHaveTextContent("Download progress has stopped changing");
    expect(screen.getByRole("status")).toHaveTextContent("switching to a more stable network");
    progress(1_000_000);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.getByText("Elapsed 0:31")).toBeInTheDocument();
  });

  it("warns after sustained slow progress and clears the warning after recovery", async () => {
    await mount();
    for (let second = 0; second < 62; second++) {
      progress(1000 * second);
      await tick(1000);
    }
    expect(screen.getByRole("status")).toHaveTextContent("The download is progressing slowly");
    progress(63_000);
    expect(screen.getByRole("status")).toHaveTextContent("The download is progressing slowly");
    progress(20_000_000);
    await tick(1000);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("offers recovery after a rejected download and resets the observation window on retry", async () => {
    mocks.download.mockRejectedValueOnce(new Error("connection lost"));
    const { onReady } = await mount();
    expect(screen.getByRole("alert")).toHaveTextContent("quit and reopen the app");
    expect(onReady).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await act(async () => {});
    expect(mocks.download).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await tick(29_000);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    await tick(1000);
    expect(screen.getByRole("status")).toBeInTheDocument();
  });

  it("does not advance when the backend returns an incomplete status", async () => {
    mocks.download.mockResolvedValue({
      ready: false,
      totalBytes: 100,
      downloadedBytes: 50,
      precision: "q8",
      modelDirectory: "/models",
      version: "1",
    });
    const { onReady } = await mount();
    expect(screen.getByRole("alert")).toHaveTextContent("Models are not ready yet");
    expect(onReady).not.toHaveBeenCalled();
  });
});

describe("feature carousel", () => {
  it("cycles independently of downloading, supports manual selection and pause", async () => {
    await mount(true);
    expect(
      screen.getByRole("heading", { name: "Remember the scene. Find the moment." }),
    ).toBeInTheDocument();
    await tick(7000);
    expect(screen.getByRole("heading", { name: "Give every idea a place" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Give every idea a place" }));
    await tick(14_000);
    expect(screen.getByRole("heading", { name: "Give every idea a place" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Resume feature carousel" }));
    await tick(7000);
    expect(
      screen.getByRole("heading", { name: "Remember the scene. Find the moment." }),
    ).toBeInTheDocument();
    expect(mocks.download).toHaveBeenCalledOnce();
  });

  it("disables autoplay for reduced motion while allowing manual navigation", async () => {
    vi.mocked(window.matchMedia).mockReturnValueOnce({
      matches: true,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    } as unknown as MediaQueryList);
    await mount(true);
    await tick(14_000);
    expect(
      screen.getByRole("heading", { name: "Remember the scene. Find the moment." }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Pause feature carousel" }),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Give every idea a place" }));
    expect(screen.getByRole("heading", { name: "Give every idea a place" })).toBeInTheDocument();
  });
});

describe("download callback lifetime", () => {
  it("keeps the current download while using an updated readiness callback", async () => {
    let complete!: (status: ModelStatus) => void;
    mocks.download.mockImplementation(
      () =>
        new Promise((resolve) => {
          complete = resolve;
        }),
    );
    const first = vi.fn();
    const next = vi.fn();
    const view = renderWithProviders(<ModelDownloadGate embedded onReady={first} />);
    await act(async () => {});
    view.rerender(<ModelDownloadGate embedded onReady={next} />);
    await act(async () =>
      complete({
        ready: true,
        totalBytes: 100,
        downloadedBytes: 100,
        precision: "q8",
        modelDirectory: "/models",
        version: "1",
      }),
    );
    expect(mocks.download).toHaveBeenCalledOnce();
    expect(first).not.toHaveBeenCalled();
    expect(next).toHaveBeenCalledOnce();
    view.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });
});
