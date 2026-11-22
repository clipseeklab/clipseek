import { useAppStore } from "@/stores/app";
import { queryKeys } from "@/lib/queryKeys";
/** Folder CRUD, confirmation, and targeted-import coverage. */
import userEvent from "@testing-library/user-event";
import { screen, waitFor, within } from "@test/helpers/render";
import { renderWithProviders } from "@test/helpers/render";
import { useImportStore } from "@/stores/import";
import type { Asset, Folder } from "@/types";
import { Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  folderBatch: vi.fn(),
  folderList: vi.fn(),
  folderCreate: vi.fn(),
  folderRename: vi.fn(),
  folderDelete: vi.fn(),
  useAssetPages: vi.fn(),
  openFileDialog: vi.fn(),
  navigate: vi.fn(),
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  api: {
    targetOptions: async () => ({ items: [{ id: 8, name: "Sketches" }], hasMore: false }),
    folderBatch: mocks.folderBatch,
    folderPage: async (query: string, page: number) => {
      const all = (await mocks.folderList()).filter((f: Folder) =>
        f.name.toLowerCase().includes(query.toLowerCase()),
      );
      return {
        items: all.slice((page - 1) * 40, page * 40),
        total: all.length,
        page,
        hasMore: page * 40 < all.length,
      };
    },
    folderGet: async (id: number) =>
      (await mocks.folderList()).find((f: Folder) => f.id === id) ?? null,
    folderSidebar: async () => ({ items: [], total: (await mocks.folderList()).length }),
    folderCreate: mocks.folderCreate,
    folderRename: mocks.folderRename,
    folderDelete: mocks.folderDelete,
    assetStorageStats: vi.fn(),
    assetTrash: vi.fn(),
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.openFileDialog }));
vi.mock("@/components/Toaster", () => ({
  toast: { success: mocks.toastSuccess, error: mocks.toastError },
}));
vi.mock("react-router-dom", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react-router-dom")>();
  return { ...actual, useNavigate: () => mocks.navigate };
});
vi.mock("@/hooks/useAssetPages", () => ({ useAssetPages: mocks.useAssetPages }));
vi.mock("@/components/assets/VirtualAssetGrid", () => ({
  default: ({
    items,
    renderItem,
  }: {
    items: Asset[];
    renderItem: (asset: Asset) => React.ReactNode;
  }) => <div data-testid="asset-grid">{items.map((asset) => renderItem(asset))}</div>,
}));
vi.mock("@/components/assets/MasonryGrid", () => ({ default: () => null }));
vi.mock("@/components/assets/AssetCard", () => ({ default: () => null }));
vi.mock("@/components/assets/GridSizeControl", () => ({ default: () => null }));
vi.mock("@/components/tasks/TaskQueueButton", () => ({ default: () => null }));
vi.mock("@/components/assets/AssetFilters", async (importOriginal) => {
  const original = await importOriginal<typeof import("@/components/assets/AssetFilters")>();
  return { ...original, AssetFilters: () => null };
});
vi.mock("@/components/assets/AssetDialogs", () => ({
  AddTagsDialog: () => null,
  AssetDetailPanel: () => null,
  AssetSourcePreview: () => null,
  MoveToFolderDialog: () => null,
  MoveToLibraryDialog: () => null,
}));

import FoldersPage from "@/pages/FoldersPage";
import { stubConvertFileSrc } from "@test/helpers/tauri";

const folders: Folder[] = [
  { id: 7, name: "Posters", createdAt: 1, assetCount: 3 },
  { id: 8, name: "Sketches", createdAt: 2, assetCount: 0 },
];

function pageResult(overrides: Record<string, unknown> = {}) {
  return {
    items: [],
    total: 0,
    isLoading: false,
    isError: false,
    isFetchingNextPage: false,
    fetchNextPage: vi.fn(),
    hasNextPage: false,
    refetch: vi.fn(),
    ...overrides,
  };
}

describe("FoldersPage list", () => {
  beforeEach(() => {
    stubConvertFileSrc();
    useImportStore.setState(useImportStore.getInitialState(), true);
    mocks.folderList.mockResolvedValue(folders);
    mocks.folderCreate.mockResolvedValue(folders[0]);
    mocks.folderRename.mockResolvedValue(undefined);
    mocks.folderDelete.mockResolvedValue(undefined);
    mocks.useAssetPages.mockReturnValue(pageResult());
  });

  it("renders folders with their asset counts and restores the drop mode on unmount", async () => {
    const view = renderWithProviders(<FoldersPage />, { route: "/folders" });

    expect(await screen.findByText("Posters")).toBeInTheDocument();
    expect(screen.getByText("3 assets")).toBeInTheDocument();
    expect(screen.getByText("Sketches")).toBeInTheDocument();
    expect(useImportStore.getState().dropMode).toBe("folder-import");

    view.unmount();
    expect(useImportStore.getState().dropMode).toBe("import");
  });

  it("shows the empty state when no folders exist", async () => {
    mocks.folderList.mockResolvedValue([]);
    renderWithProviders(<FoldersPage />, { route: "/folders" });

    expect(await screen.findByText("No folders yet")).toBeInTheDocument();
    // Both the header and empty state expose entry buttons.
    expect(screen.getAllByRole("button", { name: "New Folder" })).toHaveLength(2);
  });

  it("creates a folder through the dialog and refreshes the list", async () => {
    const user = userEvent.setup();
    const { queryClient } = renderWithProviders(<FoldersPage />, { route: "/folders" });
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");

    await user.click(screen.getByRole("button", { name: "New Folder" }));
    await user.type(screen.getByPlaceholderText("Folder name"), "Posters");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Confirm" }));

    await waitFor(() => expect(mocks.folderCreate).toHaveBeenCalledWith("Posters"));
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.folders.library(useAppStore.getState().current?.id),
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("keeps the creation dialog open and reports the error on failure", async () => {
    const user = userEvent.setup();
    mocks.folderCreate.mockRejectedValueOnce({
      code: "folder_duplicate",
      message: "UNIQUE constraint failed: folders.name",
    });
    renderWithProviders(<FoldersPage />, { route: "/folders" });

    await user.click(screen.getByRole("button", { name: "New Folder" }));
    await user.type(screen.getByPlaceholderText("Folder name"), "Posters");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Confirm" }));

    await waitFor(() =>
      expect(mocks.toastError).toHaveBeenCalledWith(
        "A folder with this name already exists. Choose a different name.",
      ),
    );
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Folder name")).toHaveValue("Posters");
    await user.clear(screen.getByPlaceholderText("Folder name"));
    await user.type(screen.getByPlaceholderText("Folder name"), "New Posters");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(mocks.folderCreate).toHaveBeenLastCalledWith("New Posters");
  });

  it("renames a folder and refreshes folders and assets", async () => {
    const user = userEvent.setup();
    const { queryClient } = renderWithProviders(<FoldersPage />, { route: "/folders" });
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");
    await screen.findByText("Posters");

    await user.click(screen.getAllByTitle("Rename")[0]);
    const input = within(await screen.findByRole("dialog")).getByRole("textbox");
    expect(input).toHaveValue("Posters");
    await user.clear(input);
    await user.type(input, "Key Art");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Confirm" }));

    await waitFor(() => expect(mocks.folderRename).toHaveBeenCalledWith(7, "Key Art"));
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.folders.library(useAppStore.getState().current?.id),
    });
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.assets.library(useAppStore.getState().current?.id),
    });
  });

  it("deletes a folder only after confirmation", async () => {
    const user = userEvent.setup();
    const { queryClient } = renderWithProviders(<FoldersPage />, { route: "/folders" });
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");
    await screen.findByText("Posters");

    await user.click(screen.getAllByTitle("Delete Folder")[0]);
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent(/moves its 3 assets to trash/);

    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(mocks.folderDelete).not.toHaveBeenCalled();

    await user.click(screen.getAllByTitle("Delete Folder")[0]);
    const confirm = await screen.findByRole("alertdialog");
    await user.click(within(confirm).getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(mocks.folderDelete).toHaveBeenCalledWith(7));
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.folders.library(useAppStore.getState().current?.id),
    });
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.assets.library(useAppStore.getState().current?.id),
    });
    expect(mocks.toastSuccess).toHaveBeenCalledWith("Done");
  });

  it("navigates into a folder when its card is clicked", async () => {
    const user = userEvent.setup();
    renderWithProviders(<FoldersPage />, { route: "/folders" });

    await user.click(await screen.findByText("Posters"));

    expect(mocks.navigate).toHaveBeenCalledWith("/folders/7");
  });
});

describe("FoldersPage single folder", () => {
  beforeEach(() => {
    useImportStore.setState(useImportStore.getInitialState(), true);
    mocks.folderList.mockResolvedValue(folders);
    mocks.useAssetPages.mockReturnValue(pageResult());
    mocks.openFileDialog.mockResolvedValue(["/media/hero.png"]);
  });

  it("targets imports at the folder and records the drop folder", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <Routes>
        <Route path="/folders/:folderId" element={<FoldersPage />} />
      </Routes>,
      { route: "/folders/7" },
    );

    expect(useImportStore.getState().dropFolderId).toBe(7);
    await user.click(screen.getAllByRole("button", { name: "Import Assets" })[0]);

    await waitFor(() => expect(useImportStore.getState().tasks).toHaveLength(1));
    expect(useImportStore.getState().tasks[0]).toMatchObject({
      paths: ["/media/hero.png"],
      folderId: 7,
    });
    expect(mocks.useAssetPages).toHaveBeenCalledWith(expect.objectContaining({ folderId: 7 }));
  });

  it("shows an error for an unknown folder id", async () => {
    renderWithProviders(
      <Routes>
        <Route path="/folders/:folderId" element={<FoldersPage />} />
      </Routes>,
      { route: "/folders/404" },
    );

    expect(await screen.findByText("Error")).toBeInTheDocument();
  });
});

it("moves selected folders after target confirmation", async () => {
  mocks.folderList.mockResolvedValue(folders);
  mocks.folderBatch.mockResolvedValue(undefined);
  const user = userEvent.setup();
  renderWithProviders(<FoldersPage />);
  await screen.findByText("Posters");
  await user.click(screen.getByRole("button", { name: "Batch actions" }));
  await user.click(screen.getByText("Posters"));
  await user.click(screen.getByRole("button", { name: "Move assets" }));
  await user.click(await screen.findByRole("option", { name: "Sketches" }));
  expect(mocks.folderBatch).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Confirm" }));
  await waitFor(() => expect(mocks.folderBatch).toHaveBeenCalledWith([7], 8));
});

it("offers card movement and hides individual actions in selection mode", async () => {
  mocks.folderList.mockResolvedValue(folders);
  const user = userEvent.setup();
  renderWithProviders(<FoldersPage />);
  await screen.findByText("Posters");
  await user.click(screen.getAllByTitle("Move assets")[0]);
  expect(screen.getByRole("textbox", { name: "Target folder" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  await user.click(screen.getByRole("button", { name: "Batch actions" }));
  expect(screen.queryByTitle("Move assets")).not.toBeInTheDocument();
  expect(screen.queryByTitle("Rename")).not.toBeInTheDocument();
  expect(screen.queryByTitle("Delete Folder")).not.toBeInTheDocument();
});

it("confirms bulk deletion with the Trash policy and supports canceling", async () => {
  mocks.folderList.mockResolvedValue(folders);
  mocks.folderBatch.mockReset().mockResolvedValue(undefined);
  const user = userEvent.setup();
  renderWithProviders(<FoldersPage />);
  await screen.findByText("Posters");
  await user.click(screen.getByRole("button", { name: "Batch actions" }));
  await user.click(screen.getByRole("button", { name: "Select this page" }));
  await user.click(screen.getByRole("button", { name: "Delete" }));
  expect(screen.getByRole("dialog")).toHaveTextContent("move their assets to Trash");
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(mocks.folderBatch).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Delete" }));
  await user.click(screen.getByRole("button", { name: "Confirm" }));
  await waitFor(() => expect(mocks.folderBatch).toHaveBeenCalledWith([7, 8], null));
});
it("keeps failed folder movement retryable without navigating away", async () => {
  mocks.folderList.mockResolvedValue(folders);
  mocks.folderBatch
    .mockReset()
    .mockRejectedValueOnce(new Error("folder move failed"))
    .mockResolvedValue(undefined);
  const user = userEvent.setup();
  renderWithProviders(<FoldersPage />);
  await screen.findByText("Posters");
  await user.click(screen.getAllByTitle("Move assets")[0]);
  await user.click(await screen.findByRole("option", { name: "Sketches" }));
  await user.click(screen.getByRole("button", { name: "Confirm" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("folder move failed");
  await user.click(screen.getByRole("button", { name: "Confirm" }));
  await waitFor(() => expect(mocks.folderBatch).toHaveBeenCalledTimes(2));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
});

it("paginates forty folders and limits batch selection to the visible page", async () => {
  mocks.folderList.mockResolvedValue(
    Array.from({ length: 85 }, (_, i) => ({
      id: i + 1,
      name: `Folder ${String(i).padStart(3, "0")}`,
      createdAt: 1,
      assetCount: 0,
    })),
  );
  const user = userEvent.setup();
  renderWithProviders(<FoldersPage />);
  await screen.findByText("Folder 000");
  expect(screen.queryByText("Folder 040")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Next page" }));
  await screen.findByText("Folder 040");
  expect(screen.queryByText("Folder 000")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Batch actions" }));
  await user.click(screen.getByRole("button", { name: "Select this page" }));
  expect(screen.getAllByRole("checkbox", { checked: true })).toHaveLength(40);
  await user.type(screen.getByRole("textbox", { name: "Search folders…" }), "084");
  await screen.findByText("Folder 084");
  expect(screen.getAllByRole("checkbox", { checked: false })).toHaveLength(1);
  expect(screen.getByRole("button", { name: "Previous page" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Next page" })).toBeDisabled();
});

it("allows pinning at capacity and refreshes the front-inserted sidebar order", async () => {
  const { api } = await import("@/lib/api");
  const { useAppStore } = await import("@/stores/app");
  useAppStore.setState({ current: { id: "pin-library" } as never });
  const all = Array.from({ length: 21 }, (_, i) => ({
    id: i + 1,
    name: `Folder ${i + 1}`,
    createdAt: 1,
    assetCount: 0,
  }));
  mocks.folderList.mockResolvedValue(all);
  const sidebar = vi
    .spyOn(api, "folderSidebar")
    .mockResolvedValue({ items: all.slice(0, 20), total: 21 });
  const pin = vi.fn(async () => {
    const next = { items: [all[20], ...all.slice(0, 19)], total: 21 };
    sidebar.mockResolvedValue(next);
    return next;
  });
  Object.assign(api, { folderPin: pin });
  const user = userEvent.setup();
  renderWithProviders(<FoldersPage />);
  const name = await screen.findByText("Folder 21");
  const card = name.parentElement!;
  const button = within(card).getByTitle("Pin to sidebar");
  expect(button).toBeEnabled();
  await user.click(button);
  await waitFor(() => expect(pin).toHaveBeenCalledWith("pin-library", 21, true));
  await waitFor(() => expect(within(card).getByTitle("Unpin")).toBeEnabled());
  expect(screen.getByText("Folder 20")).toBeInTheDocument();
  sidebar.mockRestore();
});

it("toggles selecting the current page without clearing selections on other pages", async () => {
  mocks.folderList.mockResolvedValue(
    Array.from({ length: 41 }, (_, i) => ({
      id: i + 1,
      name: `Folder ${String(i + 1).padStart(3, "0")}`,
      createdAt: 1,
      assetCount: 0,
    })),
  );
  const user = userEvent.setup();
  renderWithProviders(<FoldersPage />);
  await screen.findByText("Folder 001");
  await user.click(screen.getByRole("button", { name: "Batch actions" }));
  await user.click(screen.getByRole("button", { name: "Select this page" }));
  expect(screen.getByRole("button", { name: "View selected folders (40)" })).toBeEnabled();
  await user.click(screen.getByRole("button", { name: "Next page" }));
  await screen.findByText("Folder 041");
  await user.click(screen.getByRole("button", { name: "Select this page" }));
  expect(screen.getByRole("button", { name: "View selected folders (41)" })).toBeEnabled();
  await user.click(screen.getByRole("button", { name: "Deselect this page" }));
  expect(screen.getByRole("button", { name: "View selected folders (40)" })).toBeEnabled();
  await user.click(screen.getByRole("button", { name: "Previous page" }));
  await screen.findByText("Folder 001");
  expect(screen.getAllByRole("checkbox", { checked: true })).toHaveLength(40);
  await user.click(screen.getByRole("button", { name: "Deselect this page" }));
  expect(screen.getByRole("button", { name: "View selected folders (0)" })).toBeDisabled();
});

it("pages through selected folders, removes entries, and clears selection", async () => {
  mocks.folderList.mockResolvedValue(
    Array.from({ length: 40 }, (_, i) => ({
      id: i + 1,
      name: `Folder ${String(i + 1).padStart(3, "0")}`,
      createdAt: 1,
      assetCount: 0,
    })),
  );
  const user = userEvent.setup();
  renderWithProviders(<FoldersPage />);
  await screen.findByText("Folder 001");
  await user.click(screen.getByRole("button", { name: "Batch actions" }));
  await user.click(screen.getByRole("button", { name: "Select this page" }));
  await user.click(screen.getByRole("button", { name: "View selected folders (40)" }));
  const dialog = screen.getByRole("dialog", { name: "Selected folders (40)" });
  expect(await within(dialog).findByText("Folder 001")).toBeVisible();
  expect(within(dialog).getAllByRole("button", { name: /Deselect Folder/ })).toHaveLength(20);
  expect(within(dialog).queryByText("Folder 021")).not.toBeInTheDocument();
  await user.click(within(dialog).getByRole("button", { name: "Next page" }));
  await user.click(await within(dialog).findByRole("button", { name: "Deselect Folder 021" }));
  expect(screen.getByRole("dialog", { name: "Selected folders (39)" })).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Clear selection" }));
  expect(await within(dialog).findByText("No folders selected")).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Close" }));
  expect(screen.getAllByRole("checkbox", { checked: false })).toHaveLength(40);
});
it("clamps the selected-list page after removal and moves only the remaining folders", async () => {
  const { api } = await import("@/lib/api");
  const get = vi.spyOn(api, "folderGet");
  const all = Array.from({ length: 21 }, (_, i) => ({
    id: 101 + i,
    name: `Selected ${String(i + 1).padStart(3, "0")}`,
    createdAt: 1,
    assetCount: 0,
  }));
  mocks.folderList.mockResolvedValue(all);
  mocks.folderBatch.mockReset().mockResolvedValue(undefined);
  const user = userEvent.setup();
  renderWithProviders(<FoldersPage />);
  await screen.findByText("Selected 001");
  await user.click(screen.getByRole("button", { name: "Batch actions" }));
  await user.click(screen.getByRole("button", { name: "Select this page" }));
  await user.click(screen.getByRole("button", { name: "View selected folders (21)" }));
  let dialog = screen.getByRole("dialog");
  await within(dialog).findByText("Selected 001");
  expect(get).toHaveBeenCalledTimes(20);
  await user.click(within(dialog).getByRole("button", { name: "Next page" }));
  await user.click(await within(dialog).findByRole("button", { name: "Deselect Selected 021" }));
  dialog = screen.getByRole("dialog", { name: "Selected folders (20)" });
  expect(within(dialog).getByRole("button", { name: "Previous page" })).toBeDisabled();
  expect(within(dialog).getByRole("button", { name: "Next page" })).toBeDisabled();
  await user.click(within(dialog).getByRole("button", { name: "Close" }));
  expect(screen.getByRole("checkbox", { name: "Selected 021" })).not.toBeChecked();
  await user.click(screen.getByRole("button", { name: "Move assets" }));
  await user.click(await screen.findByRole("option", { name: "Sketches" }));
  await user.click(screen.getByRole("button", { name: "Confirm" }));
  await waitFor(() =>
    expect(mocks.folderBatch).toHaveBeenCalledWith(
      all.slice(0, 20).map((item) => item.id),
      8,
    ),
  );
  get.mockRestore();
});
it("retains selections through search and clears them on a library switch", async () => {
  const { useAppStore } = await import("@/stores/app");
  const { act } = await import("@testing-library/react");
  mocks.folderList.mockResolvedValue(folders);
  const user = userEvent.setup();
  renderWithProviders(<FoldersPage />);
  await screen.findByText("Posters");
  await user.click(screen.getByRole("button", { name: "Batch actions" }));
  await user.click(screen.getByRole("button", { name: "Select this page" }));
  await user.type(screen.getByRole("textbox", { name: "Search folders…" }), "Sketches");
  await waitFor(() => expect(screen.queryByText("Posters")).not.toBeInTheDocument());
  expect(await screen.findByRole("checkbox", { name: "Sketches" })).toBeChecked();
  await user.click(screen.getByRole("button", { name: "View selected folders (2)" }));
  expect(await within(screen.getByRole("dialog")).findByText("Posters")).toBeVisible();
  act(() => useAppStore.setState({ current: { id: "selection-other-library" } as never }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(
    screen.queryByRole("button", { name: "View selected folders (2)" }),
  ).not.toBeInTheDocument();
});

it("defaults to newest folders, offers four exclusive sorts, resets pagination and remembers the choice", async () => {
  const { api } = await import("@/lib/api");
  const calls = vi
    .spyOn(api, "folderPage")
    .mockResolvedValue({ items: folders, total: 41, page: 1, hasMore: true });
  const user = userEvent.setup();
  const view = renderWithProviders(<FoldersPage />, { route: "/folders" });
  await screen.findByText("Posters");
  expect(calls).toHaveBeenLastCalledWith("", 1, "created_desc");
  await user.click(screen.getByRole("button", { name: "Next page" }));
  await waitFor(() => expect(calls).toHaveBeenLastCalledWith("", 2, "created_desc"));
  await user.click(screen.getByRole("button", { name: "Newest first" }));
  expect(screen.getAllByRole("menuitemradio")).toHaveLength(4);
  expect(screen.getByRole("menuitemradio", { name: "Newest first" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await user.click(screen.getByRole("menuitemradio", { name: "Name A–Z" }));
  await waitFor(() => expect(calls).toHaveBeenLastCalledWith("", 1, "name_asc"));
  expect(localStorage.getItem("clipseek:folder-sort")).toBe("name_asc");
  view.unmount();
  renderWithProviders(<FoldersPage />, { route: "/folders" });
  expect(screen.getByRole("button", { name: "Name A–Z" })).toBeVisible();
  for (const [label, value] of [
    ["Name Z–A", "name_desc"],
    ["Oldest first", "created_asc"],
    ["Newest first", "created_desc"],
  ]) {
    await user.click(screen.getByRole("button", { name: /Name A–Z|Name Z–A|Oldest first/ }));
    await user.click(screen.getByRole("menuitemradio", { name: label }));
    await waitFor(() => expect(calls).toHaveBeenLastCalledWith("", 1, value));
  }
});

it("keeps current folders visible while a new sort loads and preserves the search and selected folders", async () => {
  const { api } = await import("@/lib/api");
  let resolveSort!: (value: {
    items: Folder[];
    total: number;
    page: number;
    hasMore: boolean;
  }) => void;
  vi.spyOn(api, "folderPage").mockImplementation(async (query, _page, sort) => {
    if (sort === "name_desc")
      return new Promise((resolve) => {
        resolveSort = resolve;
      });
    return {
      items: folders.filter((f) => f.name.toLowerCase().includes(query.toLowerCase())),
      total: 2,
      page: 1,
      hasMore: false,
    };
  });
  const user = userEvent.setup();
  renderWithProviders(<FoldersPage />, { route: "/folders" });
  await screen.findByText("Posters");
  await user.click(screen.getByRole("button", { name: "Batch actions" }));
  await user.click(screen.getByText("Posters"));
  await user.type(screen.getByRole("textbox", { name: "Search folders…" }), "post");
  await waitFor(() => expect(screen.queryByText("Sketches")).not.toBeInTheDocument());
  await user.click(screen.getByRole("button", { name: "Newest first" }));
  await user.click(screen.getByRole("menuitemradio", { name: "Name Z–A" }));
  expect(screen.getByText("Posters")).toBeVisible();
  expect(screen.getByRole("checkbox", { name: "Posters" })).toBeChecked();
  expect(screen.getByRole("textbox", { name: "Search folders…" })).toHaveValue("post");
  const { act } = await import("@testing-library/react");
  await act(async () => resolveSort({ items: [folders[0]], total: 1, page: 1, hasMore: false }));
  expect(screen.getByRole("checkbox", { name: "Posters" })).toBeChecked();
});
