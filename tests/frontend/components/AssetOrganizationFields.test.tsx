import { useState } from "react";
import { beforeEach, expect, it, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { fireEvent, renderWithProviders, screen } from "@test/helpers/render";
import TagInput from "@/components/metadata/TagInput";
import { collectTagNames, type TagInputValue } from "@/lib/tagInput";
import { useAppStore } from "@/stores/app";
const mocks = vi.hoisted(() => ({ tags: vi.fn(), save: vi.fn() }));
vi.mock("@/lib/api", () => ({ api: { tagList: mocks.tags } }));
function Form() {
  const [value, setValue] = useState<TagInputValue>({ names: [], input: "" });
  return (
    <>
      <TagInput value={value} onChange={setValue} />
      <button onClick={() => mocks.save(collectTagNames(value))}>Save</button>
    </>
  );
}
beforeEach(() => {
  vi.resetAllMocks();
  useAppStore.setState({
    current: { id: "lib-a", name: "A", path: "/tmp/a", createdAt: 1 },
    recentTagNames: [],
  });
  mocks.tags.mockResolvedValue([
    { id: 1, name: "Travel", createdAt: 2, assetCount: 3 },
    { id: 2, name: "Portrait", createdAt: 1, assetCount: 4 },
  ]);
});
it("selects existing tags with the keyboard and supports chip removal", async () => {
  const user = userEvent.setup();
  renderWithProviders(<Form />);
  const input = screen.getByRole("combobox", { name: "Tags" });
  await user.type(input, "trav");
  expect(await screen.findByRole("option", { name: "Travel" })).toBeVisible();
  await user.keyboard("{ArrowDown}{Enter}");
  expect(screen.getByRole("button", { name: "Delete Travel" })).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Delete Travel" }));
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(mocks.save).toHaveBeenLastCalledWith([]);
});
it("adds comma-separated tags once and includes text that was not confirmed with Enter", async () => {
  const user = userEvent.setup();
  renderWithProviders(<Form />);
  await user.type(screen.getByRole("combobox", { name: "Tags" }), "one，one,two{enter}last draft");
  expect(screen.getAllByRole("button", { name: "Delete one" })).toHaveLength(1);
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(mocks.save).toHaveBeenCalledWith(["one", "two", "last draft"]);
});
it("keeps IME confirmation separate from adding a tag", async () => {
  const user = userEvent.setup();
  renderWithProviders(<Form />);
  const input = screen.getByRole("combobox", { name: "Tags" });
  await user.click(input);
  fireEvent.compositionStart(input);
  fireEvent.change(input, { target: { value: "旅行" } });
  fireEvent.keyDown(input, { key: "Enter", isComposing: true });
  expect(screen.queryByRole("button", { name: "Delete 旅行" })).not.toBeInTheDocument();
  expect(input).toHaveValue("旅行");
  fireEvent.compositionEnd(input);
  await user.keyboard("{Enter}");
  expect(screen.getByRole("button", { name: "Delete 旅行" })).toBeVisible();
});
it("ArrowUp selects the last suggestion and Escape closes suggestions without clearing the draft", async () => {
  const user = userEvent.setup();
  renderWithProviders(<Form />);
  const input = screen.getByRole("combobox", { name: "Tags" });
  await user.click(input);
  await screen.findByRole("option", { name: "Portrait" });
  await user.keyboard("{ArrowUp}{Enter}");
  expect(screen.getByRole("button", { name: "Delete Portrait" })).toBeVisible();
  await user.type(input, "unfinished");
  await user.keyboard("{Escape}");
  expect(input).toHaveAttribute("aria-expanded", "false");
  expect(input).toHaveValue("unfinished");
});
