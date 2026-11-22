import userEvent from "@testing-library/user-event";
import { Route, Routes } from "react-router-dom";
import { act, screen, waitFor, renderWithProviders } from "@test/helpers/render";
import { describe, expect, it, vi } from "vitest";
import UsageTips from "@/components/search/UsageTips";
import { loadTipsState, prepareFirstUseTips, saveTipsState, usageTips } from "@/lib/usageTips";

vi.mock("@/components/settings/SettingsDialog", () => ({
  default: ({ initialSection }: { initialSection?: string }) => (
    <div role="dialog" aria-label="Settings">
      {initialSection}
    </div>
  ),
}));

describe("Usage tips", () => {
  it("keeps the entry quiet for existing users and opens tips on request", async () => {
    const user = userEvent.setup();
    renderWithProviders(<UsageTips />);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Usage tips/ }));
    expect(screen.getByRole("heading", { name: "Describe a scene to find it" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Previous tip" })).toBeDisabled();
    expect(screen.queryByRole("img", { name: "New tips" })).not.toBeInTheDocument();
    expect(loadTipsState().seen.subtitle).toBeUndefined();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Usage tips/ })).toHaveFocus();
  });

  it("shows first-use tips once, waits for recovery, and stays closed on remount", async () => {
    prepareFirstUseTips();
    const blocker = document.createElement("div");
    blocker.setAttribute("role", "alertdialog");
    document.body.append(blocker);
    const view = renderWithProviders(<UsageTips />);
    expect(screen.queryByRole("heading")).not.toBeInTheDocument();
    blocker.remove();
    expect(
      await screen.findByRole("heading", { name: "Describe a scene to find it" }),
    ).toBeVisible();
    expect(loadTipsState().presented).toBe(true);
    await userEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    view.unmount();
    prepareFirstUseTips();
    renderWithProviders(<UsageTips />);
    expect(loadTipsState().pending).toBe(false);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("shows first-use tips while thumbnails and background progress keep updating", async () => {
    vi.useFakeTimers();
    prepareFirstUseTips();
    const view = renderWithProviders(
      <>
        <UsageTips />
        <output aria-label="Import progress">0</output>
      </>,
    );
    for (let progress = 1; progress <= 6; progress++) {
      await act(async () => {
        screen.getByLabelText("Import progress").textContent = String(progress);
      });
      await act(async () => {
        await vi.advanceTimersByTimeAsync(100);
      });
    }
    expect(screen.getByRole("heading", { name: "Describe a scene to find it" })).toBeVisible();
    expect(loadTipsState().presented).toBe(true);
    view.unmount();
  });

  it("defers the pending tip if a recovery dialog opens during the delay", async () => {
    vi.useFakeTimers();
    prepareFirstUseTips();
    const view = renderWithProviders(<UsageTips />);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });
    const blocker = document.createElement("div");
    blocker.setAttribute("role", "dialog");
    blocker.setAttribute("data-state", "open");
    await act(async () => {
      document.body.append(blocker);
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(500);
    });
    expect(screen.queryByRole("heading")).not.toBeInTheDocument();
    expect(loadTipsState().pending).toBe(true);
    await act(async () => {
      blocker.remove();
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(500);
    });
    expect(screen.getByRole("heading", { name: "Describe a scene to find it" })).toBeVisible();
    view.unmount();
  });

  it("saves reading progress, exposes each tip and opens storage settings", async () => {
    const user = userEvent.setup();
    renderWithProviders(<UsageTips />);
    await user.click(screen.getByRole("button", { name: /Usage tips/ }));
    await user.click(screen.getByRole("button", { name: "Next tip" }));
    expect(screen.getByRole("heading", { name: "Drop files into a category" })).toBeVisible();
    expect(loadTipsState().lastId).toBe("organize");
    await user.click(screen.getByRole("button", { name: "Next tip" }));
    await user.click(screen.getByRole("button", { name: "Open settings" }));
    expect(screen.getByRole("dialog", { name: "Settings" })).toHaveTextContent("settings-storage");
    expect(screen.queryByRole("dialog", { name: "Usage tips" })).not.toBeInTheDocument();
  });

  it("marks only read tips and surfaces new revisions without auto-opening", async () => {
    const seen = Object.fromEntries(usageTips.map((tip) => [tip.id, tip.revision]));
    saveTipsState({
      pending: false,
      presented: true,
      lastId: "image",
      announced: { ...seen, organize: 0 },
      seen: { ...seen, organize: 0 },
    });
    renderWithProviders(<UsageTips />);
    expect(screen.getByRole("img", { name: "New tips" })).toBeVisible();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Usage tips/ }));
    expect(screen.getByRole("heading", { name: "Drop files into a category" })).toBeVisible();
    await waitFor(() =>
      expect(screen.queryByRole("img", { name: "New tips" })).not.toBeInTheDocument(),
    );
  });

  it("opens search and focuses its input when invoked from another page", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <>
        <UsageTips />
        <Routes>
          <Route path="/assets" element={<div>Assets</div>} />
          <Route
            path="/search"
            element={
              <div data-usage-search>
                <input aria-label="Search assets" />
              </div>
            }
          />
        </Routes>
      </>,
      { route: "/assets" },
    );
    await user.click(screen.getByRole("button", { name: "Usage tips" }));
    await user.click(screen.getByRole("button", { name: "Try a search" }));
    await waitFor(() =>
      expect(screen.getByRole("textbox", { name: "Search assets" })).toHaveFocus(),
    );
  });

  it("recovers corrupt local state and still permits manual reading", async () => {
    localStorage.setItem("clipstudio-open:usage-tips", "{bad json");
    renderWithProviders(<UsageTips />);
    await userEvent.click(screen.getByRole("button", { name: /Usage tips/ }));
    expect(screen.getByRole("heading", { name: "Describe a scene to find it" })).toBeVisible();
  });
});
