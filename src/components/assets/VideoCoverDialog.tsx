import { useEffect, useEffectEvent, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { convertFileSrc } from "@tauri-apps/api/core";
import { api } from "@/lib/api";
import { explainError } from "@/lib/errors";
import { formatDuration } from "@/lib/utils";
import type { Asset, AssetDetail } from "@/types";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
export default function VideoCoverDialog({
  asset,
  open,
  onOpenChange,
  onSaved,
}: {
  asset: AssetDetail;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: (asset: Asset) => void;
}) {
  const { t } = useTranslation();
  const group = useId();
  const frames = [...asset.frames].sort((a, b) => a.seq - b.seq || a.id - b.id).slice(0, 10);
  const [selected, setSelected] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const generation = useRef(0);
  const pending = useRef(false);
  const resetSelection = useEffectEvent(() => {
    generation.current += 1;
    pending.current = false;
    setSaving(false);
    setError(null);
    setSelected(
      frames.find((frame) => frame.absPath && frame.absPath === asset.thumbPath)?.id ??
        frames.find((frame) => frame.absPath)?.id ??
        null,
    );
  });
  useEffect(() => {
    // Reset for each opening or asset switch, not on unrelated detail refreshes.
    resetSelection();
    return () => {
      generation.current += 1;
    };
  }, [asset.id, open]);
  async function save() {
    if (selected === null || pending.current) return;
    const request = generation.current;
    pending.current = true;
    setSaving(true);
    setError(null);
    try {
      const updated = await api.assetSetCover(asset.id, selected);
      if (generation.current !== request) return;
      onSaved(updated);
      onOpenChange(false);
    } catch (cause) {
      if (generation.current === request) setError(explainError(cause));
    } finally {
      if (generation.current === request) {
        pending.current = false;
        setSaving(false);
      }
    }
  }
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!pending.current) onOpenChange(next);
      }}
    >
      <DialogContent
        className="max-h-[85vh] overflow-y-auto sm:max-w-3xl"
        aria-busy={saving}
        showCloseButton={!saving}
      >
        <DialogHeader>
          <DialogTitle>{t("assets.replaceCover")}</DialogTitle>
          <DialogDescription>{t("assets.coverHint")}</DialogDescription>
        </DialogHeader>
        <div
          role="radiogroup"
          aria-label={t("assets.replaceCover")}
          className="grid grid-cols-2 gap-3 sm:grid-cols-5"
        >
          {frames.map((frame, index) => (
            <label key={frame.id} className="relative min-w-0 cursor-pointer">
              <input
                type="radio"
                name={group}
                value={frame.id}
                checked={selected === frame.id}
                disabled={!frame.absPath || saving}
                onChange={() => setSelected(frame.id)}
                aria-label={t("assets.coverFrame", {
                  index: index + 1,
                  time: formatDuration(frame.timestampMs),
                })}
                className="peer sr-only"
              />
              <div className="overflow-hidden rounded-md border-2 border-transparent bg-muted peer-checked:border-primary peer-focus-visible:ring-2 peer-focus-visible:ring-ring peer-focus-visible:ring-offset-2 peer-disabled:cursor-not-allowed peer-disabled:opacity-40">
                {frame.absPath ? (
                  <img
                    src={convertFileSrc(frame.absPath)}
                    alt=""
                    className="aspect-video w-full object-cover"
                  />
                ) : (
                  <div className="aspect-video" />
                )}
                <div className="px-2 py-1 text-right text-xs tabular-nums">
                  {formatDuration(frame.timestampMs)}
                </div>
              </div>
            </label>
          ))}
        </div>
        {!frames.some((frame) => frame.absPath) && (
          <p className="text-sm text-muted-foreground">{t("assets.noCoverFrames")}</p>
        )}
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
        <DialogFooter>
          <Button variant="outline" disabled={saving} onClick={() => onOpenChange(false)}>
            {t("common.cancel")}
          </Button>
          <Button disabled={saving || selected === null} onClick={() => void save()}>
            {t(saving ? "assets.savingCover" : "assets.useAsCover")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
