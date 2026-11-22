import { afterEach, describe, expect, it, vi } from "vitest";
import { useImportStore } from "@/stores/import";
import { buildAsset } from "@test/fixtures/assets";

afterEach(() => {
  useImportStore.setState(useImportStore.getInitialState(), true);
});

describe("import queue", () => {
  it("lets refreshed database cards replace transient event patches without removing other in-flight assets", () => {
    const one = buildAsset({ id: 1 }),
      two = buildAsset({ id: 2 });
    useImportStore.getState().applyAsset(one);
    useImportStore.getState().applyAsset(two);
    useImportStore.getState().reconcileLiveAssets([1]);
    expect([...useImportStore.getState().liveAssets.keys()]).toEqual([2]);
    useImportStore.getState().applyAsset({ ...one, visualIndexed: true });
    expect(useImportStore.getState().liveAssets.get(1)?.visualIndexed).toBe(true);
  });
  it("tracks supported pending files and completes the matching batch", () => {
    const store = useImportStore.getState();
    store.enqueue(["/files/photo.jpg", "/files/readme.txt", "/files/movie.mp4"], 42);

    const [task] = useImportStore.getState().tasks;
    expect(task.folderId).toBe(42);
    expect(task.pendingPaths).toEqual(["/files/photo.jpg", "/files/movie.mp4"]);

    useImportStore.getState().markDispatched(task.id);
    expect(useImportStore.getState().tasks[0].status).toBe("processing");

    useImportStore.getState().finishTask(task.id, {
      total: 2,
      done: 2,
      failed: 0,
      duplicates: 0,
    });
    expect(useImportStore.getState().tasks[0].status).toBe("success");
    expect(useImportStore.getState().summary).toEqual({
      total: 2,
      done: 2,
      failed: 0,
      duplicates: 0,
    });
  });

  it("removes only paths registered by the backend", () => {
    useImportStore.getState().enqueue(["/a.jpg", "/b.jpg", "/c.jpg"], null);
    useImportStore.getState().markRegisteredBatch(["/b.jpg", "/outside.jpg"]);

    expect(useImportStore.getState().tasks[0].pendingPaths).toEqual(["/a.jpg", "/c.jpg"]);
  });

  it("keeps batches independent when one command fails", () => {
    useImportStore.getState().enqueue(["/first.jpg"], null);
    const firstId = useImportStore.getState().tasks[0].id;
    useImportStore.getState().enqueue(["/second.jpg"], 9);
    const [second, first] = useImportStore.getState().tasks;

    useImportStore.getState().markDispatched(firstId);
    useImportStore.getState().failTask(firstId, "disk full");

    expect(first).toMatchObject({ id: firstId });
    expect(useImportStore.getState().tasks).toEqual([
      expect.objectContaining({ id: second.id, status: "pending" }),
      expect.objectContaining({ id: firstId, status: "failed", error: "disk full" }),
    ]);
    expect(useImportStore.getState().tasks[0]).not.toHaveProperty("error");
  });

  it("does not create file placeholders for a folder scan", () => {
    useImportStore.getState().enqueue(["/media/folder"], 3, "folders");

    expect(useImportStore.getState().tasks[0]).toMatchObject({
      mode: "folders",
      pendingPaths: [],
      folderId: 3,
    });
  });

  it("restores a persisted recovery job with its backend batch id", () => {
    useImportStore.getState().restoreTask({
      id: "recovery-job-1",
      mode: "folders",
      paths: ["/media/large-folder"],
      folderId: null,
      status: "interrupted",
      total: 10_000,
      done: 1_000,
      failed: 0,
      duplicates: 0,
      createdAt: 123,
    });

    expect(useImportStore.getState().tasks).toEqual([
      expect.objectContaining({
        id: "recovery-job-1",
        path: "/media/large-folder",
        paths: ["/media/large-folder"],
        mode: "folders",
        status: "processing",
        createdAt: 123,
      }),
    ]);

    useImportStore.getState().finishTask("recovery-job-1", {
      total: 10_000,
      done: 10_000,
      failed: 0,
      duplicates: 0,
    });
    expect(useImportStore.getState().tasks[0].status).toBe("success");
  });

  it("bounds live assets and ignores an identical repeat", () => {
    for (let id = 1; id <= 501; id += 1) {
      useImportStore.getState().applyAsset(buildAsset({ id }));
    }

    const liveAssets = useImportStore.getState().liveAssets;
    expect(liveAssets.size).toBe(500);
    expect(liveAssets.has(1)).toBe(false);
    expect(liveAssets.has(501)).toBe(true);

    const listener = vi.fn();
    const unsubscribe = useImportStore.subscribe(listener);
    useImportStore.getState().applyAsset(buildAsset({ id: 501 }));
    expect(listener).not.toHaveBeenCalled();
    unsubscribe();
  });

  it("clears all transient queue state", () => {
    useImportStore.getState().enqueue(["/photo.jpg"], null);
    useImportStore.getState().setQueuePaused(true);
    useImportStore.getState().setSummary({ total: 1, done: 1, failed: 0, duplicates: 0 });

    useImportStore.getState().clearQueue();

    expect(useImportStore.getState()).toMatchObject({
      tasks: [],
      progress: null,
      summary: null,
      queuePaused: false,
    });
  });
});

describe("persisted import tag intent", () => {
  it("copies recovered tags into the restored queue task", () => {
    const job = {
      id: "tag-recovery",
      mode: "paths" as const,
      paths: ["/source/a.jpg"],
      folderId: null,
      status: "interrupted" as const,
      total: 1,
      done: 0,
      failed: 0,
      duplicates: 0,
      createdAt: 1,
      tags: ["旅行", "收藏"],
    };
    useImportStore.getState().restoreTask(job);
    expect(useImportStore.getState().tasks[0].tags).toEqual(["旅行", "收藏"]);
    job.tags.push("后来添加");
    expect(useImportStore.getState().tasks[0].tags).toEqual(["旅行", "收藏"]);
  });
});

it("publishes an entire asset batch once while keeping the existing live-card bound", () => {
  const changed = vi.fn();
  const stop = useImportStore.subscribe(changed);
  try {
    const batch = Array.from({ length: 700 }, (_, index) => buildAsset({ id: index + 1 }));
    useImportStore.getState().applyAssets(batch);
    expect(changed).toHaveBeenCalledTimes(1);
    expect(useImportStore.getState().liveAssets.size).toBe(500);
    expect([...useImportStore.getState().liveAssets.keys()]).toEqual(
      batch.slice(-500).map((asset) => asset.id),
    );
    useImportStore.getState().applyAssets(batch.slice(-500));
    expect(changed).toHaveBeenCalledTimes(1);
  } finally {
    stop();
  }
});
