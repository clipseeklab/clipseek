import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { useAppStore } from "@/stores/app";
import { useImportStore } from "@/stores/import";
import { renderWithProviders, screen, waitFor } from "../../helpers/render";

const mocks = vi.hoisted(() => ({
  list: vi.fn(),
  resume: vi.fn(),
  abandon: vi.fn(),
  success: vi.fn(),
  error: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  api: {
    importRecoveryList: mocks.list,
    importRecoveryResume: mocks.resume,
    importRecoveryAbandon: mocks.abandon,
  },
}));
vi.mock("@/components/Toaster", () => ({
  toast: {
    success: mocks.success,
    error: mocks.error,
  },
}));

import ImportRecoveryDialog from "@/components/import/ImportRecoveryDialog";

describe("ImportRecoveryDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useAppStore.setState(
      {
        ...useAppStore.getInitialState(),
        current: { id: "library-1", name: "Library", path: "/library", createdAt: 1 },
      },
      true,
    );
    useImportStore.setState(useImportStore.getInitialState(), true);
    mocks.list.mockResolvedValue([
      {
        id: "job-1",
        mode: "folders",
        paths: ["/large-folder"],
        folderId: null,
        status: "interrupted",
        total: 10_000,
        done: 1_000,
        failed: 0,
        duplicates: 0,
        createdAt: 1,
      },
    ]);
    mocks.abandon.mockResolvedValue(undefined);
  });

  it("closes immediately after resume is requested while recovery continues in the background", async () => {
    const user = userEvent.setup();
    mocks.resume.mockReturnValue(new Promise(() => {}));
    renderWithProviders(<ImportRecoveryDialog />);

    expect(
      await screen.findByRole("heading", { name: "Unfinished imports found" }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Continue tasks" }));

    await waitFor(() => expect(mocks.resume).toHaveBeenCalledWith("job-1"));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(useImportStore.getState().tasks).toEqual([
      expect.objectContaining({ id: "job-1", status: "processing" }),
    ]);
  });

  it("reopens with a task when background submission fails", async () => {
    const user = userEvent.setup();
    mocks.resume.mockRejectedValue(new Error("resume failed"));
    renderWithProviders(<ImportRecoveryDialog />);

    await user.click(await screen.findByRole("button", { name: "Continue tasks" }));

    expect(
      await screen.findByRole("heading", { name: "Unfinished imports found" }),
    ).toBeInTheDocument();
    expect(mocks.error).toHaveBeenCalled();
    expect(mocks.list).toHaveBeenCalledTimes(2);
    expect(useImportStore.getState().tasks[0]).toMatchObject({
      id: "job-1",
      status: "failed",
      error: "resume failed",
    });
  });

  it("does not restore a stale dialog job that the backend has already removed", async () => {
    const user = userEvent.setup();
    mocks.list
      .mockResolvedValueOnce([
        {
          id: "job-1",
          mode: "folders",
          paths: ["/large-folder"],
          folderId: null,
          status: "interrupted",
          total: 10_000,
          done: 1_000,
          failed: 0,
          duplicates: 0,
          createdAt: 1,
        },
      ])
      .mockResolvedValueOnce([]);
    mocks.resume.mockRejectedValue(new Error("source disappeared"));
    renderWithProviders(<ImportRecoveryDialog />);

    await user.click(await screen.findByRole("button", { name: "Continue tasks" }));
    await waitFor(() => expect(mocks.list).toHaveBeenCalledTimes(2));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(mocks.error).toHaveBeenCalled();
  });

  it("removes an already-resolved recovery task instead of leaving it processing", async () => {
    const user = userEvent.setup();
    mocks.resume.mockResolvedValue({ queued: 0, alreadyResolved: true });
    renderWithProviders(<ImportRecoveryDialog />);

    await user.click(await screen.findByRole("button", { name: "Continue tasks" }));
    await waitFor(() => expect(mocks.resume).toHaveBeenCalledWith("job-1"));

    expect(useImportStore.getState().tasks).toEqual([]);
  });
});
