import type { ReactNode } from "react";
import { CancelledError, QueryClientProvider, QueryObserver } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createTestQueryClient } from "@test/helpers/render";
import { buildAsset } from "@test/fixtures/assets";
import { useAppStore } from "@/stores/app";
import { useImportStore } from "@/stores/import";
import { useImportEvents } from "@/hooks/useImportEvents";
import { queryKeys } from "@/lib/queryKeys";

const observed = vi.hoisted(() => ({
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
  error: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, callback: (event: { payload: unknown }) => void) => {
    observed.listeners.set(name, callback);
    return Promise.resolve(() => observed.listeners.delete(name));
  },
}));
vi.mock("@/components/Toaster", () => ({ toast: { error: observed.error } }));
const initialApp = useAppStore.getState();
beforeEach(() => {
  observed.listeners.clear();
  observed.error.mockReset();
  useImportStore.setState(useImportStore.getInitialState(), true);
  useAppStore.setState({
    current: { id: "source", name: "Source", path: "/source", createdAt: 1 },
  });
});
afterEach(() => {
  useAppStore.setState(initialApp, true);
  useImportStore.setState(useImportStore.getInitialState(), true);
});
function mount(client = createTestQueryClient()) {
  const hook = renderHook(() => useImportEvents(), {
    wrapper: ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    ),
  });
  return { ...hook, client };
}
function notify(name: string, payload: unknown) {
  observed.listeners.get(name)!({ payload });
}

it("does not toast when a third real refresh cancels the request followed by the first", async () => {
  const client = createTestQueryClient();
  const queryKey = queryKeys.assets.library("source");
  client.setQueryData(queryKey, { items: [] });
  const pending: Array<(data: { items: never[] }) => void> = [];
  const observer = new QueryObserver(client, {
    queryKey,
    staleTime: Infinity,
    queryFn: () => new Promise<{ items: never[] }>((resolve) => pending.push(resolve)),
  });
  const stop = observer.subscribe(() => {});
  const invalidate = vi.spyOn(client, "invalidateQueries");
  const hook = mount(client);
  try {
    act(() => {
      notify("import://maintenance-done", "source");
      notify("import://maintenance-done", "source");
    });
    await Promise.resolve();
    await Promise.resolve();
    act(() => notify("import://maintenance-done", "source"));
    expect(pending).toHaveLength(3);
    await act(async () => {
      pending[pending.length - 1]!({ items: [] });
      await Promise.allSettled(invalidate.mock.results.map((result) => result.value));
    });
    expect(observed.error).not.toHaveBeenCalled();
  } finally {
    for (const resolve of pending) resolve({ items: [] });
    hook.unmount();
    stop();
    client.clear();
  }
});

it("preserves live cards on cancellation while still reporting a real error with the same message", async () => {
  const { client } = mount();
  const invalidate = vi.spyOn(client, "invalidateQueries");
  useImportStore.getState().applyAsset(buildAsset({ id: 7 }));
  invalidate.mockRejectedValue(new CancelledError({ silent: true }));
  act(() =>
    notify("import://done", { libraryId: "source", total: 1, done: 1, failed: 0, duplicates: 0 }),
  );
  await act(async () => {
    await Promise.allSettled(invalidate.mock.results.map((result) => result.value));
  });
  expect(useImportStore.getState().liveAssets.has(7)).toBe(true);
  expect(observed.error).not.toHaveBeenCalled();
  invalidate.mockRejectedValueOnce(new Error("CancelledError"));
  act(() => notify("folders://changed", "source"));
  await waitFor(() => expect(observed.error).toHaveBeenCalledWith("CancelledError"));
});

it("publishes a burst once, retaining every distinct asset and the latest version of each", () => {
  vi.useFakeTimers();
  const hook = mount();
  const changes = vi.fn();
  const stop = useImportStore.subscribe((state, previous) => {
    if (state.liveAssets !== previous.liveAssets) changes();
  });
  try {
    act(() => {
      for (let id = 1; id <= 300; id++)
        notify("import://asset", { ...buildAsset({ id }), libraryId: "source" });
      notify("import://asset", {
        ...buildAsset({ id: 1, visualIndexed: true }),
        libraryId: "source",
      });
    });
    expect(changes).not.toHaveBeenCalled();
    act(() => {
      vi.advanceTimersToNextTimer();
    });
    expect(changes).toHaveBeenCalledTimes(1);
    expect(useImportStore.getState().liveAssets.size).toBe(300);
    expect(useImportStore.getState().liveAssets.get(1)?.visualIndexed).toBe(true);
  } finally {
    stop();
    hook.unmount();
    vi.useRealTimers();
  }
});

it("flushes buffered terminal cards before completion refresh and discards old-library or unmounted work", () => {
  vi.useFakeTimers();
  const { client, unmount } = mount();
  const invalidate = vi.spyOn(client, "invalidateQueries");
  const snapshots: number[][] = [];
  invalidate.mockImplementation(() => {
    snapshots.push([...useImportStore.getState().liveAssets.keys()]);
    return Promise.resolve();
  });
  try {
    act(() => {
      notify("import://asset", { ...buildAsset({ id: 1 }), libraryId: "source" });
      notify("import://done", { libraryId: "source", total: 1, done: 1, failed: 0, duplicates: 0 });
    });
    expect(snapshots[0]).toEqual([1]);
    act(() => {
      notify("import://asset", { ...buildAsset({ id: 2 }), libraryId: "source" });
      useAppStore
        .getState()
        .setCurrent({ id: "target", name: "Target", path: "/target", createdAt: 1 });
      notify("import://asset", { ...buildAsset({ id: 3 }), libraryId: "target" });
      vi.advanceTimersToNextTimer();
    });
    expect([...useImportStore.getState().liveAssets.keys()]).toEqual([3]);
    act(() => notify("import://asset", { ...buildAsset({ id: 4 }), libraryId: "target" }));
    unmount();
    act(() => {
      vi.runOnlyPendingTimers();
    });
    expect([...useImportStore.getState().liveAssets.keys()]).toEqual([3]);
  } finally {
    unmount();
    vi.useRealTimers();
  }
});

it("does not republish buffered patches after a database snapshot takes ownership", () => {
  vi.useFakeTimers();
  const { client, unmount } = mount();
  try {
    act(() => {
      notify("import://asset", {
        ...buildAsset({ id: 1, importStage: "probing" }),
        libraryId: "source",
      });
      client.setQueryData(queryKeys.assets.pages("source", {}), {
        pages: [{ items: [buildAsset({ id: 1 })] }],
      });
      useImportStore.getState().reconcileLiveAssets([1]);
      vi.advanceTimersToNextTimer();
    });
    expect(useImportStore.getState().liveAssets.has(1)).toBe(false);
  } finally {
    unmount();
    client.clear();
    vi.useRealTimers();
  }
});
