import { act, renderWithProviders, screen } from "@test/helpers/render";
import { expect, it, vi } from "vitest";
import SidebarFolders from "@/components/layout/SidebarFolders";
const first = { id: 1, name: "First folder", createdAt: 1, assetCount: 2 };
const second = { id: 2, name: "Second folder", createdAt: 1, assetCount: 3 };
const third = { id: 3, name: "New folder", createdAt: 1, assetCount: 4 };
it("makes additions available and keeps departures only until their exit finishes", () => {
  vi.useFakeTimers();
  const view = renderWithProviders(<SidebarFolders folders={[first, second]} />);
  view.rerender(<SidebarFolders folders={[third, first]} />);
  expect(screen.getByRole("link", { name: "New folder 4" })).toHaveAttribute("href", "/folders/3");
  expect(screen.queryByRole("link", { name: "Second folder 3" })).not.toBeInTheDocument();
  expect(screen.getByText("Second folder")).toBeInTheDocument();
  act(() => {
    vi.advanceTimersByTime(240);
  });
  expect(screen.queryByText("Second folder")).not.toBeInTheDocument();
  expect(screen.getAllByRole("link").map((link) => link.textContent)).toEqual([
    "New folder4",
    "First folder2",
  ]);
});
it("does not let an older exit remove a rapidly re-added folder", () => {
  vi.useFakeTimers();
  const view = renderWithProviders(<SidebarFolders folders={[first, second]} />);
  view.rerender(<SidebarFolders folders={[first]} />);
  act(() => {
    vi.advanceTimersByTime(100);
  });
  view.rerender(
    <SidebarFolders folders={[{ ...second, name: "Renamed", assetCount: 5 }, first]} />,
  );
  act(() => {
    vi.advanceTimersByTime(240);
  });
  expect(screen.getByRole("link", { name: "Renamed 5" })).toHaveAttribute("href", "/folders/2");
  expect(screen.queryByText("Second folder")).not.toBeInTheDocument();
});
it("waits for the initial list and drops old rows immediately on library change", () => {
  vi.useFakeTimers();
  const view = renderWithProviders(<SidebarFolders key="a" />);
  view.rerender(<SidebarFolders key="a" folders={[first, second]} />);
  expect(screen.getAllByRole("link")).toHaveLength(2);
  view.rerender(<SidebarFolders key="a" folders={[first]} />);
  view.rerender(<SidebarFolders key="b" folders={[third]} />);
  expect(screen.queryByText("First folder")).not.toBeInTheDocument();
  expect(screen.queryByText("Second folder")).not.toBeInTheDocument();
  act(() => {
    vi.advanceTimersByTime(240);
  });
  expect(screen.getByRole("link", { name: "New folder 4" })).toBeInTheDocument();
});
it("prevents native dragging of folder shortcuts", () => {
  const { container } = renderWithProviders(<SidebarFolders folders={[first]} />);
  expect(container.querySelector("a")).toHaveAttribute("draggable", "false");
});
