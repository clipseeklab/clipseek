/** Zustand application state: registered libraries, current library, and preferences. */
import { create } from "zustand";
import type { LibraryMeta, Settings } from "@/types";
interface AppStore {
  libraries: LibraryMeta[];
  current: LibraryMeta | null;
  settings: Settings;
  /** Whether initial backend configuration has loaded. */
  loaded: boolean;
  sidebarHidden: boolean;
  gridColumns: number;
  recentTagNames: string[];
  /** Network-volume notice stores the library ID and stays dismissed for this run. */
  volumeWarning: string | null;
  setLibraries: (libs: LibraryMeta[]) => void;
  setCurrent: (lib: LibraryMeta | null) => void;
  setSettings: (s: Settings) => void;
  setLoaded: (v: boolean) => void;
  setSidebarHidden: (v: boolean) => void;
  setGridColumns: (v: number) => void;
  setVolumeWarning: (libraryId: string | null) => void;
  markRecentTags: (names: string[]) => void;
}
const RECENT_TAGS_KEY = "clip-studio:recent-tags";
function loadRecentTags(): string[] {
  try {
    const value = JSON.parse(localStorage.getItem(RECENT_TAGS_KEY) ?? "[]");
    return Array.isArray(value)
      ? value.filter((name): name is string => typeof name === "string").slice(0, 5)
      : [];
  } catch {
    return [];
  }
}
export function flushRecentTags() {
  localStorage.setItem(RECENT_TAGS_KEY, JSON.stringify(useAppStore.getState().recentTagNames));
}
export const useAppStore = create<AppStore>((set) => ({
  libraries: [],
  current: null,
  settings: {
    language: "en",
    theme: "auto",
    trashRetentionDays: 30,

    mcpEnabled: false,
    mcpWriteEnabled: false,
    mcpPort: 0,
    mcpListenMode: "loopback",
    importMode: "reference",
    autoVisualIndex: true,
    autoSubtitleIndex: false,
  },
  loaded: false,
  sidebarHidden: false,
  gridColumns: 4,
  recentTagNames: loadRecentTags(),
  volumeWarning: null,
  setLibraries: (libraries) => set({ libraries }),
  setCurrent: (current) => set({ current }),
  setSettings: (settings) => set({ settings }),
  setLoaded: (loaded) => set({ loaded }),
  setSidebarHidden: (sidebarHidden) => set({ sidebarHidden }),
  setGridColumns: (gridColumns) => set({ gridColumns }),
  setVolumeWarning: (volumeWarning) => set({ volumeWarning }),
  markRecentTags: (names) =>
    set((state) => ({
      recentTagNames: [...names, ...state.recentTagNames]
        .map((name) => name.trim())
        .filter((name, index, all) => name && all.indexOf(name) === index)
        .slice(0, 5),
    })),
}));
