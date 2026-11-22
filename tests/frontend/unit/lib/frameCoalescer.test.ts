// Frame scheduling is tested with deterministic callbacks instead of real time.
import { describe, expect, it } from "vitest";
import { FrameCoalescer } from "@/lib/frameCoalescer";

describe("FrameCoalescer", () => {
  it("publishes only the newest value pushed within one animation frame", () => {
    let nextHandle = 1;
    const callbacks = new Map<number, () => void>();
    const consumed: Array<string | null> = [];
    const buffer = new FrameCoalescer<string>(
      (callback) => {
        const handle = nextHandle++;
        callbacks.set(handle, () => {
          callbacks.delete(handle);
          callback();
        });
        return handle;
      },
      (handle) => callbacks.delete(handle),
      (value) => consumed.push(value),
    );

    buffer.push("embed 63/64");
    buffer.push("embed 64/64");

    expect(callbacks.size).toBe(1);
    callbacks.values().next().value?.();
    expect(consumed).toEqual(["embed 64/64"]);
  });

  it("does not allow a cancelled frame to restore stale progress", () => {
    const callbacks = new Map<number, () => void>();
    const consumed: Array<string | null> = [];
    const buffer = new FrameCoalescer<string>(
      (callback) => {
        callbacks.set(1, callback);
        return 1;
      },
      (handle) => callbacks.delete(handle),
      (value) => consumed.push(value),
    );

    buffer.push("stale final progress");
    const staleCallback = callbacks.get(1)!;
    buffer.clear();
    staleCallback();

    expect(consumed).toEqual([null]);
  });
});
