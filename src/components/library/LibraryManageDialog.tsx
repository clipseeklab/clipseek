/** Library management supports creation, renaming, relocation, merging, and deletion. */
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { ArrowRight, FolderInput, GitMerge, PenLine, Trash2 } from "lucide-react";
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
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { LibraryMeta } from "@/types";
import { useLibraryOperations } from "@/hooks/useLibraryOperations";
import { OperationStatus } from "@/components/library/OperationStatus";
import { useAppStore } from "@/stores/app";
import { explainError } from "@/lib/errors";
import { withLibraryAuthorization } from "@/lib/libraryAccess";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
interface Props {
  open: boolean;
  onOpenChange: (v: boolean) => void;
}
export default function LibraryManageDialog({ open, onOpenChange }: Props) {
  const { t } = useTranslation();
  const mergeFieldsId = useId();
  const { libraries, current, setLibraries, setCurrent } = useAppStore();
  const queryClient = useQueryClient();
  const [editing, setEditing] = useState<LibraryMeta | null>(null);
  const [editName, setEditName] = useState("");
  const [editColor, setEditColor] = useState(DEFAULT_LIBRARY_COLOR);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const [deletingFiles, setDeletingFiles] = useState(true);
  const [merging, setMerging] = useState<LibraryMeta | null>(null);
  const [mergeTarget, setMergeTarget] = useState<string>("");
  const [mergingBusy, setMergingBusy] = useState(false);
  const { operation, error: operationError, retry: retryOperations } = useLibraryOperations(open);
  const refresh = async () => {
    const libs = await api.libraryList();
    setLibraries(libs);
    const cur = await api.libraryCurrent();
    setCurrent(cur);
    await queryClient.invalidateQueries();
  };
  const rename = async () => {
    if (!editing || !editName.trim()) return;
    try {
      await api.libraryRename(editing.id, editName.trim(), editColor);
      await refresh();
      setEditing(null);
    } catch (e) {
      toast.error(explainError(e));
    }
  };
  const switchTo = async (library: LibraryMeta) => {
    if (library.id === current?.id) return;
    try {
      const selected = await withLibraryAuthorization(
        () => api.librarySwitch(library.id),
        [library],
        (item) => t("library.reauthorizeTitle", { name: item.name }),
      );
      if (!selected) return;
      setCurrent(selected);
      await queryClient.invalidateQueries();
      onOpenChange(false);
    } catch (error) {
      toast.error(explainError(error));
    }
  };
  const migrate = async (id: string) => {
    try {
      const dir = await openDialog({ directory: true, title: t("library.migrate") });
      if (typeof dir !== "string") return;
      await api.libraryMigrate(id, dir);
      await refresh();
      toast.success(t("common.operationSuccess"));
    } catch (e) {
      toast.error(`${explainError(e)} ${t("library.errors.migrateRetry")}`);
    }
  };
  const merge = async () => {
    if (!merging || !mergeTarget) return;
    setMergingBusy(true);
    try {
      const targetLibrary = libraries.find((library) => library.id === mergeTarget);
      const completed = await withLibraryAuthorization(
        () => api.libraryMerge(merging.id, mergeTarget),
        [merging, ...(targetLibrary ? [targetLibrary] : [])],
        (item) => t("library.reauthorizeTitle", { name: item.name }),
      );
      if (!completed) return;
      setMerging(null);
      await refresh();
      toast.success(t("common.operationSuccess"));
    } catch (e) {
      toast.error(`${explainError(e)} ${t("library.errors.mergeRetry")}`);
    } finally {
      setMergingBusy(false);
    }
  };
  const remove = async (id: string, deleteFiles: boolean) => {
    try {
      await api.libraryDelete(id, deleteFiles);
      setConfirmDelete(null);
      await refresh();
    } catch (e) {
      toast.error(explainError(e));
    }
  };
  const target = libraries.find((l) => l.id === confirmDelete);
  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>{t("library.manage")}</DialogTitle>
            <DialogDescription>{t("library.createDesc")}</DialogDescription>
          </DialogHeader>
          {operationError && (
            <div role="alert" className="space-y-2 text-sm text-destructive">
              <p>{operationError}</p>
              <Button variant="outline" size="sm" onClick={retryOperations}>
                {t("common.retry")}
              </Button>
            </div>
          )}
          {operation && operation.status !== "completed" && (
            <OperationStatus progress={operation} />
          )}

          <div className="max-h-72 space-y-1.5 overflow-y-auto">
            {libraries.map((lib) => (
              <div
                key={lib.id}
                role="button"
                tabIndex={0}
                className="flex cursor-pointer items-center gap-2 rounded-md border px-3 py-2 text-sm transition-colors hover:bg-accent/50"
                onClick={() => void switchTo(lib)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") void switchTo(lib);
                }}
              >
                <LibraryColorIcon color={lib.color} size={20} />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate font-medium">{lib.name}</span>
                    {lib.id === current?.id && (
                      <span className="rounded bg-primary px-1.5 py-0.5 text-[10px] text-primary-foreground">
                        ●
                      </span>
                    )}
                  </div>
                  <div className="truncate text-xs text-muted-foreground">{lib.path}</div>
                </div>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7"
                  title={t("library.rename")}
                  onClick={(event) => {
                    event.stopPropagation();
                    setEditing(lib);
                    setEditName(lib.name);
                    setEditColor(lib.color ?? DEFAULT_LIBRARY_COLOR);
                  }}
                >
                  <PenLine className="h-3.5 w-3.5" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7"
                  title={t("library.migrate")}
                  onClick={(event) => {
                    event.stopPropagation();
                    void migrate(lib.id);
                  }}
                >
                  <FolderInput className="h-3.5 w-3.5" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7"
                  title={t("library.merge")}
                  disabled={libraries.filter((l) => l.id !== lib.id).length === 0}
                  onClick={(event) => {
                    event.stopPropagation();
                    setMerging(lib);
                    setMergeTarget(libraries.find((l) => l.id !== lib.id)?.id ?? "");
                  }}
                >
                  <GitMerge className="h-3.5 w-3.5" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7 text-destructive"
                  title={t("library.delete")}
                  disabled={lib.id === current?.id}
                  onClick={(event) => {
                    event.stopPropagation();
                    setConfirmDelete(lib.id);
                  }}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </div>
            ))}
            {libraries.length === 0 && (
              <div className="py-6 text-center text-sm text-muted-foreground">
                {t("library.empty")}
              </div>
            )}
          </div>
        </DialogContent>
      </Dialog>

      <Dialog open={!!editing} onOpenChange={(value) => !value && setEditing(null)}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t("library.rename")}</DialogTitle>
          </DialogHeader>
          <Input
            value={editName}
            autoFocus
            onChange={(event) => setEditName(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void rename();
            }}
          />
          <div className="space-y-1.5">
            <Label>{t("library.colorLabel")}</Label>
            <div className="flex items-center gap-3">
              <LibraryColorIcon color={editColor} size={22} />
              <LibraryColorPicker value={editColor} onChange={setEditColor} />
            </div>
          </div>
          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={() => setEditing(null)}>
              {t("common.cancel")}
            </Button>
            <Button disabled={!editName.trim()} onClick={() => void rename()}>
              {t("common.confirm")}
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      {/* Merge dialog. */}
      <Dialog open={!!merging} onOpenChange={(v) => !v && !mergingBusy && setMerging(null)}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t("library.mergeTitle")}</DialogTitle>
            <DialogDescription>{t("library.mergeDesc")}</DialogDescription>
          </DialogHeader>
          {operation?.operationType === "merge_library" && operation.status !== "completed" && (
            <OperationStatus progress={operation} />
          )}
          <div className="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-end gap-2 text-sm">
            <div className="min-w-0 space-y-1.5">
              <Label htmlFor={`${mergeFieldsId}-source`}>{t("library.mergeSource")}</Label>
              <Input
                id={`${mergeFieldsId}-source`}
                value={merging?.name ?? ""}
                title={merging?.name}
                readOnly
                className="truncate"
              />
            </div>
            <ArrowRight className="mb-2.5 h-4 w-4 text-muted-foreground" aria-hidden="true" />
            <div className="min-w-0 space-y-1.5">
              <Label htmlFor={`${mergeFieldsId}-target`}>{t("library.mergeTarget")}</Label>
              <Select value={mergeTarget} onValueChange={setMergeTarget}>
                <SelectTrigger id={`${mergeFieldsId}-target`} className="w-full min-w-0">
                  <SelectValue placeholder={t("library.target")} />
                </SelectTrigger>
                <SelectContent>
                  {libraries
                    .filter((l) => l.id !== merging?.id)
                    .map((l) => (
                      <SelectItem key={l.id} value={l.id}>
                        {l.name}
                      </SelectItem>
                    ))}
                </SelectContent>
              </Select>
            </div>
          </div>
          <div className="flex justify-end">
            <Button size="sm" disabled={!mergeTarget || mergingBusy} onClick={() => void merge()}>
              {mergingBusy ? t("common.loading") : t("library.confirmMerge")}
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      <AlertDialog open={!!confirmDelete} onOpenChange={(v) => !v && setConfirmDelete(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("library.delete")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("library.deleteWarn")}（{target?.name}）
            </AlertDialogDescription>
          </AlertDialogHeader>
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={deletingFiles}
              onChange={(e) => setDeletingFiles(e.target.checked)}
            />
            {t("library.deleteFiles")}
          </label>
          <AlertDialogFooter>
            <AlertDialogCancel onClick={() => setConfirmDelete(null)}>
              {t("common.cancel")}
            </AlertDialogCancel>
            <AlertDialogAction
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
              onClick={() => {
                if (confirmDelete) void remove(confirmDelete, deletingFiles);
              }}
            >
              {deletingFiles ? t("library.deleteFiles") : t("library.removeOnly")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
