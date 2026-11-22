import { act, renderWithProviders, screen, waitFor } from "@test/helpers/render";
import { QueryClient } from "@tanstack/react-query";
import { useAssetPages } from "@/hooks/useAssetPages";
import { useAppStore } from "@/stores/app";
import { buildAsset } from "@test/fixtures/assets";
import type { LibraryMeta } from "@/types";
import { beforeEach, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
  blockedLibrary: null as string | null,
  pending: [] as (() => void)[],
}));

vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tauri-apps/api/core")>()),
  invoke: mocks.invoke,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((name: string, callback: (event: { payload: unknown }) => void) => {
    mocks.listeners.set(name, callback);
    return Promise.resolve(() => mocks.listeners.delete(name));
  }),
}));
vi.mock("@/components/layout/AppLayout", () => ({
  default: function AssetList() {
    const { items } = useAssetPages({});
    return <div>{items.map((asset) => asset.fileName).join(",")}</div>;
  },
}));

import App from "@/App";

const libraryA: LibraryMeta = { id: "library-a", name: "A", path: "/a", createdAt: 1 };
const libraryB: LibraryMeta = { id: "library-b", name: "B", path: "/b", createdAt: 2 };

beforeEach(() => {
  mocks.listeners.clear();
  mocks.blockedLibrary = null;
  mocks.pending = [];
  useAppStore.setState({ current: null, loaded: false });
  mocks.invoke.mockImplementation(async (command, _args, options) => {
    switch (command) {
      case "model_status":
        return { ready: true };
      case "library_list":
        return [libraryA, libraryB];
      case "library_current":
      case "library_switch":
        return libraryA;
      case "settings_get":
        return { ...useAppStore.getState().settings, autoUpdateCheck: false, language: "en" };
      case "onboarding_get":
        return { version: 2, currentStep: 3, completed: true };
      case "asset_list": {
        const libraryId =
          options?.headers?.["x-clipseek-library"] ?? useAppStore.getState().current?.id;
        if (libraryId === mocks.blockedLibrary)
          await new Promise<void>((resolve) => mocks.pending.push(resolve));
        return {
          items: [buildAsset({ id: 1, fileName: `${libraryId}.png` })],
          total: 1,
          page: 1,
          pageSize: 40,
          hasMore: false,
        };
      }
      default:
        throw new Error(`Unexpected IPC command: ${command}`);
    }
  });
});

it("does not show another library's assets while switching back to a cached library", async () => {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: 1, refetchOnWindowFocus: false, staleTime: 5_000 },
    },
  });
  renderWithProviders(<App />, { queryClient });
  await screen.findByText("library-a.png");

  act(() => mocks.listeners.get("library://switched")?.({ payload: libraryB }));
  await screen.findByText("library-b.png");
  await waitFor(() => expect(queryClient.isFetching()).toBe(0));

  expect(queryClient.getQueryData(["assets", libraryA.id, { tags: [] }])).toMatchObject({
    pages: [{ items: [{ fileName: "library-a.png" }] }],
  });
  mocks.blockedLibrary = libraryA.id;
  try {
    act(() => mocks.listeners.get("library://switched")?.({ payload: libraryA }));
    expect(screen.queryByText("library-b.png")).not.toBeInTheDocument();
  } finally {
    mocks.blockedLibrary = null;
    await act(async () => {
      for (const resolve of mocks.pending) resolve();
    });
  }
});

it("refreshes the snapshot when the current library is reopened with the same id", async () => {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: 1, staleTime: 5_000 } },
  });
  renderWithProviders(<App />, { queryClient });
  await screen.findByText("library-a.png");
  await waitFor(() => expect(queryClient.isFetching()).toBe(0));
  const before = mocks.invoke.mock.calls.filter(([command]) => command === "asset_list").length;
  act(() => mocks.listeners.get("library://switched")?.({ payload: libraryA }));
  await waitFor(() => {
    expect(
      mocks.invoke.mock.calls.filter(([command]) => command === "asset_list").length,
    ).toBeGreaterThan(before);
  });
  expect(screen.queryByText("library-b.png")).not.toBeInTheDocument();
});
