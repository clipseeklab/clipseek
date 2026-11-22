/** Shared create/rename dialog; Tauri WebViews require a custom dialog instead of window.prompt. */
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
export default function FolderNameDialog({
  open,
  onOpenChange,
  mode,
  initialName = "",
  onSubmit,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  mode: "create" | "rename";
  initialName?: string;
  onSubmit: (name: string) => Promise<void> | void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState(initialName);
  useEffect(() => {
    if (open) setName(initialName);
  }, [open, initialName]);
  const submit = async () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    try {
      await onSubmit(trimmed);
      onOpenChange(false);
    } catch {
      // The caller reports errors with a toast; keep the dialog open for retry.
    }
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent disableAnimation className="max-w-xs">
        <DialogHeader>
          <DialogTitle>{t(mode === "create" ? "folders.create" : "folders.rename")}</DialogTitle>
        </DialogHeader>
        <Input
          autoFocus
          value={name}
          placeholder={t("folders.namePlaceholder")}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void submit();
          }}
        />
        <div className="flex justify-end">
          <Button size="sm" disabled={!name.trim()} onClick={() => void submit()}>
            {t("common.confirm")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
