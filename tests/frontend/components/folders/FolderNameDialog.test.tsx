import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import FolderNameDialog from "@/components/folders/FolderNameDialog";
import { renderWithProviders, screen } from "@test/helpers/render";

describe("FolderNameDialog", () => {
  it("keeps the dialog open when submission fails", async () => {
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    renderWithProviders(
      <FolderNameDialog
        open
        mode="rename"
        initialName="Existing"
        onOpenChange={onOpenChange}
        onSubmit={() => Promise.reject(new Error("duplicate"))}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Confirm" }));

    expect(onOpenChange).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
});
