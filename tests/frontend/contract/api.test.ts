// Contract tests assert the renderer-to-Rust IPC boundary.
import { mockIPC } from "@tauri-apps/api/mocks";
import { describe, expect, it, vi } from "vitest";
import { api } from "@/lib/api";

describe("Tauri API wrapper", () => {
  it("reads asset metadata using the asset id without accepting arbitrary source paths", async () => {
    const handler = vi.fn(() => ({ format: "mp4", tags: [], streams: [] }));
    mockIPC(handler);
    await api.assetMetadata(7);
    expect(handler).toHaveBeenLastCalledWith("asset_metadata", { id: 7 });
  });
  it("scopes processing queue reads and actions to the originating library", async () => {
    const handler = vi.fn(() => null);
    mockIPC(handler);
    await api.processingList("library-a", { status: "attention", page: 2, pageSize: 12 });
    expect(handler).toHaveBeenLastCalledWith("processing_list", {
      libraryId: "library-a",
      filter: { status: "attention", page: 2, pageSize: 12 },
    });
    await api.processingSummary("library-a");
    expect(handler).toHaveBeenLastCalledWith("processing_summary", { libraryId: "library-a" });
    await api.processingHistory("library-a", "asset:7");
    expect(handler).toHaveBeenLastCalledWith("processing_history", {
      libraryId: "library-a",
      key: "asset:7",
      before: null,
    });
    await api.processingHistory("library-a", "asset:7", 42);
    expect(handler).toHaveBeenLastCalledWith("processing_history", {
      libraryId: "library-a",
      key: "asset:7",
      before: 42,
    });
    await api.processingRetry("library-a", 8);
    expect(handler).toHaveBeenLastCalledWith("processing_retry", { libraryId: "library-a", id: 8 });
    await api.processingCancel("library-a", 8);
    expect(handler).toHaveBeenLastCalledWith("processing_cancel", {
      libraryId: "library-a",
      id: 8,
    });
    await api.processingHide("library-a", "item");
    expect(handler).toHaveBeenLastCalledWith("processing_hide", {
      libraryId: "library-a",
      key: "item",
    });
    await api.processingClear("library-a");
    expect(handler).toHaveBeenLastCalledWith("processing_clear", { libraryId: "library-a" });
  });

  it("does not invent a payload for a command without arguments", async () => {
    const handler = vi.fn(() => ({ ready: true }));
    mockIPC(handler);

    await api.modelStatus();

    expect(handler).toHaveBeenCalledWith("model_status", {});
  });
});

it("passes folder pagination and library-local sidebar preferences through IPC", async () => {
  const handler = vi.fn();
  mockIPC(handler);
  await api.folderPage("旅行", 2);
  expect(handler).toHaveBeenLastCalledWith("folder_page", {
    query: "旅行",
    page: 2,
    sort: "created_desc",
  });
  await api.folderPage("", 1, "name_desc");
  expect(handler).toHaveBeenLastCalledWith("folder_page", {
    query: "",
    page: 1,
    sort: "name_desc",
  });
  await api.folderGet(80);
  expect(handler).toHaveBeenLastCalledWith("folder_get", { id: 80 });
  await api.folderSidebar();
  expect(handler).toHaveBeenLastCalledWith("folder_sidebar", {});
  await api.folderSidebarSet("lib-a", [80, 1]);
  expect(handler).toHaveBeenLastCalledWith("folder_sidebar_set", {
    libraryId: "lib-a",
    ids: [80, 1],
  });
  await api.folderSidebarSet("lib-a", [80, 1], 30);
  expect(handler).toHaveBeenLastCalledWith("folder_sidebar_set", {
    libraryId: "lib-a",
    ids: [80, 1],
    displayLimit: 30,
  });
  await api.folderPin("lib-a", 80, false);
  expect(handler).toHaveBeenLastCalledWith("folder_pin", {
    libraryId: "lib-a",
    id: 80,
    pinned: false,
  });
});

it("preserves flat general settings and explicit model selection payloads", async () => {
  const handler = vi.fn();
  mockIPC(handler);
  await api.settingsSet({ theme: "dark", importMode: "reference" });
  expect(handler).toHaveBeenLastCalledWith("settings_set", {
    theme: "dark",
    importMode: "reference",
  });
  await api.modelSettings("q4", "/models");
  expect(handler).toHaveBeenLastCalledWith("model_settings", {
    precision: "q4",
    modelDirectory: "/models",
  });
});
