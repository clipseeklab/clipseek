import { beforeEach, expect, it, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen, waitFor } from "@test/helpers/render";
import FolderPicker from "@/components/folders/FolderPicker";
import FolderSidebarDialog from "@/components/folders/FolderSidebarDialog";
import { useAppStore } from "@/stores/app";
const mocks = vi.hoisted(() => ({
  page: vi.fn(),
  get: vi.fn(),
  sidebar: vi.fn(),
  save: vi.fn(),
  targets: vi.fn(),
}));
vi.mock("@/lib/api", () => ({
  api: {
    folderPage: mocks.page,
    folderGet: mocks.get,
    folderSidebar: mocks.sidebar,
    folderSidebarSet: mocks.save,
    targetOptions: mocks.targets,
  },
}));
beforeEach(() => {
  vi.resetAllMocks();
  mocks.page.mockResolvedValue({ items: [], total: 0, page: 1, hasMore: false });
  mocks.get.mockResolvedValue({ id: 99, name: "Selected elsewhere" });
  mocks.sidebar.mockResolvedValue({ items: [{ id: 2, name: "Pinned" }], total: 100 });
  mocks.targets.mockResolvedValue({ items: [{ id: 3, name: "Add me" }], hasMore: false });
  useAppStore.setState({ current: { id: "test-library" } as never });
});
it("loads bounded pages on demand, keeps selected and pinned folders visible, and searches remotely", async () => {
  mocks.page.mockImplementation(async (query: string, page: number) => ({
    items: query
      ? [{ id: 70, name: "旅行" }]
      : page === 1
        ? [{ id: 1, name: "First" }]
        : [{ id: 41, name: "Second page" }],
    total: query ? 1 : 41,
    page,
    hasMore: !query && page === 1,
  }));
  const onChange = vi.fn();
  const user = userEvent.setup();
  renderWithProviders(<FolderPicker values={["99"]} onChange={onChange} label="Folder" />);
  expect(await screen.findByRole("option", { name: "Selected elsewhere" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(await screen.findByRole("option", { name: "Pinned" })).toBeVisible();
  expect(mocks.page).toHaveBeenCalledTimes(1);
  await user.click(await screen.findByRole("button", { name: "Load next batch" }));
  await user.click(await screen.findByRole("option", { name: "Second page" }));
  expect(onChange).toHaveBeenLastCalledWith(["41"]);
  expect(mocks.page).toHaveBeenLastCalledWith("", 2, "name_asc");
  await user.type(screen.getByRole("textbox", { name: "Search folders…" }), "旅行");
  expect(await screen.findByRole("option", { name: "旅行" })).toBeVisible();
  expect(mocks.page).toHaveBeenLastCalledWith("旅行", 1, "name_asc");
  expect(screen.queryByRole("option", { name: "First" })).not.toBeInTheDocument();
  expect(screen.queryByRole("option", { name: "Pinned" })).not.toBeInTheDocument();
});
it("retries failed folder lookup and supports multi-select", async () => {
  mocks.page.mockRejectedValueOnce(new Error("lookup failed")).mockResolvedValue({
    items: [{ id: 8, name: "Recovered" }],
    total: 1,
    page: 1,
    hasMore: false,
  });
  const onChange = vi.fn();
  const user = userEvent.setup();
  renderWithProviders(
    <FolderPicker values={["none"]} multiple onChange={onChange} label="Folder" />,
  );
  expect(await screen.findByRole("alert")).toHaveTextContent("lookup failed");
  await user.click(screen.getByRole("button", { name: "Retry" }));
  await user.click(await screen.findByRole("option", { name: "Recovered" }));
  expect(onChange).toHaveBeenCalledWith(["none", "8"]);
});
it("updates pinned order and removal while preserving failures for retry", async () => {
  const items = [
    { id: 1, name: "First" },
    { id: 2, name: "Second" },
  ];
  mocks.sidebar.mockResolvedValue({ items, total: 100 });
  mocks.save
    .mockRejectedValueOnce(new Error("save failed"))
    .mockImplementation(async (_library: string, ids: number[]) => {
      const result = { items: ids.map((id) => items.find((item) => item.id === id)), total: 100 };
      mocks.sidebar.mockResolvedValue(result);
      return result;
    });
  const user = userEvent.setup();
  renderWithProviders(<FolderSidebarDialog open onOpenChange={vi.fn()} />);
  await user.click(await screen.findByRole("button", { name: "Move Second up" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("save failed");
  await user.click(screen.getByRole("button", { name: "Move Second up" }));
  await waitFor(() => expect(mocks.save).toHaveBeenLastCalledWith("test-library", [2, 1]));
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Move Second up" })).toBeDisabled(),
  );
  await user.click(screen.getByRole("button", { name: "Unpin First" }));
  await waitFor(() => expect(mocks.save).toHaveBeenLastCalledWith("test-library", [2]));
  await waitFor(() =>
    expect(screen.queryByRole("button", { name: "Unpin First" })).not.toBeInTheDocument(),
  );
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  expect(mocks.targets).not.toHaveBeenCalled();
});
it("shows the thirty-folder capacity and hides add choices", async () => {
  mocks.sidebar.mockResolvedValue({
    items: Array.from({ length: 20 }, (_, i) => ({ id: i + 1, name: `Pinned ${i}` })),
    total: 500,
  });
  renderWithProviders(<FolderSidebarDialog open onOpenChange={vi.fn()} />);
  expect(await screen.findByText("Pinned 20 / 30")).toBeVisible();
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  expect(mocks.targets).not.toHaveBeenCalled();
});
it("does not reuse another library's folder results", async () => {
  mocks.sidebar.mockResolvedValue({ items: [], total: 1 });
  mocks.page
    .mockResolvedValueOnce({
      items: [{ id: 1, name: "Library A" }],
      total: 1,
      page: 1,
      hasMore: false,
    })
    .mockResolvedValue({
      items: [{ id: 1, name: "Library B" }],
      total: 1,
      page: 1,
      hasMore: false,
    });
  renderWithProviders(<FolderPicker values={[]} onChange={vi.fn()} label="Folder" />);
  await screen.findByRole("option", { name: "Library A" });
  const { act } = await import("@testing-library/react");
  act(() => useAppStore.setState({ current: { id: "other-library" } as never }));
  expect(await screen.findByRole("option", { name: "Library B" })).toBeVisible();
  expect(screen.queryByRole("option", { name: "Library A" })).not.toBeInTheDocument();
});
it("shows sidebar loading errors and retries before allowing edits", async () => {
  mocks.sidebar
    .mockRejectedValueOnce(new Error("sidebar unavailable"))
    .mockResolvedValue({ items: [], total: 0 });
  renderWithProviders(<FolderSidebarDialog open onOpenChange={vi.fn()} />);
  expect(await screen.findByRole("alert")).toHaveTextContent("sidebar unavailable");
  expect(mocks.targets).not.toHaveBeenCalled();
  await userEvent.setup().click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByText("Pinned 0 / 30")).toBeVisible();
  await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  expect(mocks.targets).not.toHaveBeenCalled();
});

async function dragFixture() {
  const items = [
    { id: 1, name: "First" },
    { id: 2, name: "Second" },
    { id: 3, name: "Third" },
  ];
  mocks.sidebar.mockResolvedValue({ items, total: 3 });
  mocks.save.mockImplementation(async (_library: string, ids: number[]) => {
    const next = { items: ids.map((id) => items.find((item) => item.id === id)!), total: 3 };
    mocks.sidebar.mockResolvedValue(next);
    return next;
  });
  const view = renderWithProviders(<FolderSidebarDialog open onOpenChange={vi.fn()} />);
  const handles = await Promise.all(
    items.map((item) => screen.findByRole("button", { name: `Drag to reorder ${item.name}` })),
  );
  handles.forEach((handle, index) =>
    vi.spyOn(handle.parentElement!, "getBoundingClientRect").mockReturnValue({
      top: index * 50,
      bottom: (index + 1) * 50,
      height: 50,
      left: 0,
      right: 400,
      width: 400,
      x: 0,
      y: index * 50,
      toJSON: () => ({}),
    }),
  );
  return { ...view, handles, user: userEvent.setup() };
}
it("drags from the handle upward and persists only on release", async () => {
  const { handles, user } = await dragFixture();
  await user.pointer([
    { keys: "[MouseLeft>]", target: handles[2], coords: { x: 10, y: 125 } },
    { target: handles[2], coords: { x: 10, y: 5 } },
  ]);
  expect(mocks.save).not.toHaveBeenCalled();
  await user.pointer({ keys: "[/MouseLeft]", target: handles[2], coords: { x: 10, y: 5 } });
  await waitFor(() => expect(mocks.save).toHaveBeenCalledWith("test-library", [3, 1, 2]));
  expect(mocks.save).toHaveBeenCalledTimes(1);
});
it("uses the lower half of a row as the insertion point when dragging down", async () => {
  const { handles, user } = await dragFixture();
  await user.pointer([
    { keys: "[MouseLeft>]", target: handles[0], coords: { x: 10, y: 25 } },
    { target: handles[0], coords: { x: 10, y: 140 } },
    { keys: "[/MouseLeft]", target: handles[0], coords: { x: 10, y: 140 } },
  ]);
  await waitFor(() => expect(mocks.save).toHaveBeenCalledWith("test-library", [2, 3, 1]));
});
it("does not reorder for a click or an Escape-cancelled drag", async () => {
  const { handles, user } = await dragFixture();
  await user.click(handles[0]);
  expect(mocks.save).not.toHaveBeenCalled();
  await user.pointer([
    { keys: "[MouseLeft>]", target: handles[2], coords: { x: 10, y: 125 } },
    { target: handles[2], coords: { x: 10, y: 5 } },
  ]);
  await user.keyboard("{Escape}");
  await user.pointer({ keys: "[/MouseLeft]", target: handles[2], coords: { x: 10, y: 5 } });
  expect(mocks.save).not.toHaveBeenCalled();
});
it("supports touch dragging and preserves the old order if saving fails", async () => {
  const { handles, user } = await dragFixture();
  mocks.save.mockRejectedValueOnce(new Error("save unavailable"));
  await user.pointer([
    { keys: "[TouchA>]", target: handles[0], coords: { x: 10, y: 25 } },
    { pointerName: "TouchA", target: handles[0], coords: { x: 10, y: 140 } },
    { keys: "[/TouchA]", target: handles[0], coords: { x: 10, y: 140 } },
  ]);
  expect(await screen.findByRole("alert")).toHaveTextContent("save unavailable");
  expect(screen.getByRole("button", { name: "Move First up" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Move Third down" })).toBeDisabled();
  expect(mocks.save).toHaveBeenCalledTimes(1);
});
it("scrolls at the list edge and cancels pointer gestures without saving", async () => {
  const { handles, user } = await dragFixture();
  const list = handles[0].parentElement!.parentElement!;
  vi.spyOn(list, "getBoundingClientRect").mockReturnValue({
    top: 0,
    bottom: 100,
    height: 100,
    left: 0,
    right: 400,
    width: 400,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  });
  await user.pointer([
    { keys: "[MouseLeft>]", target: handles[0], coords: { x: 10, y: 25 } },
    { target: handles[0], coords: { x: 10, y: 95 } },
  ]);
  await waitFor(() => expect(list.scrollTop).toBeGreaterThan(0));
  const { fireEvent } = await import("@testing-library/react");
  fireEvent.pointerCancel(handles[0]);
  await user.pointer({ keys: "[/MouseLeft]", target: handles[0], coords: { x: 10, y: 95 } });
  expect(mocks.save).not.toHaveBeenCalled();
});

it("saves the display limit without dropping hidden pins and retains the saved value on reopen", async () => {
  const items = Array.from({ length: 12 }, (_, i) => ({ id: i + 1, name: `Folder ${i + 1}` }));
  mocks.sidebar.mockResolvedValue({ items, total: 12, displayLimit: 10 });
  mocks.save.mockImplementation(async (_library: string, _ids: number[], displayLimit: number) => {
    const next = { items, total: 12, displayLimit };
    mocks.sidebar.mockResolvedValue(next);
    return next;
  });
  const user = userEvent.setup();
  const view = renderWithProviders(<FolderSidebarDialog open onOpenChange={vi.fn()} />);
  const select = await screen.findByRole("combobox", { name: "Folders to display" });
  await waitFor(() => expect(select).toBeEnabled());
  expect(select).toHaveValue("10");
  expect(
    Array.from((select as HTMLSelectElement).options).map((option) => Number(option.value)),
  ).toEqual(Array.from({ length: 26 }, (_, i) => i + 5));
  await user.selectOptions(select, "5");
  await waitFor(() =>
    expect(mocks.save).toHaveBeenLastCalledWith(
      "test-library",
      items.map((item) => item.id),
      5,
    ),
  );
  await waitFor(() => expect(select).toHaveValue("5"));
  expect(screen.getByRole("button", { name: "Unpin Folder 12" })).toBeVisible();
  view.unmount();
  renderWithProviders(<FolderSidebarDialog open onOpenChange={vi.fn()} />);
  await waitFor(() =>
    expect(screen.getByRole("combobox", { name: "Folders to display" })).toHaveValue("5"),
  );
  await user.selectOptions(screen.getByRole("combobox", { name: "Folders to display" }), "30");
  await waitFor(() =>
    expect(mocks.save).toHaveBeenLastCalledWith(
      "test-library",
      items.map((item) => item.id),
      30,
    ),
  );
});
it("keeps the previous display limit when saving fails", async () => {
  mocks.sidebar.mockResolvedValue({ items: [], total: 0, displayLimit: 10 });
  mocks.save.mockRejectedValue(new Error("save failed"));
  renderWithProviders(<FolderSidebarDialog open onOpenChange={vi.fn()} />);
  const select = await screen.findByRole("combobox", { name: "Folders to display" });
  await waitFor(() => expect(select).toBeEnabled());
  await userEvent.setup().selectOptions(select, "30");
  expect(await screen.findByRole("alert")).toHaveTextContent("save failed");
  expect(select).toHaveValue("10");
});

it("keeps display guidance collapsed until requested and dismisses it without closing the manager", async () => {
  const user = userEvent.setup();
  renderWithProviders(<FolderSidebarDialog open onOpenChange={vi.fn()} />);
  const hint =
    "Choose 5–30 folders (default 10). Extra pinned folders are kept in their current order.";
  expect(screen.queryByText(hint)).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Display limit help" }));
  expect(screen.getByText(hint)).toBeVisible();
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByText(hint)).not.toBeInTheDocument());
  expect(screen.getByRole("heading", { name: "Manage pinned folders" })).toBeVisible();
});
