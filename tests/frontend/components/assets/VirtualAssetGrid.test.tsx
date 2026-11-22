import { createRef } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, fireEvent, renderWithProviders, screen } from "@test/helpers/render";
import { buildAsset } from "@test/fixtures/assets";
import { useAppStore } from "@/stores/app";
import VirtualAssetGrid from "@/components/assets/VirtualAssetGrid";

let top = 0;
let width = 1000;
let resize: ResizeObserverCallback;
let nextFrame: FrameRequestCallback | undefined;
const disconnect = vi.fn();
afterEach(() => vi.unstubAllGlobals());
beforeEach(() => {
  top = 0;
  width = 1000;
  nextFrame = undefined;
  useAppStore.setState({ gridColumns: 4 });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (
    this: HTMLElement,
  ) {
    return {
      x: 0,
      y: this.getAttribute("aria-label") === "Asset viewport" ? 0 : -top,
      top: this.getAttribute("aria-label") === "Asset viewport" ? 0 : -top,
      left: 0,
      right: width,
      bottom: 600,
      width,
      height: 600,
      toJSON: () => ({}),
    };
  });
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(600);
  vi.spyOn(window, "requestAnimationFrame").mockImplementation((callback) => {
    nextFrame = callback;
    return 1;
  });
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(callback: ResizeObserverCallback) {
        resize = callback;
      }
      observe() {}
      disconnect = disconnect;
    },
  );
  disconnect.mockClear();
});

function show(count = 1000, hasMore = false, loading = false) {
  const ref = createRef<HTMLDivElement>();
  const load = vi.fn();
  const items = Array.from({ length: count }, (_, id) =>
    buildAsset({ id, fileName: `photo-${id}`, width: 100, height: 100 }),
  );
  const result = renderWithProviders(
    <div ref={ref} aria-label="Asset viewport">
      <VirtualAssetGrid
        items={items}
        scrollRef={ref}
        renderItem={(asset) => <span>{asset.fileName}</span>}
        hasMore={hasMore}
        isLoadingMore={loading}
        onLoadMore={load}
      />
    </div>,
  );
  return { ...result, load, ref };
}

it("renders a bounded window and replaces visible cards after scrolling", () => {
  const { ref } = show();
  expect(screen.getByText("photo-0")).toBeVisible();
  expect(screen.queryByText("photo-999")).not.toBeInTheDocument();
  expect(screen.getAllByText(/^photo-/).length).toBeLessThan(100);
  top = 20_000;
  fireEvent.scroll(ref.current!);
  act(() => nextFrame?.(0));
  expect(screen.queryByText("photo-0")).not.toBeInTheDocument();
  expect(screen.getAllByText(/^photo-/).length).toBeLessThan(100);
});

it.each([
  [true, false, 1],
  [true, true, 0],
  [false, false, 0],
])(
  "loads another page only when more data exists and no request is running (%s, %s)",
  (hasMore, loading, calls) => {
    const { load } = show(4, hasMore, loading);
    expect(load).toHaveBeenCalledTimes(calls);
  },
);

it("measures a resized viewport and releases observers and scroll handlers on unmount", () => {
  const remove = vi.spyOn(HTMLElement.prototype, "removeEventListener");
  const { unmount } = show();
  const previousCount = screen.getAllByText(/^photo-/).length;
  width = 600;
  act(() => resize([], {} as ResizeObserver));
  act(() => nextFrame?.(0));
  expect(screen.getByText("photo-0")).toBeVisible();
  expect(screen.getAllByText(/^photo-/).length).toBeGreaterThan(previousCount);
  unmount();
  expect(disconnect).toHaveBeenCalled();
  expect(remove).toHaveBeenCalledWith("scroll", expect.any(Function));
});
