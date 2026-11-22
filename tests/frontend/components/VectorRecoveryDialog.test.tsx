import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import VectorRecoveryDialog from "@/components/VectorRecoveryDialog";
import { useAppStore } from "@/stores/app";
import { act, renderWithProviders, screen, waitFor } from "../helpers/render";

const mocks = vi.hoisted(() => ({
  status: vi.fn(),
  retry: vi.fn(),
  preflight: vi.fn(),
  start: vi.fn(),
  cancel: vi.fn(),
  persistentSuccess: vi.fn(),
  persistentError: vi.fn(),
  listeners: new Map<string, (event: { payload: VectorStatusPayload }) => void>(),
}));

interface VectorStatusPayload {
  libraryId: string;
  state: string;
  code: string | null;
  message: string | null;
  rebuildId: string | null;
  failedFrames?: number | null;
}

vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, callback: (event: { payload: VectorStatusPayload }) => void) => {
    mocks.listeners.set(event, callback);
    return Promise.resolve(vi.fn());
  },
}));
vi.mock("@/lib/api", () => ({
  api: {
    vectorStatus: mocks.status,
    vectorRetryOpen: mocks.retry,
    vectorRebuildPreflight: mocks.preflight,
    vectorRebuildStart: mocks.start,
    vectorRebuildCancel: mocks.cancel,
  },
}));
vi.mock("@/components/Toaster", () => ({
  toast: {
    error: vi.fn(),
    persistentSuccess: mocks.persistentSuccess,
    persistentError: mocks.persistentError,
  },
}));

describe("VectorRecoveryDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useAppStore
      .getState()
      .setCurrent({ id: "library-1", name: "Test", path: "/tmp/test", createdAt: 1 });
    mocks.status.mockResolvedValue({
      libraryId: "library-1",
      state: "rebuild_pending",
      code: "vector_index_open_failed",
      message: "corrupt config",
      rebuildId: null,
    });
    mocks.preflight.mockResolvedValue({
      libraryId: "library-1",
      frameCount: 120,
      currentIndexBytes: 1024,
      estimatedRequiredBytes: 4096,
      availableBytes: 8192,
      canStart: true,
      reasonCode: null,
    });
    mocks.start.mockResolvedValue("rebuild-1");
    mocks.listeners.clear();
  });

  it("requires a separate preflight review before sending explicit confirmation", async () => {
    const user = userEvent.setup();
    renderWithProviders(<VectorRecoveryDialog />);
    expect(await screen.findByText("Visual index needs repair")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Review rebuild" }));
    expect(await screen.findByText("120")).toBeInTheDocument();
    expect(mocks.start).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Confirm and rebuild" }));
    await waitFor(() => expect(mocks.start).toHaveBeenCalledWith(true));
  });

  it("can be dismissed so degraded search does not block library browsing", async () => {
    const user = userEvent.setup();
    renderWithProviders(<VectorRecoveryDialog />);
    expect(await screen.findByText("Visual index needs repair")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Close" }));

    expect(await screen.findByRole("button", { name: /Repair visual index/ })).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("does not open the repair dialog for a healthy index rebuilt manually from settings", async () => {
    mocks.status.mockResolvedValue({
      libraryId: "library-1",
      state: "rebuilding",
      code: null,
      message: null,
      rebuildId: "manual-1",
      failedFrames: null,
    });

    renderWithProviders(<VectorRecoveryDialog />);

    await waitFor(() => expect(mocks.status).toHaveBeenCalled());
    expect(screen.queryByText("Visual index needs repair")).not.toBeInTheDocument();
  });

  it("shows a persistent global notification when rebuilding finishes", async () => {
    renderWithProviders(<VectorRecoveryDialog />);
    await screen.findByText("Visual index needs repair");
    await waitFor(() => expect(mocks.listeners.has("vector://rebuild-done")).toBe(true));

    act(() => {
      mocks.listeners.get("vector://rebuild-done")?.({
        payload: {
          libraryId: "library-1",
          state: "ready",
          code: null,
          message: null,
          rebuildId: null,
          failedFrames: 0,
        },
      });
    });

    expect(mocks.persistentSuccess).toHaveBeenCalledWith(
      "The visual index has been rebuilt. Click to dismiss this notification.",
    );
  });
});
