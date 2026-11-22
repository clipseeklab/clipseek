import { beforeEach, expect, it, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@test/helpers/render";
import { useAppStore } from "@/stores/app";
import { useImportStore } from "@/stores/import";
const mocks = vi.hoisted(() => ({ summary: vi.fn() }));
vi.mock("@/lib/api", () => ({ api: { processingSummary: mocks.summary } }));
vi.mock("@/components/import/ImportQueueSheet", () => ({
  ImportQueueContent: () => <p>Persistent import queue</p>,
}));
import TaskQueueButton from "@/components/tasks/TaskQueueButton";
beforeEach(() => {
  useAppStore.setState({ current: { id: "a", name: "A", path: "/a", createdAt: 1 } });
  useImportStore.getState().clearQueue();
  mocks.summary.mockResolvedValue({ counts: { attention: 2 }, activeCount: 0, revision: 1 });
});
it("uses the original queue popover and backend summary, with import as the sole tab", async () => {
  renderWithProviders(<TaskQueueButton />);
  await userEvent.setup().click(screen.getByRole("button", { name: "Task Queue" }));
  expect(await screen.findByText("Persistent import queue")).toBeVisible();
  expect(await screen.findByRole("tab", { name: "Import 2" })).toBeVisible();
  expect(screen.getAllByRole("tab")).toHaveLength(1);
  expect(mocks.summary).toHaveBeenCalledWith("a");
});
