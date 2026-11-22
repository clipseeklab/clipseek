import { expect, it, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@test/helpers/render";
import SelectedFoldersDialog from "@/components/folders/SelectedFoldersDialog";
const get = vi.hoisted(() => vi.fn());
vi.mock("@/lib/api", () => ({ api: { folderGet: get } }));
it("allows retry and removal of missing folders without resolving the whole selection", async () => {
  get.mockReset().mockImplementation(async (id: number) => {
    if (id === 1) throw new Error("lookup unavailable");
    return null;
  });
  const remove = vi.fn();
  const user = userEvent.setup();
  renderWithProviders(
    <SelectedFoldersDialog
      ids={[1, 2]}
      onRemove={remove}
      onClear={vi.fn()}
      onOpenChange={vi.fn()}
    />,
  );
  expect(await screen.findByRole("alert")).toHaveTextContent("lookup unavailable");
  get.mockResolvedValue({ id: 1, name: "Recovered" });
  await user.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByText("Recovered")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Deselect #2" }));
  expect(remove).toHaveBeenCalledWith(2);
  expect(get).toHaveBeenCalledTimes(3);
});
