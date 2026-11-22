export const MAX_PENDING_PREVIEWS = 48;
export interface PendingPreviewTask {
  id: string;
  pendingPaths: string[];
}
export function collectPendingPreviews(tasks: PendingPreviewTask[], limit = MAX_PENDING_PREVIEWS) {
  let total = 0;
  const items: Array<{
    key: string;
    name: string;
  }> = [];
  for (const task of tasks) {
    total += task.pendingPaths.length;
    if (items.length >= limit) continue;
    for (const path of task.pendingPaths) {
      if (items.length >= limit) break;
      items.push({
        key: `${task.id}:${path}`,
        name: path.split(/[\\/]/).pop() || path,
      });
    }
  }
  return { items, total };
}
export function removeRegisteredPaths(pendingPaths: string[], registeredPaths: string[]) {
  if (registeredPaths.length === 0 || pendingPaths.length === 0) return pendingPaths;
  const registered = new Set(registeredPaths);
  const remaining = pendingPaths.filter((path) => !registered.has(path));
  return remaining.length === pendingPaths.length ? pendingPaths : remaining;
}
