import { expect, it, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@test/helpers/render";
import { buildAsset } from "@test/fixtures/assets";
import { ContextMenu, ContextMenuTrigger } from "@/components/ui/context-menu";
import {
  AssetContextMenuContent,
  type AssetMenuActions,
} from "@/components/assets/AssetContextMenu";
vi.mock("@/lib/api", () => ({ api: {} }));
it.each(["asset", "search-result"] as const)(
  "keeps original organization actions and removes retired actions in %s",
  async (context) => {
    const actions: AssetMenuActions = {
      onSimilar: vi.fn(),
      onAddTags: vi.fn(),
      onMove: vi.fn(),
      onBatch: vi.fn(),
      onDetails: vi.fn(),
      onTrash: vi.fn(),
    };
    const asset = buildAsset({ kind: "video", visualIndexed: false, storage: "reference" });
    renderWithProviders(
      <ContextMenu>
        <ContextMenuTrigger>Movie</ContextMenuTrigger>
        <AssetContextMenuContent asset={asset} actions={actions} context={context} />
      </ContextMenu>,
    );
    await userEvent.setup().pointer({ target: screen.getByText("Movie"), keys: "[MouseRight]" });
    expect(screen.getByRole("menuitem", { name: "Find Similar" })).toBeVisible();
    expect(screen.getByRole("menuitem", { name: "Move to Folder" })).toBeVisible();
    expect(
      screen.queryByRole("menuitem", { name: /subtitle|trim|frame|download/i }),
    ).not.toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole("menuitem", { name: "Asset Details" }));
    expect(actions.onDetails).toHaveBeenCalledWith(asset);
  },
);
