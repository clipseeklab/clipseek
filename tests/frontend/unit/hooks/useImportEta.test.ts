// ETA tests use fixed timestamps and never depend on wall-clock time.
import { describe, expect, it } from "vitest";
import type { ImportProgress } from "@/types";
import { ImportEtaEstimator, importOverallFraction } from "@/hooks/useImportEta";

function progress(overrides: Partial<ImportProgress> = {}): ImportProgress {
  return {
    libraryId: "library-1",
    folderId: null,
    total: 100,
    done: 0,
    currentFile: "example.jpg",
    stage: "copy",
    failed: 0,
    ...overrides,
  };
}

describe("importOverallFraction", () => {
  it("maps pipeline stages to monotonic user-visible progress", () => {
    expect(importOverallFraction(progress({ stage: "copy", done: 100 }))).toBe(0.15);
    expect(importOverallFraction(progress({ stage: "probe", done: 50 }))).toBe(0.4);
    expect(importOverallFraction(progress({ stage: "embed", done: 50 }))).toBe(0.825);
    expect(importOverallFraction(progress({ stage: "finish", done: 100 }))).toBe(1);
  });
});

describe("ImportEtaEstimator", () => {
  it("smooths estimates across the complete import pipeline", () => {
    const estimator = new ImportEtaEstimator();
    const startedAt = 1_000_000;

    expect(estimator.update(progress(), startedAt, startedAt)).toBeNull();
    expect(estimator.update(progress({ done: 100 }), startedAt, startedAt + 15_000)).toBe(85);

    const mediaEta = estimator.update(
      progress({ stage: "probe", done: 50 }),
      startedAt,
      startedAt + 40_000,
    );
    expect(mediaEta).toBeGreaterThan(50);
    expect(mediaEta).toBeLessThan(70);

    const embedEta = estimator.update(
      progress({ stage: "embed", done: 50 }),
      startedAt,
      startedAt + 80_000,
    );
    expect(embedEta).toBeGreaterThan(10);
    expect(embedEta).toBeLessThan(30);
    expect(
      estimator.update(progress({ stage: "finish", done: 100 }), startedAt, startedAt + 100_000),
    ).toBe(0);
  });

  it("starts an independent estimate for a new task and supports reset", () => {
    const estimator = new ImportEtaEstimator();

    expect(estimator.update(progress({ done: 100 }), 2_000_000, 2_015_000)).toBe(85);
    const nextTask = estimator.update(progress({ done: 10 }), 3_000_000, 3_001_000);
    expect(nextTask).not.toBeNull();
    expect(nextTask!).toBeGreaterThan(40);

    estimator.reset();
    expect(estimator.update(progress(), undefined, 4_000_000)).toBeNull();
  });
});
