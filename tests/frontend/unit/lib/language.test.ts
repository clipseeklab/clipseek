import { expect, it, vi } from "vitest";
import { waitFor } from "@testing-library/react";
import { useAppStore } from "@/stores/app";

it("starts in English on a Chinese system while retaining explicit language choices", async () => {
  const systemLanguage = vi.spyOn(navigator, "language", "get").mockReturnValue("zh-CN");
  try {
    vi.resetModules();
    const { default: i18n, detectSystemLanguage } = await import("@/i18n");
    await waitFor(() => expect(i18n.isInitialized).toBe(true));
    expect(i18n.language).toBe("en");
    expect(document.documentElement.lang).toBe("en");
    expect(useAppStore.getInitialState().settings.language).toBe("en");
    expect(detectSystemLanguage()).toBe("zh");
    await i18n.changeLanguage("zh");
    expect(i18n.t("settings.modelManagement")).toBe("模型管理");
    expect(document.documentElement.lang).toBe("zh-CN");
    await i18n.changeLanguage("en");
    expect(i18n.t("settings.modelManagement")).toBe("Model management");
  } finally {
    systemLanguage.mockRestore();
  }
});
