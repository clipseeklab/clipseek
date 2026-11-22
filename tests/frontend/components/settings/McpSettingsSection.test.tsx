import { beforeEach, expect, it, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen, waitFor } from "@test/helpers/render";
import { useAppStore } from "@/stores/app";
const mocks = vi.hoisted(() => ({
  status: vi.fn(),
  reveal: vi.fn(),
  rotate: vi.fn(),
  error: vi.fn(),
}));
vi.mock("@/lib/api", () => ({
  api: { mcpStatus: mocks.status, mcpTokenReveal: mocks.reveal, mcpTokenRotate: mocks.rotate },
}));
vi.mock("@/components/Toaster", () => ({ toast: { success: vi.fn(), error: mocks.error } }));
import McpSettingsSection from "@/components/settings/McpSettingsSection";
const stopped = {
  enabled: false,
  running: false,
  writeEnabled: false,
  port: 0,
  url: null,
  localUrl: null,
  lanUrl: null,
  listenMode: "loopback",
  error: null,
};
beforeEach(() => {
  useAppStore.setState(useAppStore.getInitialState(), true);
  mocks.status.mockResolvedValue(stopped);
});
it("shows a visible MCP section with local read-only defaults", async () => {
  renderWithProviders(<McpSettingsSection open update={vi.fn()} />);
  expect(await screen.findByText("Stopped")).toBeVisible();
  expect(screen.getByText("MCP interface")).toBeVisible();
  expect(screen.getByRole("switch", { name: "Enable MCP service" })).not.toBeChecked();
  expect(screen.getByRole("switch", { name: "Allow organization and imports" })).toBeDisabled();
  expect(screen.getByRole("combobox")).toHaveTextContent("This computer only");
});
it("enables the service through persisted settings then reloads its connection address", async () => {
  const update = vi.fn(async () => {
    useAppStore.setState({ settings: { ...useAppStore.getState().settings, mcpEnabled: true } });
    mocks.status.mockResolvedValue({
      ...stopped,
      enabled: true,
      running: true,
      port: 12345,
      localUrl: "http://127.0.0.1:12345/mcp",
    });
  });
  renderWithProviders(<McpSettingsSection open update={update} />);
  await screen.findByText("Stopped");
  await userEvent.setup().click(screen.getByRole("switch", { name: "Enable MCP service" }));
  expect(update).toHaveBeenCalledWith({ mcpEnabled: true });
  expect(await screen.findByText("http://127.0.0.1:12345/mcp")).toBeVisible();
  expect(screen.getByRole("button", { name: "Copy access token" })).toBeEnabled();
});
it("keeps status failures actionable and retryable", async () => {
  mocks.status.mockRejectedValueOnce(new Error("status unavailable"));
  renderWithProviders(<McpSettingsSection open update={vi.fn()} />);
  expect(await screen.findByRole("alert")).toBeVisible();
  await userEvent.setup().click(screen.getByRole("button", { name: "Retry" }));
  await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  expect(screen.getByText("Stopped")).toBeVisible();
});
