import { useState } from "react";
import { act, fireEvent, renderWithProviders, screen, waitFor } from "@test/helpers/render";
import { buildAsset } from "@test/fixtures/assets";
import { stubConvertFileSrc } from "@test/helpers/tauri";
import { beforeEach, expect, it, vi } from "vitest";
import type { Asset } from "@/types";
import { useAppStore } from "@/stores/app";

const mocks = vi.hoisted(() => ({ error: vi.fn() }));
vi.mock("@/components/Toaster", () => ({ toast: { error: mocks.error } }));
vi.mock("@/lib/api", () => ({ api: {} }));
import { AssetSourcePreview } from "@/components/assets/AssetSourcePreview";

const first = buildAsset({ id: 1, fileName: "first.png" });
const last = buildAsset({ id: 2, fileName: "page-one-last.png" });
const next = buildAsset({ id: 3, fileName: "page-two-first.png" });

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

function Preview({
  load,
  scope = "library-a/all",
  open = true,
  singleton = false,
}: {
  load: () => Promise<Asset[]>;
  scope?: string;
  open?: boolean;
  singleton?: boolean;
}) {
  const [assets, setAssets] = useState(singleton ? [last] : [first, last]);
  const [hasMore, setHasMore] = useState(true);
  return (
    <AssetSourcePreview
      asset={last}
      assets={assets}
      open={open}
      onOpenChange={vi.fn()}
      pagination={{
        scopeKey: scope,
        total: singleton ? 2 : 3,
        hasMore,
        loadMore: async () => {
          const loaded = await load();
          setAssets(loaded);
          setHasMore(false);
          return loaded;
        },
      }}
    />
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  stubConvertFileSrc();
  useAppStore.setState({ current: null });
});

it("loads one page at the boundary, guards repeated input and advances using the real total", async () => {
  const pending = deferred<Asset[]>();
  const load = vi.fn(() => pending.promise);
  renderWithProviders(<Preview load={load} />);
  expect(screen.getByText("2 / 3")).toBeVisible();
  expect(load).not.toHaveBeenCalled();
  const button = screen.getByRole("button", { name: "Next asset" });
  expect(button).toBeEnabled();
  fireEvent.click(button);
  expect(button).toBeDisabled();
  expect(button).toHaveAttribute("aria-busy", "true");
  fireEvent.keyDown(screen.getByRole("dialog"), { key: "ArrowRight" });
  expect(load).toHaveBeenCalledTimes(1);
  await act(async () => pending.resolve([first, last, next]));
  expect(screen.getByAltText(next.fileName)).toBeVisible();
  expect(screen.getByText("3 / 3")).toBeVisible();
  expect(screen.getByRole("button", { name: "Next asset" })).toBeDisabled();
  fireEvent.keyDown(screen.getByRole("dialog"), { key: "ArrowLeft" });
  expect(screen.getByAltText(last.fileName)).toBeVisible();
});

it("keeps the current preview on failure and allows a retry", async () => {
  const load = vi
    .fn()
    .mockRejectedValueOnce(new Error("disk busy"))
    .mockResolvedValueOnce([first, last, next]);
  renderWithProviders(<Preview load={load} />);
  fireEvent.click(screen.getByRole("button", { name: "Next asset" }));
  await waitFor(() => expect(mocks.error).toHaveBeenCalledWith("disk busy"));
  expect(screen.getByAltText(last.fileName)).toBeVisible();
  expect(screen.getByRole("button", { name: "Next asset" })).toBeEnabled();
  fireEvent.click(screen.getByRole("button", { name: "Next asset" }));
  expect(await screen.findByAltText(next.fileName)).toBeVisible();
  expect(load).toHaveBeenCalledTimes(2);
});

it("does not advance after navigating backwards while a page is loading", async () => {
  const pending = deferred<Asset[]>();
  const load = vi.fn(() => pending.promise);
  renderWithProviders(<Preview load={load} />);
  fireEvent.click(screen.getByRole("button", { name: "Next asset" }));
  expect(load).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole("button", { name: "Previous asset" }));
  expect(screen.getByAltText(first.fileName)).toBeVisible();
  await act(async () => pending.resolve([first, last, next]));
  expect(screen.getByAltText(first.fileName)).toBeVisible();
});

it.each(["close", "scope", "library"])("ignores late navigation after %s", async (change) => {
  const pending = deferred<Asset[]>();
  const load = vi.fn(() => pending.promise);
  const { rerender } = renderWithProviders(<Preview load={load} />);
  fireEvent.click(screen.getByRole("button", { name: "Next asset" }));
  expect(load).toHaveBeenCalledTimes(1);
  if (change === "library") {
    act(() =>
      useAppStore.setState({
        current: { id: "other", name: "Other", path: "/other", createdAt: 1 },
      }),
    );
  }
  rerender(
    <Preview
      load={load}
      open={change !== "close"}
      scope={change === "scope" ? "library-a/folder-7" : undefined}
    />,
  );
  await act(async () => pending.resolve([first, last, next]));
  if (change === "library") act(() => useAppStore.setState({ current: null }));
  rerender(<Preview load={load} scope={change === "scope" ? "library-a/folder-7" : undefined} />);
  expect(screen.getByAltText(last.fileName)).toBeVisible();
});

it("can load beyond a single loaded asset", async () => {
  const load = vi.fn().mockResolvedValue([last, next]);
  renderWithProviders(<Preview load={load} singleton />);
  fireEvent.keyDown(screen.getByRole("dialog"), { key: "ArrowRight" });
  expect(await screen.findByAltText(next.fileName)).toBeVisible();
  expect(load).toHaveBeenCalledTimes(1);
});
