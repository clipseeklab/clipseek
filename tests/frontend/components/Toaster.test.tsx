import { act, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { toast, Toaster, useToastStore } from "@/components/Toaster";
import { renderWithProviders } from "../helpers/render";

describe("Toaster persistent notifications", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useToastStore.setState({ toasts: [] });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("keeps a global completion notice until the user clicks it", () => {
    const message = "Search index rebuilt. Click to dismiss.";
    act(() => toast.persistentSuccess(message));
    renderWithProviders(<Toaster />);

    act(() => {
      vi.advanceTimersByTime(30_000);
    });
    const notification = screen.getByRole("button", { name: message });
    expect(notification).toBeInTheDocument();

    fireEvent.click(notification);
    expect(screen.queryByText(message)).not.toBeInTheDocument();
  });
});
