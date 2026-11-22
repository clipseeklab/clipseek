/** Library creation dialog shows only the creation form, not existing registrations. */
import { useEffect, useEffectEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { homeDir } from "@tauri-apps/api/path";
import { FolderOpen } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/components/Toaster";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  DEFAULT_LIBRARY_COLOR,
  LibraryColorIcon,
  LibraryColorPicker,
} from "@/components/library/LibraryBadge";
import { useAppStore } from "@/stores/app";
import { errorCode, explainError } from "@/lib/errors";
import { libraryPath as buildLibraryPath, selectedDirectoryName } from "@/lib/libraryPath";
import type { LibraryMeta } from "@/types";
import { useLibraryNameInput } from "@/hooks/useLibraryNameInput";
export default function CreateLibraryDialog({
  open,
  onOpenChange,
  onCreated,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated?: (library: LibraryMeta) => Promise<void>;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { setCurrent, setLibraries } = useAppStore();
  const [name, setName] = useState("");
  const [parentPath, setParentPath] = useState("");
  const [color, setColor] = useState(DEFAULT_LIBRARY_COLOR);
  const [busy, setBusy] = useState(false);
  const [createError, setCreateError] = useState<{
    message: string;
    code?: string;
  } | null>(null);
  const nameInput = useLibraryNameInput(name, (value) => {
    setName(value);
    setCreateError(null);
  });
  const libraryPath = buildLibraryPath(parentPath, name);
  const resetNameInput = useEffectEvent(() => nameInput.reset());
  useEffect(() => {
    let disposed = false;
    if (open && !parentPath) {
      homeDir()
        .then((path) => {
          if (!disposed) setParentPath((current) => current || path);
        })
        .catch(() => {});
    }
    if (!open) {
      resetNameInput();
      setName("");
      setParentPath("");
      setColor(DEFAULT_LIBRARY_COLOR);
      setBusy(false);
      setCreateError(null);
    }
    return () => {
      disposed = true;
    };
  }, [open, parentPath]);
  const choosePath = async () => {
    try {
      const picked = await openDialog({ directory: true, title: t("library.pathLabel") });
      if (typeof picked !== "string") return;
      setParentPath(picked);
      setCreateError(null);
      if (!name.trim()) setName(selectedDirectoryName(picked));
    } catch (error) {
      setCreateError({ message: explainError(error), code: errorCode(error) });
    }
  };
  const create = async () => {
    if (!nameInput.canSubmit() || !parentPath || busy) return;
    setCreateError(null);
    setBusy(true);
    try {
      const library = await api.libraryCreate(name.trim(), libraryPath, color);
      if (onCreated) await onCreated(library);
      else {
        setCurrent(library);
        setLibraries(await api.libraryList());
        await queryClient.invalidateQueries();
      }
      onOpenChange(false);
      toast.success(t("common.operationSuccess"));
    } catch (error) {
      setCreateError({ message: explainError(error), code: errorCode(error) });
      setBusy(false);
    }
  };
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!busy) onOpenChange(next);
      }}
    >
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("library.createTitle")}</DialogTitle>
          <DialogDescription>{t("library.createDesc")}</DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <div className="flex items-center gap-4">
            <LibraryColorIcon color={color} size={32} />
            <LibraryColorPicker value={color} onChange={setColor} />
          </div>
          <Input
            aria-label={t("library.nameLabel")}
            value={name}
            placeholder={t("library.namePlaceholder")}
            disabled={busy}
            {...nameInput.inputProps}
            aria-invalid={!!nameInput.error}
            aria-describedby={nameInput.error ? "create-library-name-error" : undefined}
          />
          {nameInput.error && (
            <p id="create-library-name-error" role="alert" className="text-sm text-destructive">
              {t(`library.nameValidation.${nameInput.error}`)}
            </p>
          )}
          <div className="flex gap-2">
            <Input readOnly value={parentPath} placeholder={t("library.pathPlaceholder")} />
            <Button variant="outline" onClick={() => void choosePath()}>
              <FolderOpen className="h-4 w-4" />
              {t("library.choose")}
            </Button>
          </div>
          {parentPath && name.trim() && (
            <div className="rounded-md bg-muted px-2.5 py-1.5 font-mono text-xs break-all text-muted-foreground">
              {libraryPath}
            </div>
          )}
          {createError && (
            <div
              role="alert"
              className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive"
            >
              <p>{createError.message}</p>
              {createError.code === "library_dir_not_empty" && (
                <p className="mt-1">{t("library.existingDirectoryHint")}</p>
              )}
            </div>
          )}
          <div className="flex justify-end gap-2">
            <Button variant="outline" disabled={busy} onClick={() => onOpenChange(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              disabled={!nameInput.valid || !parentPath || busy}
              onClick={() => void create()}
            >
              {busy ? t("library.creating") : t("library.createBtn")}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
