import { queryKeys } from "@/lib/queryKeys";
import type { ReactNode } from "react";
import { QueryClientProvider, QueryObserver } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createTestQueryClient } from "@test/helpers/render";
import { useAppStore } from "@/stores/app";
import { useImportStore } from "@/stores/import";

const mocks = vi.hoisted(() => ({
  dragCallback: null as null | ((event: { payload: { type: string; paths: string[] } }) => void),
  dragUnlisten: vi.fn(),
  eventListeners: new Map<string, (event: { payload: unknown }) => void>(),
  eventUnlisteners: [] as ReturnType<typeof vi.fn>[],
  importPaths: vi.fn(() => Promise.resolve({ queued: 1 })),
  importFolders: vi.fn(() => Promise.resolve({ queued: 1 })),
  toastError: vi.fn(),
}));

vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: (callback: typeof mocks.dragCallback) => {
      mocks.dragCallback = callback;
      return Promise.resolve(mocks.dragUnlisten);
    },
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, callback: (event: { payload: unknown }) => void) => {
    mocks.eventListeners.set(event, callback);
    const unlisten = vi.fn();
    mocks.eventUnlisteners.push(unlisten);
    return Promise.resolve(unlisten);
  },
}));

vi.mock("@/lib/api", () => ({
  api: {
    importPaths: mocks.importPaths,
    importFolders: mocks.importFolders,
  },
}));

vi.mock("@/components/Toaster", () => ({
  toast: { error: mocks.toastError },
}));

import { useGlobalDnd } from "@/hooks/useGlobalDnd";

function wrapper(queryClient: ReturnType<typeof createTestQueryClient>) {
  return function TestProviders({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.importPaths.mockResolvedValue({ queued: 1 });
  mocks.importFolders.mockResolvedValue({ queued: 1 });
  mocks.dragCallback = null;
  mocks.eventListeners.clear();
  mocks.eventUnlisteners.length = 0;
  useImportStore.setState(useImportStore.getInitialState(), true);
  useAppStore.setState(
    {
      ...useAppStore.getInitialState(),
      current: {
        id: "library-current",
        name: "Current",
        path: "/library",
        createdAt: 1,
      },
    },
    true,
  );
});

describe("useGlobalDnd", () => {
  it("ignores asset-state events emitted by another library", () => {
    const queryClient = createTestQueryClient();
    renderHook(() => useGlobalDnd(), { wrapper: wrapper(queryClient) });
    act(() =>
      mocks.eventListeners.get("import://asset")?.({
        payload: { id: 1, libraryId: "previous-library", fileName: "old.png", importStage: "done" },
      }),
    );
    expect(useImportStore.getState().liveAssets.size).toBe(0);
    act(() =>
      mocks.eventListeners.get("import://done")?.({
        payload: { libraryId: "previous-library", total: 7, done: 7, failed: 0, duplicates: 0 },
      }),
    );
    expect(useImportStore.getState().summary).toBeNull();
  });
  it("captures the tag at drop time and sends it with the queued import", async () => {
    const queryClient = createTestQueryClient();
    renderHook(() => useGlobalDnd(), { wrapper: wrapper(queryClient) });
    act(() => {
      useImportStore.getState().setDropTags(["Travel"]);
      mocks.dragCallback?.({ payload: { type: "drop", paths: ["/media/photo.jpg"] } });
      useImportStore.getState().setDropTags([]);
    });
    await waitFor(() => expect(mocks.importPaths).toHaveBeenCalledTimes(1));
    const task = useImportStore.getState().tasks[0];
    expect(task.tags).toEqual(["Travel"]);
    expect(mocks.importPaths).toHaveBeenCalledWith(["/media/photo.jpg"], null, task.id, ["Travel"]);
  });
  it("routes a normal file drop through the import queue and backend", async () => {
    const queryClient = createTestQueryClient();
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");
    renderHook(() => useGlobalDnd(), { wrapper: wrapper(queryClient) });

    act(() => {
      mocks.dragCallback?.({ payload: { type: "drop", paths: ["/media/photo.jpg"] } });
    });

    await waitFor(() => expect(mocks.importPaths).toHaveBeenCalledTimes(1));
    const task = useImportStore.getState().tasks[0];
    expect(mocks.importPaths).toHaveBeenCalledWith(["/media/photo.jpg"], null, task.id);
    expect(task.status).toBe("processing");
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.folders.library(useAppStore.getState().current?.id),
    });
  });

  it("routes an image-search drop without creating an import task", () => {
    const queryClient = createTestQueryClient();
    useImportStore.getState().setDropMode("image-search");
    renderHook(() => useGlobalDnd(), { wrapper: wrapper(queryClient) });

    act(() => {
      mocks.dragCallback?.({
        payload: { type: "drop", paths: ["/media/readme.txt", "/media/query.png"] },
      });
    });

    expect(useImportStore.getState().searchDropPath).toBe("/media/query.png");
    expect(useImportStore.getState().tasks).toEqual([]);
    expect(mocks.importPaths).not.toHaveBeenCalled();
  });

  it("ignores another library's events and releases every listener on unmount", async () => {
    const queryClient = createTestQueryClient();
    const { unmount } = renderHook(() => useGlobalDnd(), { wrapper: wrapper(queryClient) });
    act(() => useImportStore.getState().enqueue(["/media/a.jpg", "/media/b.jpg"], null));
    await waitFor(() => expect(mocks.importPaths).toHaveBeenCalledTimes(1));

    const registered = mocks.eventListeners.get("import://registered-batch")!;
    act(() =>
      registered({
        payload: { libraryId: "library-other", sourcePaths: ["/media/a.jpg"] },
      }),
    );
    expect(useImportStore.getState().tasks[0].pendingPaths).toEqual([
      "/media/a.jpg",
      "/media/b.jpg",
    ]);

    act(() =>
      registered({
        payload: { libraryId: "library-current", sourcePaths: ["/media/a.jpg"] },
      }),
    );
    expect(useImportStore.getState().tasks[0].pendingPaths).toEqual(["/media/b.jpg"]);

    unmount();
    await waitFor(() => expect(mocks.dragUnlisten).toHaveBeenCalledTimes(1));
    await waitFor(() => {
      expect(mocks.eventUnlisteners.length).toBeGreaterThan(0);
      expect(mocks.eventUnlisteners.every((unlisten) => unlisten.mock.calls.length === 1)).toBe(
        true,
      );
    });
  });
});
it("ignores non-file enter and over events while retaining real file drag feedback", () => {
  renderHook(() => useGlobalDnd(), { wrapper: wrapper(createTestQueryClient()) });
  act(() => mocks.dragCallback?.({ payload: { type: "enter", paths: [] } }));
  expect(useImportStore.getState().dragging).toBe(false);
  act(() => mocks.dragCallback?.({ payload: { type: "over", paths: [] } }));
  expect(useImportStore.getState().dragging).toBe(false);
  act(() => mocks.dragCallback?.({ payload: { type: "enter", paths: ["/media/photo.jpg"] } }));
  expect(useImportStore.getState().dragging).toBe(true);
  act(() => mocks.dragCallback?.({ payload: { type: "over", paths: [] } }));
  expect(useImportStore.getState().dragging).toBe(true);
  act(() => mocks.dragCallback?.({ payload: { type: "leave", paths: [] } }));
  expect(useImportStore.getState().dragging).toBe(false);
  act(() => mocks.dragCallback?.({ payload: { type: "over", paths: [] } }));
  expect(useImportStore.getState().dragging).toBe(false);
  expect(mocks.importPaths).not.toHaveBeenCalled();
});

describe("import event library lifetime", () => {
  it("invalidates only the emitting library's asset cache", () => {
    const client = createTestQueryClient();
    client.setQueryData(["assets", "library-current", {}], { value: "current" });
    client.setQueryData(["assets", "library-other", {}], { value: "other" });
    renderHook(() => useGlobalDnd(), { wrapper: wrapper(client) });
    act(() =>
      mocks.eventListeners.get("import://maintenance-done")?.({ payload: "library-current" }),
    );
    expect(client.getQueryState(["assets", "library-current", {}])?.isInvalidated).toBe(true);
    expect(client.getQueryState(["assets", "library-other", {}])?.isInvalidated).toBe(false);
  });

  it("cancels delayed asset refreshes when the library changes", () => {
    vi.useFakeTimers();
    try {
      const client = createTestQueryClient();
      const invalidate = vi.spyOn(client, "invalidateQueries");
      const { unmount } = renderHook(() => useGlobalDnd(), { wrapper: wrapper(client) });
      act(() =>
        mocks.eventListeners.get("import://asset")?.({
          payload: {
            id: 1,
            libraryId: "library-current",
            fileName: "old.png",
            importStage: "done",
          },
        }),
      );
      act(() =>
        useAppStore
          .getState()
          .setCurrent({ id: "library-other", name: "Other", path: "/other", createdAt: 1 }),
      );
      invalidate.mockClear();
      act(() => {
        vi.advanceTimersByTime(350);
      });
      expect(invalidate).not.toHaveBeenCalled();
      expect(useImportStore.getState().liveAssets.size).toBe(0);
      unmount();
    } finally {
      vi.useRealTimers();
    }
  });

  it("cancels delayed asset refreshes on unmount", () => {
    vi.useFakeTimers();
    try {
      const client = createTestQueryClient();
      const invalidate = vi.spyOn(client, "invalidateQueries");
      const { unmount } = renderHook(() => useGlobalDnd(), { wrapper: wrapper(client) });
      act(() =>
        mocks.eventListeners.get("import://asset")?.({
          payload: {
            id: 1,
            libraryId: "library-current",
            fileName: "old.png",
            importStage: "done",
          },
        }),
      );
      unmount();
      act(() => {
        vi.advanceTimersByTime(350);
      });
      expect(invalidate).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it("does not clear new live cards when an old completion refresh resolves", async () => {
    const client = createTestQueryClient();
    let finishRefresh = () => {};
    const pending = new Promise<void>((resolve) => {
      finishRefresh = resolve;
    });
    vi.spyOn(client, "invalidateQueries").mockImplementation((filters) =>
      filters?.queryKey?.[0] === "assets" ? pending : Promise.resolve(),
    );
    const { unmount } = renderHook(() => useGlobalDnd(), { wrapper: wrapper(client) });
    act(() =>
      mocks.eventListeners.get("import://done")?.({
        payload: { libraryId: "library-current", total: 1, done: 1, failed: 0, duplicates: 0 },
      }),
    );
    act(() => {
      useAppStore
        .getState()
        .setCurrent({ id: "library-other", name: "Other", path: "/other", createdAt: 1 });
      mocks.eventListeners.get("library://switched")?.({ payload: {} });
      mocks.eventListeners.get("import://asset")?.({
        payload: { id: 2, libraryId: "library-other", fileName: "new.png", importStage: "done" },
      });
    });
    await waitFor(() => expect(useImportStore.getState().liveAssets.has(2)).toBe(true));
    await act(async () => {
      finishRefresh();
      await pending;
    });
    expect(useImportStore.getState().liveAssets.has(2)).toBe(true);
    unmount();
  });
});

it("keeps cards from a newer batch when a previous refresh finishes in the same library", async () => {
  const client = createTestQueryClient();
  let resolveRefresh = () => {};
  const pending = new Promise<void>((resolve) => {
    resolveRefresh = resolve;
  });
  vi.spyOn(client, "invalidateQueries").mockImplementation((filters) =>
    filters?.queryKey?.[0] === "assets" ? pending : Promise.resolve(),
  );
  const { unmount } = renderHook(() => useGlobalDnd(), { wrapper: wrapper(client) });
  act(() =>
    mocks.eventListeners.get("import://done")?.({
      payload: { libraryId: "library-current", total: 1, done: 1, failed: 0, duplicates: 0 },
    }),
  );
  act(() =>
    mocks.eventListeners.get("import://asset")?.({
      payload: { id: 3, libraryId: "library-current", fileName: "newer.png", importStage: "done" },
    }),
  );
  await waitFor(() => expect(useImportStore.getState().liveAssets.has(3)).toBe(true));
  await act(async () => {
    resolveRefresh();
    await pending;
  });
  expect(useImportStore.getState().liveAssets.has(3)).toBe(true);
  unmount();
});

it("preserves live cards and reports a failed completion refresh", async () => {
  const client = createTestQueryClient();
  const observer = new QueryObserver(client, {
    queryKey: ["assets", "library-current", {}],
    queryFn: () => Promise.reject(new Error("refresh failed")),
    initialData: [],
    staleTime: Infinity,
  });
  const stop = observer.subscribe(() => {});
  const { unmount } = renderHook(() => useGlobalDnd(), { wrapper: wrapper(client) });
  try {
    act(() =>
      mocks.eventListeners.get("import://asset")?.({
        payload: { id: 4, libraryId: "library-current", fileName: "kept.png", importStage: "done" },
      }),
    );
    await act(async () => {
      mocks.eventListeners.get("import://done")?.({
        payload: { libraryId: "library-current", total: 1, done: 1, failed: 0, duplicates: 0 },
      });
    });
    expect(useImportStore.getState().liveAssets.has(4)).toBe(true);
    expect(mocks.toastError).toHaveBeenCalledWith("refresh failed");
  } finally {
    unmount();
    stop();
  }
});

it("ignores late import and drop callbacks after unmount", () => {
  const client = createTestQueryClient();
  const { unmount } = renderHook(() => useGlobalDnd(), { wrapper: wrapper(client) });
  const assetEvent = mocks.eventListeners.get("import://asset");
  const dropEvent = mocks.dragCallback;
  unmount();
  act(() => {
    assetEvent?.({
      payload: { id: 1, libraryId: "library-current", fileName: "late.png", importStage: "done" },
    });
    dropEvent?.({ payload: { type: "drop", paths: ["/media/late.png"] } });
  });
  expect(useImportStore.getState().liveAssets.size).toBe(0);
  expect(useImportStore.getState().tasks).toEqual([]);
});

it("discards buffered progress when only the current-library store changes", () => {
  vi.useFakeTimers();
  try {
    const client = createTestQueryClient();
    const { unmount } = renderHook(() => useGlobalDnd(), { wrapper: wrapper(client) });
    act(() =>
      mocks.eventListeners.get("import://progress")?.({
        payload: {
          libraryId: "library-current",
          total: 2,
          done: 1,
          currentFile: "old.png",
          stage: "copy",
          failed: 0,
        },
      }),
    );
    act(() =>
      useAppStore
        .getState()
        .setCurrent({ id: "library-other", name: "Other", path: "/other", createdAt: 1 }),
    );
    act(() => {
      vi.advanceTimersByTime(50);
    });
    expect(useImportStore.getState().progress).toBeNull();
    unmount();
  } finally {
    vi.useRealTimers();
  }
});

it("ignores an old library's rejected import request after switching", async () => {
  let rejectImport = (_error: Error) => {};
  const pending = new Promise<{ queued: number }>((_resolve, reject) => {
    rejectImport = reject;
  });
  mocks.importPaths.mockReturnValueOnce(pending);
  const client = createTestQueryClient();
  const { unmount } = renderHook(() => useGlobalDnd(), { wrapper: wrapper(client) });
  act(() => useImportStore.getState().enqueue(["/media/old.png"], null));
  await waitFor(() => expect(mocks.importPaths).toHaveBeenCalledTimes(1));
  act(() =>
    useAppStore
      .getState()
      .setCurrent({ id: "library-other", name: "Other", path: "/other", createdAt: 1 }),
  );
  await act(async () => {
    rejectImport(new Error("old import failed"));
    await pending.catch(() => {});
  });
  expect(mocks.toastError).not.toHaveBeenCalled();
  expect(useImportStore.getState().tasks).toEqual([]);
  unmount();
});
