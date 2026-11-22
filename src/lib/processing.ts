import type { Asset, ProcessingTask } from "@/types";
export function currentProcessingTasks(tasks: ProcessingTask[] = []) {
  const latest = new Map<string, ProcessingTask>();
  for (const task of tasks) {
    const key = task.kind === "warning" ? `warning:${task.id}` : task.kind;
    if ((latest.get(key)?.id ?? -1) < task.id) latest.set(key, task);
  }
  return [...latest.values()];
}
export const activeProcessing = (task: ProcessingTask) =>
  ["queued", "running"].includes(task.status);
export const needsProcessingAttention = (task: ProcessingTask) =>
  ["failed", "interrupted", "attention", "no_source"].includes(task.status);
export function visibleProcessingTasks(asset: Asset) {
  return currentProcessingTasks(asset.processingTasks)
    .filter((task) => {
      if (task.kind === "import") return false; // Base-import labels already show the precise import phase.
      if (activeProcessing(task)) return true;
      if (task.kind === "visual" && asset.visualIndexed) return false;
      if (task.kind === "subtitles" && asset.subtitleIndexed) return false;
      return needsProcessingAttention(task);
    })
    .sort((a, b) => Number(needsProcessingAttention(b)) - Number(needsProcessingAttention(a)));
}
