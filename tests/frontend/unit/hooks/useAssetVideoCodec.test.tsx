import type { ReactNode } from "react";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createTestQueryClient } from "@test/helpers/render";
import { buildAsset } from "@test/fixtures/assets";
import { useAppStore } from "@/stores/app";
import { useAssetVideoCodec } from "@/hooks/useAssetVideoCodec";

const probe = vi.hoisted(() => vi.fn());
vi.mock("@/lib/api", () => ({ api: { assetPlaybackCodec: probe } }));

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

const video = buildAsset({ kind: "video", absPath: "/movie.mov" });
const library = (id: string) => ({ id, name: id, path: `/${id}`, createdAt: 1 });

function harness() {
  const client = createTestQueryClient();
  return {
    client,
    wrapper: ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    ),
  };
}

beforeEach(() => {
  probe.mockReset().mockResolvedValue("h264");
  useAppStore.setState({ current: library("one") });
});
afterEach(() => useAppStore.setState({ current: null }));

describe("video codec probe queue", () => {
  it.each([
    { kind: "image" },
    { importStage: "queued" },
    { importStage: "error:decode" },
    { sourceMissing: true },
  ] as const)("does not probe unavailable video metadata: %j", async (overrides) => {
    const { wrapper } = harness();
    const { result } = renderHook(
      () => useAssetVideoCodec(buildAsset({ ...video, ...overrides })),
      { wrapper },
    );
    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.fetchStatus).toBe("idle");
    expect(probe).not.toHaveBeenCalled();
  });

  it("shares cached probes and refreshes when the source generation changes", async () => {
    const { wrapper } = harness();
    const { result, rerender } = renderHook(
      ({ asset }) => {
        const first = useAssetVideoCodec(asset);
        const second = useAssetVideoCodec(asset);
        return [first, second];
      },
      { wrapper, initialProps: { asset: video } },
    );
    await waitFor(() => expect(result.current.every((query) => query.data === "h264")).toBe(true));
    expect(probe).toHaveBeenCalledTimes(1);
    rerender({ asset: { ...video, fileSize: video.fileSize + 1 } });
    await waitFor(() => expect(probe).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(result.current[0].data).toBe("h264"));
  });

  it("keeps probes serial and starts the next video after a failure", async () => {
    const first = deferred<string>();
    probe.mockReturnValueOnce(first.promise);
    const { wrapper } = harness();
    const { result } = renderHook(
      () => [useAssetVideoCodec(video), useAssetVideoCodec({ ...video, id: 2 })],
      { wrapper },
    );
    await waitFor(() => expect(probe).toHaveBeenCalledTimes(1));
    expect(result.current[1].fetchStatus).toBe("fetching");
    await act(async () => first.reject(new Error("bad media")));
    await waitFor(() => expect(result.current[1].data).toBe("h264"));
    expect(result.current[0].error?.message).toBe("bad media");
    expect(probe.mock.calls).toEqual([[video.id], [2]]);
  });

  it("discards an unmounted queued probe before it reaches IPC", async () => {
    const first = deferred<string>();
    probe.mockReturnValueOnce(first.promise);
    const { wrapper } = harness();
    const active = renderHook(() => useAssetVideoCodec(video), { wrapper });
    await waitFor(() => expect(probe).toHaveBeenCalledTimes(1));
    const queued = renderHook(() => useAssetVideoCodec({ ...video, id: 2 }), { wrapper });
    queued.unmount();
    await act(async () => first.resolve("h264"));
    await waitFor(() => expect(active.result.current.data).toBe("h264"));
    expect(probe.mock.calls).toEqual([[video.id]]);
  });

  it("does not probe the old library's queued video after switching libraries", async () => {
    const first = deferred<string>();
    probe.mockReturnValueOnce(first.promise);
    const { wrapper } = harness();
    const { result } = renderHook(
      () => [useAssetVideoCodec(video), useAssetVideoCodec({ ...video, id: 2 })],
      { wrapper },
    );
    await waitFor(() => expect(probe).toHaveBeenCalledTimes(1));
    act(() => useAppStore.setState({ current: library("two") }));
    await act(async () => first.resolve("old-codec"));
    await waitFor(() => expect(result.current.every((query) => query.data === "h264")).toBe(true));
    expect(probe.mock.calls).toEqual([[video.id], [video.id], [2]]);
  });
});
