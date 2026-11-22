// Correctness checks are separate from the benchmark in benchmarks/.
import { describe, expect, it } from "vitest";
import {
  MAX_PENDING_PREVIEWS,
  collectPendingPreviews,
  removeRegisteredPaths,
} from "@/lib/importPerformance";

describe("large import state helpers", () => {
  it("bounds rendered placeholders without losing the total count", () => {
    const paths = Array.from({ length: 10_000 }, (_, index) => `/media/photo-${index}.jpg`);
    const preview = collectPendingPreviews([{ id: "large-import", pendingPaths: paths }]);

    expect(preview.total).toBe(10_000);
    expect(preview.items).toHaveLength(MAX_PENDING_PREVIEWS);
  });

  it("removes registered paths in batches and preserves the original when nothing matches", () => {
    const pending = ["/a.jpg", "/b.jpg", "/c.jpg"];
    expect(removeRegisteredPaths(pending, ["/b.jpg"])).toEqual(["/a.jpg", "/c.jpg"]);
    expect(removeRegisteredPaths(pending, ["/outside.jpg"])).toBe(pending);
  });
});
