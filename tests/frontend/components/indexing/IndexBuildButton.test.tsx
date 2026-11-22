import userEvent from "@testing-library/user-event";
import { act, screen, waitFor, renderWithProviders } from "@test/helpers/render";
import { beforeEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({ indexBuild: vi.fn(), modelStatus: vi.fn(), success: vi.fn() }));
vi.mock("@/lib/api", () => ({
  api: { indexBuild: mocks.indexBuild, modelStatus: mocks.modelStatus },
}));
vi.mock("@/components/Toaster", () => ({ toast: { success: mocks.success } }));
vi.mock("@/components/ModelDownloadGate", () => ({
  default: () => <div>Download visual models</div>,
}));
import { useAppStore } from "@/stores/app";
import IndexBuildButton from "@/components/indexing/IndexBuildButton";

describe("index build controls", () => {
  beforeEach(() => {
    mocks.indexBuild.mockResolvedValue(4);
    mocks.modelStatus.mockResolvedValue({ ready: true });
  });
  it("submits the complete folder scope and selected index types", async () => {
    const user = userEvent.setup();
    renderWithProviders(<IndexBuildButton scope={{ folderIds: [8, 9] }} />);
    await user.click(screen.getByRole("button", { name: "Build search indexes" }));
    expect(screen.getByText(/Only missing indexes/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Build indexes" }));
    await waitFor(() =>
      expect(mocks.indexBuild).toHaveBeenCalledWith({ folderIds: [8, 9] }, ["visual"]),
    );
    expect(mocks.success).toHaveBeenCalledWith("Queued 4 indexing tasks");
  });
  it("offers model download on demand before enqueuing visual work", async () => {
    mocks.modelStatus.mockResolvedValue({ ready: false });
    const user = userEvent.setup();
    renderWithProviders(<IndexBuildButton scope={{ assetIds: [5] }} visualOnly />);
    await user.click(screen.getByRole("button", { name: "Build search indexes" }));
    await user.click(screen.getByRole("button", { name: "Build indexes" }));
    expect(await screen.findByText("Download visual models")).toBeInTheDocument();
    expect(mocks.indexBuild).not.toHaveBeenCalled();
  });
  it("keeps a failed submission retryable", async () => {
    mocks.indexBuild.mockRejectedValueOnce({ code: "operation_busy", message: "busy" });
    const user = userEvent.setup();
    renderWithProviders(<IndexBuildButton scope={{ tagIds: [2] }} />);
    await user.click(screen.getByRole("button", { name: "Build search indexes" }));
    await user.click(screen.getByRole("button", { name: "Build indexes" }));
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Build indexes" }));
    await waitFor(() => expect(mocks.indexBuild).toHaveBeenCalledTimes(2));
  });
});

it("does not submit the old scope to a newly selected library after model checking", async () => {
  let ready!: (value: { ready: boolean }) => void;
  mocks.indexBuild.mockReset();
  mocks.modelStatus.mockReset();
  mocks.modelStatus.mockReturnValueOnce(
    new Promise<{ ready: boolean }>((resolve) => {
      ready = resolve;
    }),
  );
  useAppStore.setState({ current: { id: "a", name: "a", path: "/a", createdAt: 1 } });
  const user = userEvent.setup();
  renderWithProviders(<IndexBuildButton scope={{ assetIds: [5] }} visualOnly />);
  await user.click(screen.getByRole("button", { name: "Build search indexes" }));
  await user.click(screen.getByRole("button", { name: "Build indexes" }));
  await act(async () => {
    useAppStore.setState({ current: { id: "b", name: "b", path: "/b", createdAt: 1 } });
    ready({ ready: true });
  });
  expect(mocks.indexBuild).not.toHaveBeenCalled();
});
