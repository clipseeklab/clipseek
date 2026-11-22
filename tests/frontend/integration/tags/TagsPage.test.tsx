import { queryKeys } from "@/lib/queryKeys";
import { useAppStore } from "@/stores/app";
/** Tag grouping, filtering, renaming, and deletion confirmation coverage. */
import userEvent from "@testing-library/user-event";
import { fireEvent, screen, waitFor, within } from "@test/helpers/render";
import { renderWithProviders } from "@test/helpers/render";
import type { Tag } from "@/types";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  tagBatch: vi.fn(),
  tagList: vi.fn(),
  tagCreateMany: vi.fn(),
  tagRename: vi.fn(),
  tagDelete: vi.fn(),
  navigate: vi.fn(),
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  api: {
    targetOptions: async () => ({ items: [{ id: 2, name: "Beta" }], hasMore: false }),
    tagBatch: mocks.tagBatch,
    tagCreateMany: mocks.tagCreateMany,
    tagList: mocks.tagList,
    tagRename: mocks.tagRename,
    tagDelete: mocks.tagDelete,
  },
}));
vi.mock("@/components/Toaster", () => ({
  toast: { success: mocks.toastSuccess, error: mocks.toastError },
}));
vi.mock("react-router-dom", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react-router-dom")>();
  return { ...actual, useNavigate: () => mocks.navigate };
});

import TagsPage from "@/pages/TagsPage";

const tags: Tag[] = [
  { id: 1, name: "alpha", assetCount: 2, createdAt: 40 },
  { id: 2, name: "Beta", assetCount: 5, createdAt: 30 },
  { id: 3, name: "travel", assetCount: 1, createdAt: 20 },
  { id: 4, name: "设计", assetCount: 8, createdAt: 10 },
];

async function openTagMenu(name: string) {
  const chip = await screen.findByText(name);
  fireEvent.contextMenu(chip.closest('[class*="rounded-full"]')!);
  return await screen.findByRole("menu");
}

describe("TagsPage", () => {
  beforeEach(() => {
    mocks.tagList.mockResolvedValue(tags);
    mocks.tagRename.mockResolvedValue(undefined);
    mocks.tagDelete.mockResolvedValue(undefined);
  });

  it("groups tags by letter with non-ASCII names under #", async () => {
    const view = renderWithProviders(<TagsPage />);

    expect(await screen.findByText("alpha")).toBeInTheDocument();
    // Group headings live under section#tag-group-<letter>; localeCompare puts # before
    // letters.
    const groupIds = [...view.container.querySelectorAll("section[id^='tag-group-']")].map(
      (section) => section.id.replace("tag-group-", ""),
    );
    expect(groupIds).toEqual(["#", "A", "B", "T"]);
    expect(screen.getByText("设计")).toBeInTheDocument();
    expect(screen.getByText("4 tags")).toBeInTheDocument();
  });

  it("filters tags by keyword and reports no matches", async () => {
    const user = userEvent.setup();
    renderWithProviders(<TagsPage />);
    await screen.findByText("alpha");

    await user.type(screen.getByPlaceholderText("Search tags…"), "bet");
    expect(screen.getByText("Beta")).toBeInTheDocument();
    expect(screen.queryByText("alpha")).not.toBeInTheDocument();

    await user.clear(screen.getByPlaceholderText("Search tags…"));
    await user.type(screen.getByPlaceholderText("Search tags…"), "zzz");
    expect(screen.getByText("No matching assets")).toBeInTheDocument();
  });

  it("opens the tag's assets on click", async () => {
    const user = userEvent.setup();
    renderWithProviders(<TagsPage />);

    await user.click(await screen.findByText("travel"));

    expect(mocks.navigate).toHaveBeenCalledWith("/tag/travel");
  });

  it("renames a tag from its context menu", async () => {
    const user = userEvent.setup();
    const { queryClient } = renderWithProviders(<TagsPage />);
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");

    const menu = await openTagMenu("Beta");
    await user.click(within(menu).getByText("Rename"));

    const dialog = await screen.findByRole("dialog");
    const nameInput = within(dialog).getByRole("textbox");
    expect(nameInput).toHaveValue("Beta");
    expect(within(dialog).getByRole("button", { name: "Confirm" })).toBeDisabled();

    await user.clear(nameInput);
    await user.type(nameInput, "beta-v2");
    await user.click(within(dialog).getByRole("button", { name: "Confirm" }));

    await waitFor(() => expect(mocks.tagRename).toHaveBeenCalledWith(2, "beta-v2"));
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.tags.library(useAppStore.getState().current?.id),
    });
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.assets.library(useAppStore.getState().current?.id),
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("deletes a tag only after confirmation", async () => {
    const user = userEvent.setup();
    const { queryClient } = renderWithProviders(<TagsPage />);
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");

    const menu = await openTagMenu("设计");
    await user.click(within(menu).getByText("Delete"));

    const dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent(/removes it from 8 assets/);
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(mocks.tagDelete).not.toHaveBeenCalled();

    const again = await openTagMenu("设计");
    await user.click(within(again).getByText("Delete"));
    const confirm = await screen.findByRole("alertdialog");
    await user.click(within(confirm).getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(mocks.tagDelete).toHaveBeenCalledWith(4));
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.tags.library(useAppStore.getState().current?.id),
    });
    expect(mocks.toastSuccess).toHaveBeenCalledWith("Done");
  });

  it("reports delete failures and keeps the page usable", async () => {
    const user = userEvent.setup();
    mocks.tagDelete.mockRejectedValue(new Error("db locked"));
    renderWithProviders(<TagsPage />);

    const menu = await openTagMenu("travel");
    await user.click(within(menu).getByText("Delete"));
    const dialog = await screen.findByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledOnce());
    expect(screen.getByText("travel")).toBeInTheDocument();
  });

  it("shows the empty state when the library has no tags", async () => {
    mocks.tagList.mockResolvedValue([]);
    renderWithProviders(<TagsPage />);

    expect(await screen.findByText("No tags yet")).toBeInTheDocument();
  });
});

it("moves a tag's assets after choosing and confirming a target", async () => {
  mocks.tagList.mockResolvedValue(tags);
  mocks.tagBatch.mockResolvedValue(undefined);
  const user = userEvent.setup();
  renderWithProviders(<TagsPage />);
  const menu = await openTagMenu("alpha");
  await user.click(within(menu).getByRole("menuitem", { name: "Move assets" }));
  await user.click(await screen.findByRole("option", { name: "Beta" }));
  expect(mocks.tagBatch).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Confirm" }));
  await waitFor(() => expect(mocks.tagBatch).toHaveBeenCalledWith([1], 2));
});

it("requires confirmation for bulk deletion and does nothing on cancellation", async () => {
  mocks.tagList.mockResolvedValue(tags);
  mocks.tagBatch.mockReset().mockResolvedValue(undefined);
  const user = userEvent.setup();
  renderWithProviders(<TagsPage />);
  await screen.findByText("alpha");
  await user.click(screen.getByRole("button", { name: "Batch actions" }));
  await user.click(screen.getByText("alpha"));
  await user.click(screen.getByText("Beta"));
  await user.click(screen.getByRole("button", { name: "Delete" }));
  expect(screen.getByRole("dialog")).toHaveTextContent("Assets will be kept");
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(mocks.tagBatch).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Delete" }));
  await user.click(screen.getByRole("button", { name: "Confirm" }));
  await waitFor(() => expect(mocks.tagBatch).toHaveBeenCalledWith([1, 2], null));
});
it("preserves the target and selected tags after a move fails, and allows retry", async () => {
  mocks.tagList.mockResolvedValue(tags);
  mocks.tagBatch
    .mockReset()
    .mockRejectedValueOnce(new Error("move failed"))
    .mockResolvedValue(undefined);
  const user = userEvent.setup();
  renderWithProviders(<TagsPage />);
  const menu = await openTagMenu("alpha");
  await user.click(within(menu).getByRole("menuitem", { name: "Move assets" }));
  await user.click(await screen.findByRole("option", { name: "Beta" }));
  await user.click(screen.getByRole("button", { name: "Confirm" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("move failed");
  expect(screen.getByRole("option", { name: "Beta" })).toHaveAttribute("aria-selected", "true");
  await user.click(screen.getByRole("button", { name: "Confirm" }));
  await waitFor(() => expect(mocks.tagBatch).toHaveBeenCalledTimes(2));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
});
it("retains the batch-add draft after a failure and does not create blank input", async () => {
  mocks.tagList.mockResolvedValue(tags);
  mocks.tagCreateMany.mockReset().mockRejectedValue(new Error("cannot create"));
  const user = userEvent.setup();
  renderWithProviders(<TagsPage />);
  await user.click(screen.getByRole("button", { name: "Add tags" }));
  expect(screen.getByRole("button", { name: "Confirm" })).toBeDisabled();
  await user.type(screen.getByRole("textbox", { name: "Tag names" }), "new tag");
  await user.click(screen.getByRole("button", { name: "Confirm" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("cannot create");
  expect(screen.getByRole("textbox", { name: "Tag names" })).toHaveValue("new tag");
});

it("toggles all shown tags and preserves selections outside the search results", async () => {
  mocks.tagList.mockResolvedValue(tags);
  const user = userEvent.setup();
  renderWithProviders(<TagsPage />);
  await screen.findByText("alpha");
  await user.click(screen.getByRole("button", { name: "Batch actions" }));
  await user.click(screen.getByRole("button", { name: "Select all shown" }));
  expect(screen.getByRole("button", { name: "View selected tags (4)" })).toBeEnabled();
  await user.type(screen.getByPlaceholderText("Search tags…"), "Beta");
  await user.click(screen.getByRole("button", { name: "Deselect all shown" }));
  expect(screen.getByRole("checkbox", { name: "Beta" })).not.toBeChecked();
  expect(screen.getByRole("button", { name: "View selected tags (3)" })).toBeEnabled();
  await user.clear(screen.getByPlaceholderText("Search tags…"));
  expect(screen.getAllByRole("checkbox", { checked: true })).toHaveLength(3);
  await user.click(screen.getByRole("button", { name: "Select all shown" }));
  await user.click(screen.getByRole("button", { name: "Deselect all shown" }));
  expect(screen.getByRole("button", { name: "View selected tags (0)" })).toBeDisabled();
});
it("paginates selected tags, cancels individual items, and clears the selection", async () => {
  const all = Array.from({ length: 21 }, (_, i) => ({
    id: i + 101,
    name: `Tag ${String(i + 1).padStart(3, "0")}`,
    createdAt: 1,
    assetCount: 0,
  }));
  mocks.tagList.mockResolvedValue(all);
  mocks.tagBatch.mockReset().mockResolvedValue(undefined);
  const user = userEvent.setup();
  renderWithProviders(<TagsPage />);
  await screen.findByText("Tag 001");
  await user.click(screen.getByRole("button", { name: "Batch actions" }));
  await user.click(screen.getByRole("button", { name: "Select all shown" }));
  await user.click(screen.getByRole("button", { name: "View selected tags (21)" }));
  let dialog = screen.getByRole("dialog", { name: "Selected tags (21)" });
  expect(within(dialog).getAllByRole("button", { name: /Deselect Tag/ })).toHaveLength(20);
  expect(within(dialog).queryByText("Tag 021")).not.toBeInTheDocument();
  await user.click(within(dialog).getByRole("button", { name: "Next page" }));
  await user.click(within(dialog).getByRole("button", { name: "Deselect Tag 021" }));
  dialog = screen.getByRole("dialog", { name: "Selected tags (20)" });
  expect(within(dialog).getByRole("button", { name: "Previous page" })).toBeDisabled();
  expect(within(dialog).getByRole("button", { name: "Next page" })).toBeDisabled();
  await user.click(within(dialog).getByRole("button", { name: "Close" }));
  expect(screen.getByRole("checkbox", { name: "Tag 021" })).not.toBeChecked();
  await user.click(screen.getByRole("button", { name: "View selected tags (20)" }));
  await user.click(
    within(screen.getByRole("dialog")).getByRole("button", { name: "Clear selection" }),
  );
  expect(within(screen.getByRole("dialog")).getByText("No tags selected")).toBeVisible();
  await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Close" }));
  expect(screen.getAllByRole("checkbox", { checked: false })).toHaveLength(21);
});
it("moves only the tags retained in the selected list", async () => {
  const all = Array.from({ length: 3 }, (_, i) => ({
    id: 101 + i,
    name: `Selected ${i + 1}`,
    createdAt: 1,
    assetCount: 0,
  }));
  mocks.tagList.mockResolvedValue(all);
  mocks.tagBatch.mockReset().mockResolvedValue(undefined);
  const user = userEvent.setup();
  renderWithProviders(<TagsPage />);
  await screen.findByText("Selected 1");
  await user.click(screen.getByRole("button", { name: "Batch actions" }));
  await user.click(screen.getByRole("button", { name: "Select all shown" }));
  await user.click(screen.getByRole("button", { name: "View selected tags (3)" }));
  await user.click(
    within(screen.getByRole("dialog")).getByRole("button", { name: "Deselect Selected 2" }),
  );
  await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Close" }));
  expect(screen.getByRole("checkbox", { name: "Selected 2" })).not.toBeChecked();
  await user.click(screen.getByRole("button", { name: "Move assets" }));
  await user.click(await screen.findByRole("option", { name: "Beta" }));
  await user.click(screen.getByRole("button", { name: "Confirm" }));
  await waitFor(() => expect(mocks.tagBatch).toHaveBeenCalledWith([101, 103], 2));
});
it("keeps hidden selected tags visible in the dialog and clears selection on library switch", async () => {
  const { useAppStore } = await import("@/stores/app");
  const { act } = await import("@testing-library/react");
  mocks.tagList.mockResolvedValue(tags);
  const user = userEvent.setup();
  renderWithProviders(<TagsPage />);
  await screen.findByText("alpha");
  await user.click(screen.getByRole("button", { name: "Batch actions" }));
  await user.click(screen.getByRole("button", { name: "Select all shown" }));
  await user.type(screen.getByPlaceholderText("Search tags…"), "Beta");
  await user.click(screen.getByRole("button", { name: "View selected tags (4)" }));
  expect(within(screen.getByRole("dialog")).getByText("alpha")).toBeVisible();
  mocks.tagList.mockResolvedValue([
    { id: 1, name: "Other library tag", createdAt: 1, assetCount: 0 },
  ]);
  act(() => useAppStore.setState({ current: { id: "tags-other-library" } as never }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(await screen.findByText("Other library tag")).toBeVisible();
  expect(screen.queryByRole("button", { name: "View selected tags (4)" })).not.toBeInTheDocument();
  expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
});
