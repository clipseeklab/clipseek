import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen, waitFor } from "@test/helpers/render";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAppStore } from "@/stores/app";

const platform = vi.hoisted(() => ({ isWindows: true }));
vi.mock("@/lib/platform", () => ({
  get isWindows() {
    return platform.isWindows;
  },
}));
vi.mock("@/lib/api", () => ({
  api: {
    folderSidebar: vi.fn().mockResolvedValue({ items: [], total: 0 }),
    tagList: vi.fn().mockResolvedValue([]),
    assetList: vi.fn().mockResolvedValue({ items: [], total: 0, page: 1, hasMore: false }),
  },
}));
vi.mock("@/components/library/LibraryManageDialog", () => ({ default: () => null }));
vi.mock("@/components/settings/SettingsDialog", () => ({ default: () => null }));

import { api } from "@/lib/api";
import Sidebar from "@/components/layout/Sidebar";

describe("library selector window interaction", () => {
  beforeEach(() => {
    platform.isWindows = true;
    const library = { id: "library-2", name: "Library2", path: "/library", createdAt: 1 };
    useAppStore.setState({ current: library, libraries: [library] });
  });

  it("keeps the Windows selector interactive without a native window drag region", async () => {
    const { container } = renderWithProviders(<Sidebar />);
    expect(container.querySelector("[data-tauri-drag-region]")).toBeNull();
    await userEvent.setup().click(screen.getByRole("button", { name: "Library2" }));
    expect(await screen.findByRole("menu")).toBeInTheDocument();
  });

  it("retains the macOS library-row drag region", () => {
    platform.isWindows = false;
    const { container } = renderWithProviders(<Sidebar />);
    expect(container.querySelector("[data-tauri-drag-region]")).not.toBeNull();
  });
});

it("places an empty trash link beside settings outside the main navigation", async () => {
  const library = { id: "empty-library", name: "Empty", path: "/empty", createdAt: 1 };
  useAppStore.setState({ current: library, libraries: [library] });
  renderWithProviders(<Sidebar />);
  const trash = await screen.findByRole("link", { name: "Trash (empty)" });
  expect(trash.closest("nav")).toBeNull();
  expect(trash.parentElement).toContainElement(screen.getByRole("button", { name: "Settings" }));
  const tips = screen.getByRole("button", { name: "Usage tips" });
  expect(trash.parentElement).toContainElement(tips);
  expect(tips).toHaveAttribute("title", "Usage tips");
  expect(tips.textContent).toBe("");
});
it("shows the occupied trash state and count", async () => {
  const library = { id: "full-library", name: "Full", path: "/full", createdAt: 1 };
  useAppStore.setState({ current: library, libraries: [library] });
  vi.mocked(api.assetList).mockImplementation(async (filter) => ({
    items: [],
    total: filter.inTrash ? 3 : 0,
    page: 1,
    pageSize: 40,
    hasMore: false,
  }));
  renderWithProviders(<Sidebar />);
  expect(await screen.findByRole("link", { name: "Trash (3 items)" })).toHaveTextContent("3");
});
it("prevents native link drags from sidebar menus without cancelling file drag-over", async () => {
  const { fireEvent } = await import("@testing-library/react");
  renderWithProviders(<Sidebar />);
  const search = screen.getByRole("link", { name: "Search" });
  expect(search).toHaveAttribute("draggable", "false");
  expect(fireEvent.dragStart(search)).toBe(false);
  expect(fireEvent.dragStart(search.querySelector("svg")!)).toBe(false);
  expect(fireEvent.dragOver(search)).toBe(true);
  expect(screen.getByRole("link", { name: /Trash/ })).toHaveAttribute("draggable", "false");
});

it.each([undefined, 5, 30])(
  "shows only the configured number of pinned folders (%s)",
  async (displayLimit) => {
    const items = Array.from({ length: 30 }, (_, index) => ({
      id: index + 1,
      name: `Pinned folder ${index + 1}`,
      createdAt: 1,
      assetCount: 0,
    }));
    vi.mocked(api.folderSidebar).mockResolvedValueOnce({ items, total: 30, displayLimit });
    renderWithProviders(<Sidebar />);
    const limit = displayLimit ?? 10;
    expect(await screen.findByRole("link", { name: `Pinned folder ${limit} 0` })).toBeVisible();
    if (limit < 30)
      expect(
        screen.queryByRole("link", { name: `Pinned folder ${limit + 1} 0` }),
      ).not.toBeInTheDocument();
    expect(screen.getAllByRole("link", { name: /Pinned folder/ })).toHaveLength(limit);
    expect(screen.getByRole("button", { name: "Manage pinned folders" })).toBeVisible();
  },
);

it.each([0, 5])(
  "hides the pinned-folder heading and settings when no folders are pinned (total %s)",
  async (total) => {
    vi.mocked(api.folderSidebar).mockResolvedValueOnce({ items: [], total });
    renderWithProviders(<Sidebar />);
    await waitFor(() =>
      expect(screen.getByRole("link", { name: /^Folders/ })).toHaveTextContent(
        new RegExp(`Folders\\s*${total}$`),
      ),
    );
    expect(screen.queryByRole("button", { name: "Manage pinned folders" })).not.toBeInTheDocument();
    expect(screen.getAllByText("Folders")).toHaveLength(1);
  },
);
