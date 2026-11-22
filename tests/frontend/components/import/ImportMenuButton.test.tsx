import userEvent from "@testing-library/user-event";
import { useLocation } from "react-router-dom";
import { beforeEach, expect, it, vi } from "vitest";
import { renderWithProviders, screen, waitFor } from "@test/helpers/render";
import { useAppStore } from "@/stores/app";
import { useImportStore } from "@/stores/import";

const mocks = vi.hoisted(() => ({
  open: vi.fn(),
  importLibrary: vi.fn(),
  list: vi.fn(),
  error: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));
vi.mock("@/lib/api", () => ({
  api: { libraryImport: mocks.importLibrary, libraryList: mocks.list },
}));
vi.mock("@/components/Toaster", () => ({ toast: { success: vi.fn(), error: mocks.error } }));
vi.mock("@/components/library/CreateLibraryDialog", () => ({ default: () => null }));
vi.mock("@/components/folders/FolderNameDialog", () => ({ default: () => null }));
import ImportMenuButton from "@/components/import/ImportMenuButton";

const library = { id: "library-a", name: "A", path: "/a", createdAt: 1 };
function Location() {
  return <output aria-label="Current route">{useLocation().pathname}</output>;
}
beforeEach(() => {
  vi.resetAllMocks();
  useAppStore.setState({ current: library, libraries: [library] });
  useImportStore.setState(useImportStore.getInitialState(), true);
  mocks.open.mockResolvedValue(null);
});
async function choose(name: string) {
  const user = userEvent.setup();
  renderWithProviders(
    <>
      <ImportMenuButton />
      <Location />
    </>,
    { route: "/search" },
  );
  await user.click(screen.getByRole("button", { name: "Import" }));
  await user.click(screen.getByRole("menuitem", { name }));
}

it.each([
  ["Import Assets", false, "paths"],
  ["Import Folder", true, "folders"],
])("queues selected paths with the correct import mode: %s", async (name, directory, mode) => {
  mocks.open.mockResolvedValue(["/source/a.jpg", "/source/b.jpg"]);
  const enqueue = vi.spyOn(useImportStore.getState(), "enqueue");
  await choose(name);
  await waitFor(() =>
    expect(enqueue).toHaveBeenCalledWith(["/source/a.jpg", "/source/b.jpg"], null, mode),
  );
  expect(mocks.open).toHaveBeenCalledWith(expect.objectContaining({ multiple: true, directory }));
  expect(screen.getByLabelText("Current route")).toHaveTextContent("/assets");
});

it("does not enqueue or navigate after cancelling file selection", async () => {
  const enqueue = vi.spyOn(useImportStore.getState(), "enqueue");
  await choose("Import Assets");
  await waitFor(() => expect(mocks.open).toHaveBeenCalledOnce());
  expect(enqueue).not.toHaveBeenCalled();
  expect(screen.getByLabelText("Current route")).toHaveTextContent("/search");
});

it.each(["Import Library", "Import Library library.json"])(
  "imports an existing library through %s",
  async (name) => {
    const imported = { ...library, id: "library-b", name: "B", path: "/b" };
    mocks.open.mockResolvedValue("/b/library.json");
    mocks.importLibrary.mockResolvedValue(imported);
    mocks.list.mockResolvedValue([library, imported]);
    const { queryClient } = renderWithProviders(<ImportMenuButton />);
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Import" }));
    await user.click(screen.getByRole("menuitem", { name }));
    await waitFor(() => expect(useAppStore.getState().current?.id).toBe(imported.id));
    expect(useAppStore.getState().libraries).toEqual([library, imported]);
    expect(invalidate).toHaveBeenCalled();
    expect(mocks.importLibrary).toHaveBeenCalledWith("/b/library.json");
  },
);

it("preserves the current library and reports an import failure", async () => {
  mocks.open.mockResolvedValue("/broken");
  mocks.importLibrary.mockRejectedValue(new Error("Invalid library"));
  await choose("Import Library");
  await waitFor(() =>
    expect(mocks.error).toHaveBeenCalledWith(expect.stringContaining("Invalid library")),
  );
  expect(useAppStore.getState().current).toEqual(library);
});
