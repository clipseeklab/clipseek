import { queryKeys } from "@/lib/queryKeys";
/** Trash retention badges, individual/bulk restore, permanent deletion, and empty confirmation
 * coverage.
 */
import userEvent from "@testing-library/user-event";
import { screen, waitFor, within } from "@test/helpers/render";
import { renderWithProviders } from "@test/helpers/render";
import { buildAsset } from "@test/fixtures/assets";
import { useAppStore } from "@/stores/app";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  useAssetPages: vi.fn(),
  assetRestore: vi.fn(),
  assetDeletePermanent: vi.fn(),
  trashPurgeAll: vi.fn(),
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock("@/hooks/useAssetPages", () => ({ useAssetPages: mocks.useAssetPages }));
vi.mock("@/lib/api", () => ({
  api: {
    assetRestore: mocks.assetRestore,
    assetDeletePermanent: mocks.assetDeletePermanent,
    trashPurgeAll: mocks.trashPurgeAll,
  },
}));
vi.mock("@/components/Toaster", () => ({
  toast: { success: mocks.toastSuccess, error: mocks.toastError },
}));

import TrashPage from "@/pages/TrashPage";
import { stubConvertFileSrc } from "@test/helpers/tauri";

const DAY = 86_400;

function trashItems() {
  const now = Math.floor(Date.now() / 1000);
  return [
    buildAsset({ id: 1, fileName: "old-post.jpg", deletedAt: now - 28 * DAY }),
    buildAsset({
      id: 2,
      fileName: "recent-clip.mp4",
      kind: "video",
      mimeType: "video/mp4",
      deletedAt: now - DAY,
    }),
  ];
}

function pageResult(overrides: Record<string, unknown> = {}) {
  return {
    items: trashItems(),
    total: 2,
    isLoading: false,
    isError: false,
    isFetchingNextPage: false,
    fetchNextPage: vi.fn(),
    hasNextPage: false,
    refetch: vi.fn(),
    ...overrides,
  };
}

describe("TrashPage", () => {
  it("uses a placeholder for a video without a thumbnail", () => {
    mocks.useAssetPages.mockReturnValue(
      pageResult({
        items: [buildAsset({ kind: "video", thumbPath: null, absPath: "C:/big.mkv" })],
      }),
    );
    const { container } = renderWithProviders(<TrashPage />);
    expect(container.querySelector("img")).toBeNull();
  });
  beforeEach(() => {
    stubConvertFileSrc();
    useAppStore.setState(useAppStore.getInitialState(), true);
    mocks.useAssetPages.mockReturnValue(pageResult());
    mocks.assetRestore.mockResolvedValue(undefined);
    mocks.assetDeletePermanent.mockResolvedValue(undefined);
    mocks.trashPurgeAll.mockResolvedValue(undefined);
  });

  it("shows the empty state when there is nothing trashed", () => {
    mocks.useAssetPages.mockReturnValue(pageResult({ items: [], total: 0 }));
    renderWithProviders(<TrashPage />);

    expect(screen.getByText("Trash is empty")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Empty Trash" })).not.toBeInTheDocument();
  });

  it("flags assets close to automatic cleanup", async () => {
    renderWithProviders(<TrashPage />);

    expect(await screen.findByText("Expiring soon")).toBeInTheDocument();
    expect(screen.getByText("Auto-deletes in 29 days")).toBeInTheDocument();
    expect(screen.getByText("2 total")).toBeInTheDocument();
  });

  it("restores a single asset and refreshes the queries", async () => {
    const user = userEvent.setup();
    const { queryClient } = renderWithProviders(<TrashPage />);
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");

    const restoreButtons = await screen.findAllByRole("button", { name: "Restore" });
    await user.click(restoreButtons[0]);

    await waitFor(() => expect(mocks.assetRestore).toHaveBeenCalledWith([1]));
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.assets.library(useAppStore.getState().current?.id),
    });
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.folders.library(useAppStore.getState().current?.id),
    });
    expect(mocks.toastSuccess).toHaveBeenCalledWith("Done");
  });

  it("purges a single asset without a success toast", async () => {
    const user = userEvent.setup();
    const { queryClient } = renderWithProviders(<TrashPage />);
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");

    const purgeButtons = await screen.findAllByRole("button", { name: "Delete Permanently" });
    await user.click(purgeButtons[0]);

    await waitFor(() => expect(mocks.assetDeletePermanent).toHaveBeenCalledWith([1]));
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: queryKeys.assets.library(useAppStore.getState().current?.id),
    });
    expect(mocks.toastSuccess).not.toHaveBeenCalled();
  });

  it("restores every listed asset at once", async () => {
    const user = userEvent.setup();
    renderWithProviders(<TrashPage />);

    await user.click(await screen.findByRole("button", { name: "Restore All" }));

    await waitFor(() => expect(mocks.assetRestore).toHaveBeenCalledWith([1, 2]));
  });

  it("empties the trash only after confirmation", async () => {
    const user = userEvent.setup();
    renderWithProviders(<TrashPage />);

    await user.click(await screen.findByRole("button", { name: "Empty Trash" }));
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent("Permanent deletion cannot be undone. Delete 2 assets?");

    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(mocks.trashPurgeAll).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Empty Trash" }));
    const confirm = await screen.findByRole("alertdialog");
    await user.click(within(confirm).getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(mocks.trashPurgeAll).toHaveBeenCalledOnce());
    expect(mocks.toastSuccess).toHaveBeenCalledWith("Done");
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
  });

  it("closes the confirmation and reports the error when emptying fails", async () => {
    const user = userEvent.setup();
    mocks.trashPurgeAll.mockRejectedValue(new Error("disk error"));
    renderWithProviders(<TrashPage />);

    await user.click(await screen.findByRole("button", { name: "Empty Trash" }));
    const dialog = await screen.findByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledOnce());
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
  });
});
