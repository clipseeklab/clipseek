import userEvent from "@testing-library/user-event";
import { act, renderWithProviders, screen, waitFor, within } from "@test/helpers/render";
import { beforeEach, expect, it, vi } from "vitest";
import { useAppStore } from "@/stores/app";
import type { OperationProgress } from "@/types";

const mocks = vi.hoisted(() => ({
  operations: vi.fn<() => Promise<OperationProgress[]>>(),
  listen:
    vi.fn<
      (
        event: string,
        callback: (event: { payload: OperationProgress }) => void,
      ) => Promise<() => void>
    >(),
  migrate: vi.fn(),
  list: vi.fn(),
  current: vi.fn(),
  chooseDirectory: vi.fn(),
  toastError: vi.fn(),
  rename: vi.fn(),
  merge: vi.fn(),
  remove: vi.fn(),
  switch: vi.fn(),
  reauthorize: vi.fn(),
}));
vi.mock("@/lib/api", () => ({
  api: {
    libraryOperations: mocks.operations,
    libraryMigrate: mocks.migrate,
    libraryList: mocks.list,
    libraryCurrent: mocks.current,
    libraryRename: mocks.rename,
    libraryMerge: mocks.merge,
    libraryDelete: mocks.remove,
    librarySwitch: mocks.switch,
    libraryReauthorize: mocks.reauthorize,
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.chooseDirectory }));
vi.mock("@/components/Toaster", () => ({ toast: { error: mocks.toastError, success: vi.fn() } }));
import LibraryManageDialog from "@/components/library/LibraryManageDialog";

const library = { id: "library", name: "Assets", path: "/Assets", createdAt: 1 };
const progress: OperationProgress = {
  operationId: "move",
  operationType: "migrate_library",
  status: "running",
  phase: "copying_files",
  totalItems: 2,
  completedItems: 1,
  totalBytes: 0,
  completedBytes: 0,
  currentItem: null,
  failed: 0,
  skipped: 0,
  message: "live progress",
};
beforeEach(() => {
  vi.resetAllMocks();
  useAppStore.setState({ libraries: [library], current: library });
  mocks.operations.mockResolvedValue([]);
  mocks.listen.mockResolvedValue(vi.fn());
  mocks.list.mockResolvedValue([library]);
  mocks.current.mockResolvedValue(library);
  mocks.migrate.mockResolvedValue(undefined);
  mocks.chooseDirectory.mockResolvedValue(null);
});

const other = { ...library, id: "other", name: "Archive", path: "/Archive" };
function showTwo() {
  useAppStore.setState({ libraries: [library, other] });
  mocks.list.mockResolvedValue([library, other]);
  return renderWithProviders(<LibraryManageDialog open onOpenChange={vi.fn()} />);
}

it.each([true, false])(
  "deletes an inactive library only with the confirmed file policy (%s)",
  async (deleteFiles) => {
    const user = userEvent.setup();
    showTwo();
    expect(screen.getAllByTitle("Delete Library")[0]).toBeDisabled();
    await user.click(screen.getAllByTitle("Delete Library")[1]);
    const dialog = screen.getByRole("alertdialog");
    const checkbox = within(dialog).getByRole("checkbox", { name: "Delete files on disk too" });
    expect(checkbox).toBeChecked();
    if (!deleteFiles) await user.click(checkbox);
    expect(mocks.remove).not.toHaveBeenCalled();
    await user.click(
      within(dialog).getByRole("button", {
        name: deleteFiles ? "Delete files on disk too" : "Remove registration only",
      }),
    );
    await waitFor(() =>
      expect(mocks.remove).toHaveBeenCalledExactlyOnceWith(other.id, deleteFiles),
    );
  },
);

it("cancels deletion without changing the library or its files", async () => {
  const user = userEvent.setup();
  showTwo();
  await user.click(screen.getAllByTitle("Delete Library")[1]);
  await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Cancel" }));
  expect(mocks.remove).not.toHaveBeenCalled();
});

it("retains failed deletion for a deliberate retry", async () => {
  const user = userEvent.setup();
  mocks.remove.mockRejectedValueOnce(new Error("Delete denied"));
  showTwo();
  await user.click(screen.getAllByTitle("Delete Library")[1]);
  await user.click(
    within(screen.getByRole("alertdialog")).getByRole("button", {
      name: "Delete files on disk too",
    }),
  );
  await waitFor(() =>
    expect(mocks.toastError).toHaveBeenCalledWith(expect.stringContaining("Delete denied")),
  );
  expect(useAppStore.getState().libraries).toHaveLength(2);
});

it("allows retry after a merge failure and clears the busy state", async () => {
  const user = userEvent.setup();
  mocks.merge.mockRejectedValueOnce(new Error("Target busy")).mockResolvedValueOnce(undefined);
  showTwo();
  await user.click(screen.getAllByTitle("Merge into…")[0]);
  await user.click(screen.getByRole("button", { name: "Start Merge" }));
  await waitFor(() =>
    expect(mocks.toastError).toHaveBeenCalledWith(expect.stringContaining("Target busy")),
  );
  expect(screen.getByRole("button", { name: "Start Merge" })).toBeEnabled();
  await user.click(screen.getByRole("button", { name: "Start Merge" }));
  await waitFor(() => expect(mocks.merge).toHaveBeenCalledTimes(2));
  expect(mocks.merge).toHaveBeenLastCalledWith(library.id, other.id);
});

it("reauthorizes a library before retrying a switch", async () => {
  const user = userEvent.setup();
  mocks.switch
    .mockRejectedValueOnce({
      code: "library_database_open_failed",
      message: "/Archive inaccessible",
    })
    .mockResolvedValueOnce(other);
  mocks.chooseDirectory.mockResolvedValue("/Restored");
  showTwo();
  await user.click(screen.getByRole("button", { name: /Archive \/Archive/ }));
  await waitFor(() => expect(mocks.reauthorize).toHaveBeenCalledWith(other.id, "/Restored"));
  expect(mocks.switch).toHaveBeenCalledTimes(2);
  expect(useAppStore.getState().current?.id).toBe(other.id);
});

it("does not migrate when directory selection is cancelled", async () => {
  renderWithProviders(<LibraryManageDialog open onOpenChange={vi.fn()} />);
  await userEvent.setup().click(screen.getByTitle("Move Library Directory"));
  expect(mocks.chooseDirectory).toHaveBeenCalledOnce();
  expect(mocks.migrate).not.toHaveBeenCalled();
});

it("labels the fixed source and selectable target libraries in the merge dialog", async () => {
  const target = { ...library, id: "target", name: "Design Assets", path: "/Design" };
  useAppStore.setState({ libraries: [library, target] });
  renderWithProviders(<LibraryManageDialog open onOpenChange={vi.fn()} />);
  await userEvent.setup().click(screen.getAllByTitle("Merge into…")[0]);
  const dialog = screen.getByRole("dialog", { name: "Merge Libraries" });
  const source = within(dialog).getByRole("textbox", { name: "Source Library" });
  expect(source).toHaveValue("Assets");
  expect(source).toHaveAttribute("readonly");
  expect(within(dialog).getByRole("combobox", { name: "Target Library" })).toHaveTextContent(
    "Design Assets",
  );
  expect(within(dialog).getByText("Source Library")).toBeVisible();
  expect(within(dialog).getByText("Target Library")).toBeVisible();
});

it("migrates to the chosen directory and refreshes the selected library", async () => {
  mocks.chooseDirectory.mockResolvedValue("/Moved");
  mocks.list.mockResolvedValue([{ ...library, path: "/Moved" }]);
  mocks.current.mockResolvedValue({ ...library, path: "/Moved" });
  renderWithProviders(<LibraryManageDialog open onOpenChange={vi.fn()} />);
  await userEvent.setup().click(screen.getByTitle("Move Library Directory"));
  await waitFor(() => expect(screen.getByText("/Moved")).toBeVisible());
  expect(mocks.migrate).toHaveBeenCalledExactlyOnceWith(library.id, "/Moved");
  expect(useAppStore.getState().current?.path).toBe("/Moved");
});

it("reports directory picker rejection instead of leaving an unhandled promise", async () => {
  mocks.chooseDirectory.mockRejectedValue(new Error("Picker unavailable"));
  renderWithProviders(<LibraryManageDialog open onOpenChange={vi.fn()} />);
  await userEvent.setup().click(screen.getByTitle("Move Library Directory"));
  await waitFor(() =>
    expect(mocks.toastError).toHaveBeenCalledWith(expect.stringContaining("Picker unavailable")),
  );
  expect(mocks.migrate).not.toHaveBeenCalled();
});

it("shows a subscription failure and can retry registration", async () => {
  mocks.listen.mockRejectedValueOnce(new Error("Events unavailable"));
  renderWithProviders(<LibraryManageDialog open onOpenChange={vi.fn()} />);
  expect(await screen.findByRole("alert")).toHaveTextContent("Events unavailable");
  await userEvent.setup().click(screen.getByRole("button", { name: "Retry" }));
  await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  expect(mocks.listen).toHaveBeenCalledTimes(2);
});

it("shows an operation snapshot failure and can load it again", async () => {
  mocks.operations.mockRejectedValueOnce(new Error("History unavailable"));
  renderWithProviders(<LibraryManageDialog open onOpenChange={vi.fn()} />);
  expect(await screen.findByRole("alert")).toHaveTextContent("History unavailable");
  await userEvent.setup().click(screen.getByRole("button", { name: "Retry" }));
  await waitFor(() => expect(mocks.operations).toHaveBeenCalledTimes(2));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

it("does not replace a live progress event with a late snapshot", async () => {
  let resolve!: (items: OperationProgress[]) => void;
  mocks.operations.mockReturnValue(
    new Promise((done) => {
      resolve = done;
    }),
  );
  renderWithProviders(<LibraryManageDialog open onOpenChange={vi.fn()} />);
  await waitFor(() => expect(mocks.operations).toHaveBeenCalledOnce());
  act(() => mocks.listen.mock.calls[0][1]({ payload: progress }));
  expect(screen.getByText("live progress")).toBeVisible();
  await act(async () => resolve([{ ...progress, message: "stale snapshot" }]));
  expect(screen.getByText("live progress")).toBeVisible();
  expect(screen.queryByText("stale snapshot")).not.toBeInTheDocument();
});

it("releases a subscription that resolves after unmount", async () => {
  let resolve!: (stop: () => void) => void;
  const stop = vi.fn();
  mocks.listen.mockReturnValue(
    new Promise((done) => {
      resolve = done;
    }),
  );
  const { unmount } = renderWithProviders(<LibraryManageDialog open onOpenChange={vi.fn()} />);
  unmount();
  await act(async () => resolve(stop));
  expect(stop).toHaveBeenCalledOnce();
});
