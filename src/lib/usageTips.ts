/** Stable IDs and per-tip revisions let new content surface without replaying onboarding. */
export const usageTips = [
  { id: "visual", revision: 1, action: "search" },
  { id: "organize", revision: 1 },
  { id: "reference", revision: 1, action: "settings" },
  { id: "image", revision: 1 },
] as const;
export interface TipsState {
  pending: boolean;
  presented: boolean;
  lastId: string;
  seen: Record<string, number>;
  announced: Record<string, number>;
}
const KEY = "clipstudio-open:usage-tips";
export function loadTipsState(): TipsState {
  const fallback: TipsState = {
    pending: false,
    presented: false,
    lastId: "visual",
    seen: {},
    announced: {},
  };
  try {
    const value = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (!value || typeof value !== "object") return fallback;
    return {
      pending: value.pending === true,
      presented: value.presented === true,
      lastId: typeof value.lastId === "string" ? value.lastId : fallback.lastId,
      announced:
        value.announced && typeof value.announced === "object" && !Array.isArray(value.announced)
          ? value.announced
          : {},
      seen:
        value.seen && typeof value.seen === "object" && !Array.isArray(value.seen)
          ? value.seen
          : {},
    };
  } catch {
    return fallback;
  }
}
export function saveTipsState(state: TipsState) {
  try {
    localStorage.setItem(KEY, JSON.stringify(state));
  } catch {
    /* Tips must not block app startup. */
  }
}
export function prepareFirstUseTips({
  existingOnly = false,
  restart = false,
}: {
  existingOnly?: boolean;
  restart?: boolean;
} = {}) {
  // Recover onboarding tips suppressed by earlier versions, without enrolling
  // established users who have no first-use record.
  if (existingOnly) {
    try {
      const saved = JSON.parse(localStorage.getItem(KEY) ?? "null");
      if (saved?.presented !== false) return;
    } catch {
      return;
    }
  }
  const state = loadTipsState();
  // Native onboarding can be reset while the WebView retains its old reading history.
  // A new onboarding journey must offer the first card again.
  if (restart)
    saveTipsState({ ...state, pending: true, presented: false, lastId: usageTips[0].id });
  else if (!state.presented) saveTipsState({ ...state, pending: true });
}
