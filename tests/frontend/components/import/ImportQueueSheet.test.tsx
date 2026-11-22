import { queryKeys } from "@/lib/queryKeys";
import { beforeEach, describe, expect, it, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { act, screen, waitFor, within } from "@testing-library/react";
import { renderWithProviders } from "@test/helpers/render";
import { useAppStore } from "@/stores/app";
import { useImportStore } from "@/stores/import";
import type { ProcessingItem, ProcessingPage } from "@/types";

const mocks = vi.hoisted(() => ({
  list: vi.fn(),
  summary: vi.fn(),
  history: vi.fn(),
  retry: vi.fn(),
  hide: vi.fn(),
  clear: vi.fn(),
  cancel: vi.fn(),
  notices: vi.fn(),
}));
vi.mock("@/lib/api", () => ({
  api: {
    processingList: mocks.list,
    processingSummary: mocks.summary,
    processingHistory: mocks.history,
    processingRetry: mocks.retry,
    processingHide: mocks.hide,
    processingClear: mocks.clear,
    processingCancel: mocks.cancel,
    importNotices: mocks.notices,
  },
}));
import ImportQueueButton from "@/components/import/ImportQueueSheet";

const item: ProcessingItem = {
  key: "file-1",
  batchId: "batch-1",
  assetId: null,
  fileName: "broken.png",
  sourcePath: "C:/source/broken.png",
  status: "attention",
  createdAt: 123,
  tasks: [
    {
      id: 7,
      kind: "import",
      status: "failed",
      phase: "copy",
      error: "Permission denied",
      attempt: 1,
      indexTaskId: null,
    },
  ],
};
const response = (items: ProcessingItem[] = [item]): ProcessingPage => ({
  items,
  total: items.length,
  page: 1,
  pageSize: 12,
  counts: { attention: items.length, processing: 0 },
});
beforeEach(() => {
  vi.clearAllMocks();
  useImportStore.getState().clearQueue();
  useAppStore.setState({
    current: { id: "library-a", name: "Library A", path: "/lib", createdAt: 1, color: "blue" },
  });
  mocks.list.mockResolvedValue(response());
  mocks.summary.mockResolvedValue({ counts: { attention: 1 }, activeCount: 0, revision: 1 });
  mocks.history.mockResolvedValue({ tasks: item.tasks, nextCursor: null });
  mocks.notices.mockResolvedValue([]);
  mocks.retry.mockResolvedValue(undefined);
  mocks.hide.mockResolvedValue(undefined);
  mocks.clear.mockResolvedValue(24);
});
async function open() {
  renderWithProviders(<ImportQueueButton />);
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: /Import Queue/ }));
  await screen.findByRole("article", { name: "broken.png" });
  return user;
}
describe("durable material queue", () => {
  it("refreshes assets only when the processing revision changes", async () => {
    const { queryClient } = renderWithProviders(<ImportQueueButton />);
    await waitFor(() =>
      expect(queryClient.getQueryData(["processing", "library-a", "summary"])).toBeDefined(),
    );
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");
    await act(async () => {
      await queryClient.refetchQueries({ queryKey: ["processing", "library-a", "summary"] });
    });
    expect(invalidate).not.toHaveBeenCalledWith({
      queryKey: queryKeys.assets.library(useAppStore.getState().current?.id),
    });
    mocks.summary.mockResolvedValue({ counts: { success: 1 }, activeCount: 0, revision: 2 });
    await act(async () => {
      await queryClient.refetchQueries({ queryKey: ["processing", "library-a", "summary"] });
    });
    await waitFor(() =>
      expect(invalidate).toHaveBeenCalledWith({
        queryKey: queryKeys.assets.library(useAppStore.getState().current?.id),
      }),
    );
    expect(mocks.list).not.toHaveBeenCalled();
  });
  it("only requests badge counts while closed and loads history on expansion", async () => {
    mocks.history.mockResolvedValue({ tasks: item.tasks, nextCursor: null });
    renderWithProviders(<ImportQueueButton />);
    await waitFor(() => expect(mocks.summary).toHaveBeenCalledWith("library-a"));
    expect(mocks.list).not.toHaveBeenCalled();
    expect(mocks.history).not.toHaveBeenCalled();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Import Queue/ }));
    await screen.findByRole("article", { name: "broken.png" });
    expect(mocks.history).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Previous attempts" }));
    await waitFor(() =>
      expect(mocks.history).toHaveBeenCalledWith("library-a", "file-1", undefined),
    );
    expect(await screen.findByText(/#1.*Permission denied/)).toBeVisible();
  });
  it("loads older attempts using the server cursor", async () => {
    mocks.history
      .mockResolvedValueOnce({
        tasks: [{ ...item.tasks[0], id: 200, attempt: 2 }],
        nextCursor: 200,
      })
      .mockResolvedValue({ tasks: item.tasks, nextCursor: null });
    const user = await open();
    await user.click(screen.getByRole("button", { name: "Previous attempts" }));
    await user.click(await screen.findByRole("button", { name: "Load More" }));
    await waitFor(() => expect(mocks.history).toHaveBeenLastCalledWith("library-a", "file-1", 200));
    expect(await screen.findByText(/#1.*Permission denied/)).toBeVisible();
  });
  it("keeps current steps visible when loading history fails", async () => {
    mocks.history.mockRejectedValue(new Error("History unavailable"));
    const user = await open();
    await user.click(screen.getByRole("button", { name: "Previous attempts" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("History unavailable");
    expect(screen.getByText("Permission denied")).toBeVisible();
  });
  it("hides internal batch information and clears terminal records across all pages", async () => {
    mocks.list.mockResolvedValue({
      ...response([{ ...item, batchId: "legacy-index" }]),
      total: 25,
    });
    const user = await open();
    expect(screen.queryByText(/legacy-index/)).toBeNull();
    await user.click(screen.getByRole("button", { name: "Clear finished" }));
    expect(mocks.clear).toHaveBeenCalledExactlyOnceWith("library-a");
    expect(await screen.findByRole("status")).toHaveTextContent("24");
  });
  it("reports a bulk clear failure without hiding the records", async () => {
    mocks.clear.mockRejectedValue(new Error("Library changed"));
    const user = await open();
    await user.click(screen.getByRole("button", { name: "Clear finished" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Library changed");
    expect(screen.getByRole("article", { name: "broken.png" })).toBeInTheDocument();
  });
  it("disables bulk clear for an empty queue", async () => {
    mocks.list.mockResolvedValue(response([]));
    mocks.summary.mockResolvedValue({ counts: {}, activeCount: 0, revision: 1 });
    renderWithProviders(<ImportQueueButton />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Import Queue/ }));
    await screen.findByText("No tasks");
    expect(screen.getByRole("button", { name: "Clear finished" })).toBeDisabled();
    expect(mocks.clear).not.toHaveBeenCalled();
  });
  it("shows failed files without an asset and retries the exact task in its original library", async () => {
    const user = await open();
    expect(screen.getByText("Permission denied")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(mocks.retry).toHaveBeenCalledWith("library-a", 7);
    expect(mocks.list.mock.calls.length).toBeGreaterThan(1);
  });
  it("shows retry errors and retains the failed row", async () => {
    mocks.retry.mockRejectedValue(new Error("Disk is still full"));
    const user = await open();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Disk is still full");
    expect(screen.getByRole("article", { name: "broken.png" })).toBeInTheDocument();
  });
  it("prevents clearing active work and shows import completion with pending indexing", async () => {
    mocks.list.mockResolvedValue(
      response([
        {
          ...item,
          assetId: 8,
          status: "processing",
          tasks: [
            { ...item.tasks[0], kind: "import", status: "completed", error: null },
            {
              ...item.tasks[0],
              id: 8,
              kind: "visual",
              status: "running",
              error: null,
              indexTaskId: 44,
            },
            {
              ...item.tasks[0],
              id: 9,
              kind: "subtitles",
              status: "queued",
              error: null,
              indexTaskId: 45,
            },
          ],
        },
      ]),
    );
    const user = await open();
    const row = screen.getByRole("article", { name: "broken.png" });
    expect(within(row).getByText("Visual index · Building")).toBeInTheDocument();
    expect(within(row).getByText("Subtitle index · Waiting")).toBeInTheDocument();
    expect(within(row).getByRole("button", { name: "Clear record" })).toBeDisabled();
    await user.click(within(row).getAllByRole("button", { name: "Cancel" })[0]);
    expect(mocks.cancel).toHaveBeenCalledWith("library-a", 8);
  });
  it("keeps the previous failure visible after a successful retry", async () => {
    mocks.list.mockResolvedValue(
      response([
        {
          ...item,
          status: "success",
          tasks: [
            { ...item.tasks[0], id: 8, status: "completed", error: null, attempt: 2 },
            item.tasks[0],
          ],
        },
      ]),
    );
    const user = await open();
    await user.click(screen.getByRole("button", { name: "Previous attempts" }));
    expect(await screen.findByText(/#1.*Failed.*Permission denied/)).toBeVisible();
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Clear record" }));
    expect(mocks.hide).toHaveBeenCalledWith("library-a", "file-1");
  });
  it("requests server-side status/type filters and switches library keys", async () => {
    const user = await open();
    await user.click(screen.getByRole("tab", { name: /Has issues/ }));
    await user.selectOptions(screen.getByRole("combobox", { name: "Task type" }), "visual");
    await waitFor(() =>
      expect(mocks.list).toHaveBeenLastCalledWith("library-a", {
        status: "attention",
        kind: "visual",
        page: 1,
        pageSize: 12,
      }),
    );
  });
  it("shows a recoverable query error instead of an empty queue", async () => {
    mocks.list.mockRejectedValue(new Error("Database unavailable"));
    renderWithProviders(<ImportQueueButton />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Import Queue/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Database unavailable");
    expect(screen.queryByText("No tasks")).toBeNull();
    mocks.list.mockResolvedValue(response());
    mocks.summary.mockResolvedValue({ counts: { attention: 1 }, activeCount: 0, revision: 1 });
    mocks.history.mockResolvedValue({ tasks: item.tasks, nextCursor: null });
    await user.click(screen.getByRole("button", { name: "Retry" }));
    await screen.findByRole("article", { name: "broken.png" });
  });
});
