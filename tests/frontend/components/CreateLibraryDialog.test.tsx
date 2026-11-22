import userEvent from "@testing-library/user-event";
import { act, fireEvent, renderWithProviders, screen } from "@test/helpers/render";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  libraryCreate: vi.fn(),
  libraryList: vi.fn(),
  homeDir: vi.fn(),
  openDialog: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  api: { libraryCreate: mocks.libraryCreate, libraryList: mocks.libraryList },
}));
vi.mock("@tauri-apps/api/path", () => ({ homeDir: mocks.homeDir }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.openDialog }));

import CreateLibraryDialog from "@/components/library/CreateLibraryDialog";

describe("CreateLibraryDialog", () => {
  beforeEach(() => {
    mocks.homeDir.mockResolvedValue("/Users/tester");
    mocks.libraryCreate.mockRejectedValue({
      code: "library_dir_not_empty",
      message: "directory already exists",
    });
  });

  it("keeps a creation error visible and explains how to import an existing library", async () => {
    const user = userEvent.setup();
    renderWithProviders(<CreateLibraryDialog open onOpenChange={vi.fn()} />);
    await screen.findByDisplayValue("/Users/tester");

    await user.type(screen.getByPlaceholderText("e.g. My Design Assets"), "Library");
    await user.click(screen.getByRole("button", { name: "Create Library" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("The target directory already exists and is not empty");
    expect(alert).toHaveTextContent("Import Library Directory");

    await user.type(screen.getByPlaceholderText("e.g. My Design Assets"), " New");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
  it("shares the same invalid-character and reserved-name checks", async () => {
    const user = userEvent.setup();
    renderWithProviders(<CreateLibraryDialog open onOpenChange={vi.fn()} />);
    await screen.findByDisplayValue("/Users/tester");
    const input = screen.getByLabelText("Library Name");
    await user.click(input);
    await user.paste("Bad:Name");
    expect(input).toHaveValue("");
    expect(screen.getByRole("alert")).toHaveTextContent("These characters are not allowed");
    await user.type(input, "CON");
    expect(screen.getByRole("alert")).toHaveTextContent("reserved by the system");
    expect(screen.getByRole("button", { name: "Create Library" })).toBeDisabled();
    await user.type(input, "tent");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create Library" })).toBeEnabled();
  });

  it("keeps the path preview visible while composing or validating a nonempty name", async () => {
    const user = userEvent.setup();
    renderWithProviders(<CreateLibraryDialog open onOpenChange={vi.fn()} />);
    await screen.findByDisplayValue("/Users/tester");
    const input = screen.getByLabelText("Library Name");
    await user.type(input, "Archive");
    expect(screen.getByText("/Users/tester/Archive")).toBeVisible();

    fireEvent.compositionStart(input);
    expect(screen.getByText("/Users/tester/Archive")).toBeVisible();
    expect(screen.getByRole("button", { name: "Create Library" })).toBeDisabled();
    fireEvent.change(input, { target: { value: "Archive she" } });
    expect(screen.getByText("/Users/tester/Archive she")).toBeVisible();
    fireEvent.change(input, { target: { value: "Archive 设计" } });
    expect(screen.getByText("/Users/tester/Archive 设计")).toBeVisible();
    fireEvent.compositionEnd(input, { data: "设计" });
    expect(screen.getByText("/Users/tester/Archive 设计")).toBeVisible();

    fireEvent.change(input, { target: { value: "Archive 设计." } });
    expect(input).toHaveValue("Archive 设计.");
    expect(screen.getByText("/Users/tester/Archive 设计")).toBeVisible();
    expect(screen.getByRole("button", { name: "Create Library" })).toBeDisabled();
    fireEvent.change(input, { target: { value: "" } });
    expect(screen.queryByText("/Users/tester/Archive 设计")).not.toBeInTheDocument();
    fireEvent.change(input, { target: { value: "   " } });
    expect(screen.queryByText("/Users/tester/Library")).not.toBeInTheDocument();
  });

  it("reports a directory picker failure and allows choosing again", async () => {
    mocks.openDialog.mockRejectedValueOnce(new Error("Cannot open directory picker"));
    mocks.openDialog.mockResolvedValueOnce("/chosen/location");
    const user = userEvent.setup();
    renderWithProviders(<CreateLibraryDialog open onOpenChange={vi.fn()} />);
    await screen.findByDisplayValue("/Users/tester");
    await user.click(screen.getByRole("button", { name: "Choose Directory" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Cannot open directory picker");
    await user.click(screen.getByRole("button", { name: "Choose Directory" }));
    expect(screen.getByDisplayValue("/chosen/location")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("prevents cancellation while creation is in flight and reports the opened library", async () => {
    const library = { id: "created", name: "Archive", path: "/Users/tester/Archive", createdAt: 1 };
    let finish!: (value: typeof library) => void;
    mocks.libraryCreate.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const onCreated = vi.fn().mockResolvedValue(undefined);
    const onOpenChange = vi.fn();
    renderWithProviders(
      <CreateLibraryDialog open onOpenChange={onOpenChange} onCreated={onCreated} />,
    );
    const user = userEvent.setup();
    await screen.findByDisplayValue("/Users/tester");
    await user.type(screen.getByPlaceholderText("e.g. My Design Assets"), "Archive");
    await user.click(screen.getByRole("button", { name: "Create Library" }));
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
    await user.keyboard("{Escape}");
    expect(onOpenChange).not.toHaveBeenCalled();
    expect(onCreated).not.toHaveBeenCalled();
    await act(async () => finish(library));
    expect(onCreated).toHaveBeenCalledExactlyOnceWith(library);
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("does not overwrite a chosen directory when the default home path resolves late", async () => {
    let finish!: (path: string) => void;
    mocks.homeDir.mockImplementationOnce(
      () =>
        new Promise<string>((resolve) => {
          finish = resolve;
        }),
    );
    mocks.openDialog.mockResolvedValueOnce("/chosen/location");
    renderWithProviders(<CreateLibraryDialog open onOpenChange={vi.fn()} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Choose Directory" }));
    expect(screen.getByDisplayValue("/chosen/location")).toBeInTheDocument();
    await act(async () => finish("/Users/tester"));
    expect(screen.getByDisplayValue("/chosen/location")).toBeInTheDocument();
  });
});
