import userEvent from "@testing-library/user-event";
import { fireEvent, renderWithProviders, screen, waitFor } from "@test/helpers/render";
import { useAppStore } from "@/stores/app";
import i18n from "@/i18n";
import { beforeEach, describe, expect, it, vi } from "vitest";

const windowMock = vi.hoisted(() => ({
  startDragging: vi.fn().mockResolvedValue(undefined),
  isMaximized: vi.fn().mockResolvedValue(false),
  onResized: vi.fn().mockResolvedValue(() => {}),
  toggleMaximize: vi.fn().mockResolvedValue(undefined),
  minimize: vi.fn().mockResolvedValue(undefined),
  hide: vi.fn().mockResolvedValue(undefined),
  close: vi.fn().mockResolvedValue(undefined),
}));
const traySetLabels = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));

vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => windowMock }));
vi.mock("@/lib/api", () => ({ api: { traySetLabels } }));
vi.mock("@/components/import/ImportMenuButton", async () => {
  const { createPortal } = await import("react-dom");
  return {
    default: () => (
      <>
        <button type="button">Import menu</button>
        {createPortal(<button type="button">Portal menu item</button>, document.body)}
      </>
    ),
  };
});

import WindowsTitleBar from "@/components/layout/WindowsTitleBar";

describe("WindowsTitleBar", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
    traySetLabels.mockClear();
    useAppStore.setState({ sidebarHidden: false });
    windowMock.isMaximized.mockResolvedValue(false);
    windowMock.startDragging.mockClear();
    windowMock.hide.mockClear();
    windowMock.close.mockClear();
  });

  it("shows the current page and keeps window controls separate from dragging", async () => {
    const user = userEvent.setup();
    renderWithProviders(<WindowsTitleBar mainActions />, { route: "/assets" });

    expect(screen.getByText("Assets")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Import menu" })).toBeInTheDocument();
    fireEvent.mouseDown(screen.getByText("Assets"), { button: 0 });
    expect(windowMock.startDragging).toHaveBeenCalledOnce();
    const dragArea = screen.getByText("Assets").parentElement!;
    expect(dragArea).not.toHaveAttribute("data-tauri-drag-region");
    fireEvent.mouseDown(dragArea, { button: 0 });
    expect(windowMock.startDragging).toHaveBeenCalledTimes(2);

    await user.click(screen.getByRole("button", { name: "Minimize" }));
    await user.click(screen.getByRole("button", { name: "Maximize" }));
    await waitFor(() => expect(windowMock.toggleMaximize).toHaveBeenCalledOnce());
    expect(windowMock.minimize).toHaveBeenCalledOnce();
    expect(windowMock.startDragging).toHaveBeenCalledTimes(2);
  });

  it("keeps the app running when the close button is clicked", async () => {
    const user = userEvent.setup();
    renderWithProviders(<WindowsTitleBar mainActions />, { route: "/assets" });

    await user.click(screen.getByRole("button", { name: "Close to tray" }));

    expect(windowMock.hide).toHaveBeenCalledOnce();
    expect(windowMock.close).not.toHaveBeenCalled();
  });

  it("does not start dragging when a portal menu item receives a mouse press", () => {
    renderWithProviders(<WindowsTitleBar mainActions />);

    fireEvent.mouseDown(screen.getByRole("button", { name: "Portal menu item" }), { button: 0 });
    fireEvent.doubleClick(screen.getByRole("button", { name: "Portal menu item" }));

    expect(windowMock.startDragging).not.toHaveBeenCalled();
    expect(windowMock.toggleMaximize).not.toHaveBeenCalled();
  });

  it("updates the tray menu when the interface language changes", async () => {
    renderWithProviders(<WindowsTitleBar mainActions />, { route: "/assets" });
    await waitFor(() =>
      expect(traySetLabels).toHaveBeenCalledWith("Show main window", "Quit ClipSeek Open"),
    );

    await i18n.changeLanguage("zh");
    await waitFor(() =>
      expect(traySetLabels).toHaveBeenCalledWith("显示主窗口", "退出 ClipSeek Open"),
    );
  });

  it("toggles the sidebar from the titlebar", async () => {
    const user = userEvent.setup();
    renderWithProviders(<WindowsTitleBar mainActions />, { route: "/folders" });

    await user.click(screen.getByRole("button", { name: "Hide sidebar" }));
    expect(useAppStore.getState().sidebarHidden).toBe(true);
    expect(screen.getByRole("button", { name: "Show sidebar" })).toBeInTheDocument();
  });
});
