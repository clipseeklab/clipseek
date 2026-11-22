import { useMemo, useRef } from "react";
import type { ImportProgress } from "@/types";
const ESTIMATE_SMOOTHING = 0.25;
const MAX_ETA_SECONDS = 30 * 24 * 60 * 60;
/** Map stages with different work units to approximate monotonic batch completion; weights are
 * estimates.
 */
export function importOverallFraction(progress: ImportProgress) {
  const ratio = progress.total > 0 ? Math.min(1, Math.max(0, progress.done / progress.total)) : 0;
  if (progress.stage === "copy") return ratio * 0.15;
  if (progress.stage === "embed") return 0.65 + ratio * 0.35;
  if (progress.stage === "finish") return 1;
  if (progress.stage === "error") return 0;
  return 0.15 + ratio * 0.5;
}
/** Estimate remaining end-to-end time from elapsed time and batch completion. */
export class ImportEtaEstimator {
  private signature = "";
  private startedAt = 0;
  private lastFraction = 0;
  private estimatedTotal: number | null = null;
  reset() {
    this.signature = "";
    this.startedAt = 0;
    this.lastFraction = 0;
    this.estimatedTotal = null;
  }
  update(progress: ImportProgress, taskStartedAt?: number, nowMs = Date.now()): number | null {
    const signature = `${progress.libraryId}:${taskStartedAt ?? "maintenance"}`;
    if (signature !== this.signature) {
      this.signature = signature;
      this.startedAt = taskStartedAt ?? nowMs;
      this.lastFraction = 0;
      this.estimatedTotal = null;
    }
    const fraction = importOverallFraction(progress);
    if (fraction >= 1) return 0;
    const elapsedSeconds = Math.max(0, (nowMs - this.startedAt) / 1000);
    if (elapsedSeconds < 1 || fraction <= 0) return null;
    if (fraction > this.lastFraction) {
      const observedTotal = elapsedSeconds / fraction;
      this.estimatedTotal =
        this.estimatedTotal == null
          ? observedTotal
          : this.estimatedTotal * (1 - ESTIMATE_SMOOTHING) + observedTotal * ESTIMATE_SMOOTHING;
      this.lastFraction = fraction;
    }
    if (!this.estimatedTotal || !Number.isFinite(this.estimatedTotal)) return null;
    return Math.min(MAX_ETA_SECONDS, Math.max(1, Math.ceil(this.estimatedTotal - elapsedSeconds)));
  }
}
export function useImportEta(progress: ImportProgress | null, taskStartedAt?: number) {
  const estimator = useRef(new ImportEtaEstimator());
  return useMemo(() => {
    if (!progress) {
      estimator.current.reset();
      return null;
    }
    return estimator.current.update(progress, taskStartedAt);
  }, [progress, taskStartedAt]);
}
