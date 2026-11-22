import { useState } from "react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { renderWithProviders, screen, within } from "@test/helpers/render";
import {
  AssetFilters,
  EMPTY_FILTER,
  toAssetFilter,
  type FilterState,
} from "@/components/assets/AssetFilters";
import { useAppStore } from "@/stores/app";

vi.mock("@/lib/api", () => ({
  api: {
    tagList: async () => [
      { id: 1, name: "Travel", assetCount: 2 },
      { id: 2, name: "Work", assetCount: 1 },
    ],
  },
}));
const reset = vi.fn();
function Filters({
  initial = EMPTY_FILTER,
  fixedTags = [],
}: {
  initial?: FilterState;
  fixedTags?: string[];
}) {
  const [value, onChange] = useState(initial);
  return (
    <>
      <AssetFilters
        value={value}
        onChange={onChange}
        showFolder={false}
        fixedTags={fixedTags}
        properties={["type", "tags", "name", "assetId"]}
        defaultPinned={["type", "tags", "name", "assetId"]}
        onReset={reset}
      />
      <output aria-label="Applied filters">{JSON.stringify(toAssetFilter(value))}</output>
    </>
  );
}
beforeEach(() => {
  useAppStore.setState({
    current: { id: "filters", name: "Filters", path: "/filters", createdAt: 1 },
  });
  reset.mockClear();
});

it("combines real type, name and numeric ID controls and clears them together", async () => {
  const user = userEvent.setup();
  renderWithProviders(<Filters />);
  await user.click(screen.getByRole("button", { name: "Image" }));
  await user.type(screen.getByRole("textbox", { name: "File Name" }), "holiday");
  await user.type(screen.getByRole("textbox", { name: "Asset ID" }), "a12b3");
  expect(screen.getByRole("textbox", { name: "Asset ID" })).toHaveValue("123");
  expect(screen.getByLabelText("Applied filters")).toHaveTextContent('"kinds":["image"]');
  expect(screen.getByLabelText("Applied filters")).toHaveTextContent('"query":"holiday"');
  await user.click(screen.getByRole("button", { name: "Clear Filters" }));
  expect(screen.getByRole("textbox", { name: "File Name" })).toHaveValue("");
  expect(screen.getByRole("textbox", { name: "Asset ID" })).toHaveValue("");
  expect(reset).toHaveBeenCalledOnce();
});

it("keeps fixed tags selected while allowing other tags to be toggled", async () => {
  const user = userEvent.setup();
  renderWithProviders(<Filters fixedTags={["Travel"]} />);
  await user.click(screen.getByRole("button", { name: "Tags(1)" }));
  const travel = await screen.findByRole("checkbox", { name: /Travel/ });
  expect(travel).toBeDisabled();
  await user.click(screen.getByRole("checkbox", { name: /Work/ }));
  expect(screen.getByLabelText("Applied filters")).toHaveTextContent('"tags":["Work"]');
  expect(travel).toBeDisabled();
});

it("recovers from corrupt persisted preferences and saves unpinned properties", async () => {
  localStorage.setItem("clip-studio.filter-properties.v1", "invalid json");
  const user = userEvent.setup();
  const { unmount } = renderWithProviders(
    <Filters initial={{ ...EMPTY_FILTER, nameQuery: "holiday" }} />,
  );
  expect(screen.getByRole("textbox", { name: "File Name" })).toHaveValue("holiday");
  await user.click(screen.getByRole("button", { name: "Add property" }));
  await user.type(screen.getByPlaceholderText("Search filter properties…"), "File name");
  // The property picker exposes icon buttons; scope to the visible property label.
  const row = screen.getByText("File name", { exact: true }).parentElement!;
  await user.click(within(row).getByRole("button", { name: "Remove from filter bar" }));
  expect(screen.queryByRole("textbox", { name: "File Name" })).not.toBeInTheDocument();
  expect(screen.getByLabelText("Applied filters")).not.toHaveTextContent("holiday");
  const saved = JSON.parse(localStorage.getItem("clip-studio.filter-properties.v1") ?? "null") as {
    pinned: string[];
  };
  expect(saved.pinned).not.toContain("name");
  unmount();
  renderWithProviders(<Filters />);
  expect(screen.queryByRole("textbox", { name: "File Name" })).not.toBeInTheDocument();
});
