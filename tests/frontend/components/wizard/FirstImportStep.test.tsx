import { beforeEach, expect, it, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@test/helpers/render";
const mocks = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));
import FirstImportStep from "@/components/wizard/FirstImportStep";
import { useImportStore } from "@/stores/import";
import { useAppStore } from "@/stores/app";
beforeEach(() => {
  useImportStore.getState().clearQueue();
  useAppStore.setState({ current: { id: "lib", name: "Lib", path: "/library", createdAt: 1 } });
});
it("queues local references with the original import queue", async () => {
  mocks.open.mockResolvedValue(["/media/photo.png", "/media/movie.mp4"]);
  const done = vi.fn().mockResolvedValue(undefined);
  renderWithProviders(<FirstImportStep onDone={done} />);
  expect(screen.getByText(/Originals stay in place/)).toBeVisible();
  await userEvent.setup().click(screen.getByRole("button", { name: "Import Assets" }));
  expect(useImportStore.getState().tasks).toHaveLength(1);
  expect(useImportStore.getState().tasks[0]).toMatchObject({
    paths: ["/media/photo.png", "/media/movie.mp4"],
    mode: "paths",
  });
  expect(done).toHaveBeenCalledOnce();
});
it("canceling the picker adds no task and stays on the step", async () => {
  mocks.open.mockResolvedValue(null);
  const done = vi.fn();
  renderWithProviders(<FirstImportStep onDone={done} />);
  await userEvent.setup().click(screen.getByRole("button", { name: "Import Assets" }));
  expect(useImportStore.getState().tasks).toHaveLength(0);
  expect(done).not.toHaveBeenCalled();
});
it("lets the user enter the library without downloading samples", async () => {
  const done = vi.fn().mockResolvedValue(undefined);
  renderWithProviders(<FirstImportStep onDone={done} />);
  await userEvent.setup().click(screen.getByRole("button", { name: "Add later" }));
  expect(done).toHaveBeenCalledOnce();
  expect(mocks.open).not.toHaveBeenCalled();
});
