import { beforeEach, expect, it, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen, waitFor } from "@test/helpers/render";
const mocks = vi.hoisted(() => ({
  status: vi.fn(),
  apply: vi.fn(),
  download: vi.fn(),
  error: vi.fn(),
  success: vi.fn(),
}));
vi.mock("@/lib/api", () => ({
  api: { modelStatus: mocks.status, modelSettings: mocks.apply, modelDownload: mocks.download },
}));
vi.mock("@/components/Toaster", () => ({ toast: { error: mocks.error, success: mocks.success } }));
import EmbeddingSettingsSection from "@/components/settings/EmbeddingSettingsSection";
beforeEach(() => {
  mocks.status.mockResolvedValue({ ready: true, precision: "q8", modelDirectory: "/models" });
  mocks.apply.mockResolvedValue({ ready: true, precision: "q4", modelDirectory: "/models" });
  mocks.download.mockResolvedValue({});
});
it("shows compact model management with Q8 selected and no path editor", async () => {
  renderWithProviders(<EmbeddingSettingsSection />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Q4" })).toBeEnabled());
  expect(screen.getByRole("heading", { name: "Model management" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Q8" })).toHaveAttribute("aria-pressed", "true");
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  expect(screen.queryByText("Metal GPU")).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Download model" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Choose model directory" })).not.toBeInTheDocument();
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Q4" }));
  await user.click(screen.getByRole("button", { name: "Apply model" }));
  await waitFor(() => expect(mocks.apply).toHaveBeenCalledWith("q4", "/models"));
  expect(mocks.download).not.toHaveBeenCalled();
  expect(screen.queryByText(/plugin|subtitle|YouTube/i)).not.toBeInTheDocument();
});
it("reports rejected selection and leaves the form usable", async () => {
  mocks.apply.mockRejectedValueOnce({ code: "operation_busy", message: "busy" });
  renderWithProviders(<EmbeddingSettingsSection />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Q4" })).toBeEnabled());
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "FP32" }));
  await user.click(screen.getByRole("button", { name: "Apply model" }));
  await waitFor(() => expect(mocks.error).toHaveBeenCalled());
  expect(screen.getByRole("button", { name: "Apply model" })).toBeEnabled();
  expect(mocks.download).not.toHaveBeenCalled();
});

it("downloads the selected precision after applying it, instead of downloading the previous model", async () => {
  mocks.apply.mockResolvedValueOnce({ ready: false, precision: "q4", modelDirectory: "/models" });
  renderWithProviders(<EmbeddingSettingsSection />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Q4" })).toBeEnabled());
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Q4" }));
  await user.click(screen.getByRole("button", { name: "Apply model" }));
  await waitFor(() => expect(mocks.download).toHaveBeenCalledOnce());
  expect(mocks.apply).toHaveBeenCalledWith("q4", "/models");
  expect(mocks.apply.mock.invocationCallOrder[0]).toBeLessThan(
    mocks.download.mock.invocationCallOrder[0],
  );
});

it("downloads the current precision when missing without resetting its indexes", async () => {
  mocks.status.mockResolvedValue({ ready: false, precision: "q8", modelDirectory: "/models" });
  renderWithProviders(<EmbeddingSettingsSection />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Apply model" })).toBeEnabled());
  await userEvent.setup().click(screen.getByRole("button", { name: "Apply model" }));
  await waitFor(() => expect(mocks.download).toHaveBeenCalledOnce());
  expect(mocks.apply).not.toHaveBeenCalled();
});
it("keeps an already installed current model without downloads or rebuilding", async () => {
  renderWithProviders(<EmbeddingSettingsSection />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Apply model" })).toBeEnabled());
  await userEvent.setup().click(screen.getByRole("button", { name: "Apply model" }));
  await waitFor(() => expect(mocks.success).toHaveBeenCalled());
  expect(mocks.apply).not.toHaveBeenCalled();
  expect(mocks.download).not.toHaveBeenCalled();
});

it("shows Metal only after the encoder reports an active GPU backend", async () => {
  mocks.status.mockResolvedValue({
    ready: true,
    precision: "q8",
    modelDirectory: "/models",
    execution: { backend: "metal", fallbackReason: null },
  });
  renderWithProviders(<EmbeddingSettingsSection />);
  expect(await screen.findByText("Metal GPU")).toBeVisible();
  expect(screen.queryByText("CPU")).not.toBeInTheDocument();
});
it("shows CPU and the reason when Metal initialization falls back", async () => {
  mocks.status.mockResolvedValue({
    ready: true,
    precision: "q8",
    modelDirectory: "/models",
    execution: {
      backend: "cpu",
      fallbackReason: "Metal initialization failed: adapter unavailable",
    },
  });
  renderWithProviders(<EmbeddingSettingsSection />);
  expect(await screen.findByText("CPU")).toHaveAttribute(
    "title",
    "Metal initialization failed: adapter unavailable",
  );
  expect(screen.queryByText("Metal GPU")).not.toBeInTheDocument();
});

it("shows the active native CoreML GPU backend", async () => {
  mocks.status.mockResolvedValue({
    ready: true,
    precision: "q8",
    modelDirectory: "/models",
    execution: { backend: "coreml", fallbackReason: null },
  });
  renderWithProviders(<EmbeddingSettingsSection />);
  expect(await screen.findByText("CoreML GPU")).toBeVisible();
  expect(screen.queryByText("Metal GPU")).not.toBeInTheDocument();
});
