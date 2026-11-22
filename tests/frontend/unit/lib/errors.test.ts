// User-facing error formatting stays independent from the Tauri runtime.
import { describe, expect, it } from "vitest";
import { explainError, formatErrorBytes } from "@/lib/errors";

describe("formatErrorBytes", () => {
  it("formats zero and binary units for user-facing errors", () => {
    expect(formatErrorBytes()).toBe("0 B");
    expect(formatErrorBytes(1024)).toBe("1 KB");
    expect(formatErrorBytes(5 * 1024 ** 3)).toBe("5.0 GB");
  });
});

describe("first import size errors", () => {
  it("uses decimal MB and keeps a one-byte excess visible", () => {
    expect(
      explainError({
        code: "first_import_size_exceeded",
        incomingBytes: 100_000_001,
        limitBytes: 100_000_000,
      }),
    ).toContain("100.01 MB. Your first import cannot exceed 100 MB");
  });
});

describe("quota errors", () => {
  it("explains unavailable usage without suggesting payment", () => {
    const message = explainError({
      code: "quota_usage_unavailable",
      message: "internal diagnostic",
    });
    expect(message).toContain("Check access to your libraries");
    expect(message).toContain("still browse and clean up");
    expect(message).not.toContain("internal diagnostic");
  });
});
