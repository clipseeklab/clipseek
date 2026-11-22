import { expect, it, vi } from "vitest";
import { waitFor } from "@testing-library/react";

it("reports initialization failure without leaving an unhandled rejection", async () => {
  const error = new Error("Translations unavailable");
  const report = vi.spyOn(console, "error").mockImplementation(() => {});
  vi.resetModules();
  const on = vi.fn();
  vi.doMock("i18next", () => ({
    default: { on, use: () => ({ init: () => Promise.reject(error) }) },
  }));
  try {
    await import("@/i18n");
    expect(on).toHaveBeenCalledWith("languageChanged", expect.any(Function));
    await waitFor(() =>
      expect(report).toHaveBeenCalledWith("Language initialization failed", error),
    );
  } finally {
    vi.doUnmock("i18next");
    report.mockRestore();
  }
});
