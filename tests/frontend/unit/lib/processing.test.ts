import { describe, expect, it } from "vitest";
import { currentProcessingTasks, visibleProcessingTasks } from "@/lib/processing";
import type { Asset, ProcessingTask } from "@/types";
const task = (
  id: number,
  status: string,
  kind: ProcessingTask["kind"] = "visual",
): ProcessingTask => ({ id, status, kind, phase: kind, error: null, attempt: 1, indexTaskId: id });
describe("shared processing state", () => {
  it("chooses the latest attempt regardless of transport ordering", () => {
    expect(
      currentProcessingTasks([task(3, "completed"), task(1, "failed"), task(2, "queued")]),
    ).toEqual([task(3, "completed")]);
  });
  it("keeps concurrent queued work visible after import completes and prioritizes failures", () => {
    const asset = {
      visualIndexed: false,
      subtitleIndexed: false,
      processingTasks: [task(1, "running"), task(2, "failed", "subtitles")],
    } as Asset;
    expect(visibleProcessingTasks(asset).map((t) => t.status)).toEqual(["failed", "running"]);
  });
  it("does not keep historical errors on a successfully indexed asset", () => {
    const asset = { visualIndexed: true, processingTasks: [task(1, "failed")] } as Asset;
    expect(visibleProcessingTasks(asset)).toEqual([]);
    asset.processingTasks = [task(2, "running")];
    expect(visibleProcessingTasks(asset)).toHaveLength(1);
  });
});
