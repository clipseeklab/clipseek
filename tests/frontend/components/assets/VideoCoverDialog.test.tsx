import { act, screen, waitFor, renderWithProviders } from "@test/helpers/render";
import { buildAsset } from "@test/fixtures/assets";
import { stubConvertFileSrc } from "@test/helpers/tauri";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import type { AssetDetail } from "@/types";
const { save } = vi.hoisted(() => ({ save: vi.fn() }));
vi.mock("@/lib/api", () => ({ api: { assetSetCover: save } }));
import VideoCoverDialog from "@/components/assets/VideoCoverDialog";

const asset = (count = 12): AssetDetail => ({
  ...buildAsset({ id: 7, kind: "video", thumbPath: "/library/frame-2.png" }),
  frames: Array.from({ length: count }, (_, seq) => ({
    id: seq + 100,
    uid: `frame-${seq}`,
    seq,
    timestampMs: seq * 1000,
    absPath: `/library/frame-${seq}.png`,
  })),
});
beforeEach(async () => {
  stubConvertFileSrc();
  await i18n.changeLanguage("zh-CN");
});
function setup(detail = asset()) {
  const onSaved = vi.fn();
  const onOpenChange = vi.fn();
  const view = renderWithProviders(
    <VideoCoverDialog asset={detail} open onSaved={onSaved} onOpenChange={onOpenChange} />,
  );
  return { ...view, onSaved, onOpenChange, user: userEvent.setup() };
}

it("orders candidates by sequence, limits to ten, and selects the current cover", () => {
  const detail = asset();
  detail.frames.reverse();
  setup(detail);
  const radios = screen.getAllByRole("radio");
  expect(radios).toHaveLength(10);
  expect(radios[0]).toHaveAccessibleName(/第 1 个关键帧，0:00/);
  expect(radios[2]).toBeChecked();
  expect(radios[9]).toHaveAttribute("value", "109");
});

it("only saves the chosen frame on confirmation", async () => {
  const { user, onSaved, onOpenChange } = setup();
  const updated = buildAsset({ id: 7, kind: "video", thumbPath: "/library/frame-9.png" });
  save.mockResolvedValue(updated);
  await user.click(screen.getAllByRole("radio")[9]);
  expect(save).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "设为封面" }));
  expect(save).toHaveBeenCalledExactlyOnceWith(7, 109);
  await waitFor(() => expect(onSaved).toHaveBeenCalledWith(updated));
  expect(onOpenChange).toHaveBeenCalledWith(false);
});

it("keeps a manual choice on detail refresh and resets to the latest cover on reopening", async () => {
  const { user, rerender, onSaved, onOpenChange } = setup();
  await user.click(screen.getAllByRole("radio")[5]);
  const detail = { ...asset(), thumbPath: "/library/frame-3.png" };
  rerender(<VideoCoverDialog asset={detail} open onSaved={onSaved} onOpenChange={onOpenChange} />);
  expect(screen.getAllByRole("radio")[5]).toBeChecked();
  rerender(
    <VideoCoverDialog asset={detail} open={false} onSaved={onSaved} onOpenChange={onOpenChange} />,
  );
  rerender(<VideoCoverDialog asset={detail} open onSaved={onSaved} onOpenChange={onOpenChange} />);
  expect(screen.getAllByRole("radio")[3]).toBeChecked();
  expect(save).not.toHaveBeenCalled();
});

it("cancel does not change the cover", async () => {
  const { user, onSaved, onOpenChange } = setup();
  await user.click(screen.getAllByRole("radio")[5]);
  await user.click(screen.getByRole("button", { name: "取消" }));
  expect(save).not.toHaveBeenCalled();
  expect(onSaved).not.toHaveBeenCalled();
  expect(onOpenChange).toHaveBeenCalledWith(false);
});

it("supports fewer than ten frames and does not promote frame eleven when an earlier path is missing", () => {
  const detail = asset();
  detail.frames[0].absPath = null;
  setup(detail);
  expect(screen.getAllByRole("radio")).toHaveLength(10);
  expect(screen.getAllByRole("radio")[0]).toBeDisabled();
  expect(screen.queryByDisplayValue("110")).not.toBeInTheDocument();
});

it("disables confirmation when no frames are available", () => {
  setup(asset(0));
  expect(screen.getByText("暂无可用的关键帧封面。")).toBeVisible();
  expect(screen.getByRole("button", { name: "设为封面" })).toBeDisabled();
});

it("keeps the selection on failure and allows retry", async () => {
  const { user, onSaved, onOpenChange } = setup(asset(3));
  save.mockRejectedValueOnce({ code: "cover_frame_unavailable", message: "missing" });
  await user.click(screen.getAllByRole("radio")[1]);
  await user.click(screen.getByRole("button", { name: "设为封面" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("所选关键帧已丢失或损坏");
  expect(screen.getAllByRole("radio")[1]).toBeChecked();
  expect(onSaved).not.toHaveBeenCalled();
  expect(onOpenChange).not.toHaveBeenCalled();
  save.mockResolvedValue(buildAsset({ id: 7 }));
  await user.click(screen.getByRole("button", { name: "设为封面" }));
  await waitFor(() => expect(onSaved).toHaveBeenCalledOnce());
});

it("prevents duplicate saves and dismissal while pending, and ignores a response after switching assets", async () => {
  let finish!: (value: ReturnType<typeof buildAsset>) => void;
  save.mockReturnValue(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  const { user, onSaved, onOpenChange, rerender } = setup();
  await user.click(screen.getByRole("button", { name: "设为封面" }));
  expect(screen.getByRole("button", { name: "保存中…" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "取消" })).toBeDisabled();
  await user.keyboard("{Escape}");
  expect(onOpenChange).not.toHaveBeenCalled();
  expect(save).toHaveBeenCalledOnce();
  rerender(
    <VideoCoverDialog
      asset={{ ...asset(), id: 8 }}
      open
      onSaved={onSaved}
      onOpenChange={onOpenChange}
    />,
  );
  await act(async () => {
    finish(buildAsset({ id: 7 }));
  });
  expect(onSaved).not.toHaveBeenCalled();
  expect(onOpenChange).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "设为封面" })).toBeEnabled();
});
