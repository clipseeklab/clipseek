import { beforeEach, expect, it, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen, waitFor } from "@test/helpers/render";
import TargetPicker from "@/components/TargetPicker";
import { useAppStore } from "@/stores/app";
const lookup = vi.hoisted(() => vi.fn());
vi.mock("@/lib/api", () => ({ api: { targetOptions: lookup } }));
const props = {
  kind: "tags" as const,
  exclude: [3, 9],
  value: "",
  onChange: vi.fn(),
  disabled: false,
  label: "Target tag",
};
beforeEach(() => {
  lookup.mockReset();
  props.onChange.mockReset();
  lookup.mockResolvedValue({ items: [], hasMore: false });
});
it("requests one bounded page and fetches the next only on demand", async () => {
  lookup
    .mockResolvedValueOnce({
      items: Array.from({ length: 50 }, (_, i) => ({
        id: i + 10,
        name: `Tag ${i}`,
      })),
      hasMore: true,
    })
    .mockResolvedValueOnce({
      items: [{ id: 60, name: "Last tag" }],
      hasMore: false,
    });
  renderWithProviders(<TargetPicker {...props} />);
  expect(await screen.findAllByRole("option")).toHaveLength(50);
  expect(lookup).toHaveBeenCalledTimes(1);
  expect(lookup).toHaveBeenLastCalledWith("tags", "", 0, [3, 9]);
  await userEvent.setup().click(screen.getByRole("button", { name: "Load next batch" }));
  expect(await screen.findByRole("option", { name: "Last tag" })).toBeVisible();
  expect(lookup).toHaveBeenLastCalledWith("tags", "", 59, [3, 9]);
  expect(screen.queryByRole("button", { name: "Load next batch" })).not.toBeInTheDocument();
});
it("searches remotely from the first page and replaces the old choices", async () => {
  lookup
    .mockResolvedValueOnce({
      items: [{ id: 1, name: "Old tag" }],
      hasMore: true,
    })
    .mockResolvedValue({ items: [{ id: 90, name: "旅行" }], hasMore: false });
  renderWithProviders(<TargetPicker {...props} />);
  await screen.findByRole("option", { name: "Old tag" });
  await userEvent.setup().type(screen.getByRole("textbox", { name: "Target tag" }), "旅行");
  await waitFor(() => expect(lookup).toHaveBeenLastCalledWith("tags", "旅行", 0, [3, 9]));
  expect(await screen.findByRole("option", { name: "旅行" })).toBeVisible();
  expect(screen.queryByRole("option", { name: "Old tag" })).not.toBeInTheDocument();
});
it("allows retry after a lookup failure", async () => {
  lookup.mockRejectedValueOnce(new Error("lookup failed")).mockResolvedValue({
    items: [{ id: 2, name: "Recovered" }],
    hasMore: false,
  });
  renderWithProviders(<TargetPicker {...props} />);
  expect(await screen.findByRole("alert")).toHaveTextContent("lookup failed");
  await userEvent.setup().click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByRole("option", { name: "Recovered" })).toBeVisible();
});
it("selects the numeric identity and disables choices while submitting", async () => {
  lookup.mockResolvedValue({
    items: [{ id: 42, name: "Destination" }],
    hasMore: false,
  });
  const { rerender } = renderWithProviders(<TargetPicker {...props} />);
  await userEvent.setup().click(await screen.findByRole("option", { name: "Destination" }));
  expect(props.onChange).toHaveBeenCalledWith("42");
  rerender(<TargetPicker {...props} value="42" disabled />);
  expect(screen.getByRole("option", { name: "Destination" })).toBeDisabled();
  expect(screen.getByRole("option", { name: "Destination" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(screen.getByRole("textbox")).toBeDisabled();
});
it("does not show a previous library's cached targets after switching libraries", async () => {
  useAppStore.setState({
    current: { id: "lib-old", name: "Old", path: "/old", createdAt: 1 },
  });
  lookup.mockResolvedValueOnce({
    items: [{ id: 1, name: "Old library tag" }],
    hasMore: false,
  });
  const { rerender } = renderWithProviders(<TargetPicker {...props} />);
  await screen.findByRole("option", { name: "Old library tag" });
  lookup.mockResolvedValue({
    items: [{ id: 2, name: "New library tag" }],
    hasMore: false,
  });
  useAppStore.setState({
    current: { id: "lib-new", name: "New", path: "/new", createdAt: 1 },
  });
  rerender(<TargetPicker {...props} />);
  expect(await screen.findByRole("option", { name: "New library tag" })).toBeVisible();
  expect(screen.queryByRole("option", { name: "Old library tag" })).not.toBeInTheDocument();
});
it("renders an empty search result without inventing a selectable target", async () => {
  renderWithProviders(<TargetPicker {...props} kind="folders" />);
  await waitFor(() => expect(lookup).toHaveBeenCalledWith("folders", "", 0, [3, 9]));
  expect(await screen.findByText("No matching assets")).toBeVisible();
  expect(screen.queryByRole("option")).not.toBeInTheDocument();
});
