import { create } from "zustand";
import type { Location } from "react-router-dom";
import type { Asset } from "@/types";
interface VideoFrameSearchStore {
  target: {
    asset: Asset;
    location: Location;
    mode: "visual" | "dialogue";
  } | null;
  open: (asset: Asset, location: Location, mode?: "visual" | "dialogue") => void;
  close: () => void;
}
export const useVideoFrameSearchStore = create<VideoFrameSearchStore>((set) => ({
  target: null,
  open: (asset, location, mode = "visual") => set({ target: { asset, location, mode } }),
  close: () => set({ target: null }),
}));
