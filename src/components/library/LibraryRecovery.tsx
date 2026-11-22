import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen, FolderPlus, RefreshCw } from "lucide-react";
import { api } from "@/lib/api";
import { explainError } from "@/lib/errors";
import { withLibraryAuthorization } from "@/lib/libraryAccess";
import { useAppStore } from "@/stores/app";
import type { LibraryMeta } from "@/types";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import CreateLibraryDialog from "./CreateLibraryDialog";
export default function LibraryRecovery({
  library,
  reason,
  onRecovered,
  onRetry,
}: {
  library: LibraryMeta;
  reason: string;
  onRecovered: (replacement: LibraryMeta) => Promise<void>;
  onRetry: () => void;
}) {
  const { t } = useTranslation();
  const libraries = useAppStore((s) => s.libraries);
  const [choosing, setChoosing] = useState(false);
  const [creating, setCreating] = useState(false);
  const [busy, setBusy] = useState(false);
  const working = useRef(false);
  const [error, setError] = useState("");
  const [opened, setOpened] = useState<LibraryMeta | null>(null);
  const recover = async (operation: () => Promise<LibraryMeta | null>) => {
    if (working.current) return;
    working.current = true;
    setBusy(true);
    setError("");
    try {
      const replacement = await operation();
      if (!replacement) return;
      setOpened(replacement);
      await onRecovered(replacement);
    } catch (cause) {
      setChoosing(false);
      setError(explainError(cause));
    } finally {
      working.current = false;
      setBusy(false);
    }
  };
  const choose = (candidate: LibraryMeta) => {
    setChoosing(false);
    void recover(() =>
      withLibraryAuthorization(
        () => api.librarySwitch(candidate.id),
        [candidate],
        (item) => t("library.reauthorizeTitle", { name: item.name }),
      ),
    );
  };
  const browse = () =>
    void recover(async () => {
      const picked = await openDialog({ directory: true, title: t("libraryRecovery.choose") });
      if (typeof picked !== "string") return null;
      setChoosing(false);
      return api.libraryImport(picked);
    });
  return (
    <section className="flex h-full items-center justify-center overflow-auto p-6">
      <div className="w-full max-w-lg space-y-5">
        <div className="space-y-2">
          <h1 className="text-lg font-semibold">{t("libraryRecovery.title")}</h1>
          <p className="text-sm text-muted-foreground">{t("libraryRecovery.description")}</p>
        </div>
        <div className="space-y-2 rounded-lg border bg-muted/30 p-4">
          <p className="break-words font-medium">{library.name}</p>
          <p className="break-all text-xs text-muted-foreground">{library.path}</p>
          <p role="alert" className="break-words text-sm text-destructive">
            {reason}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button disabled={busy} onClick={() => setChoosing(true)}>
            <FolderOpen />
            {t("libraryRecovery.choose")}
          </Button>
          <Button disabled={busy} variant="outline" onClick={() => setCreating(true)}>
            <FolderPlus />
            {t("libraryRecovery.create")}
          </Button>
          <Button
            disabled={busy}
            variant="ghost"
            onClick={() => (opened ? void recover(() => Promise.resolve(opened)) : onRetry())}
          >
            <RefreshCw />
            {t("common.retry")}
          </Button>
        </div>
        <p className="text-xs text-muted-foreground">{t("libraryRecovery.cleanupHint")}</p>
        {busy && (
          <p role="status" className="text-sm text-muted-foreground">
            {t("common.loading")}
          </p>
        )}
        {error && (
          <p role="alert" className="break-words text-sm text-destructive">
            {error}
          </p>
        )}
      </div>
      <Dialog
        open={choosing}
        onOpenChange={(next) => {
          if (!busy) setChoosing(next);
        }}
      >
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>{t("libraryRecovery.choose")}</DialogTitle>
            <DialogDescription>{t("libraryRecovery.chooseDescription")}</DialogDescription>
          </DialogHeader>
          <div className="max-h-64 space-y-2 overflow-auto">
            {libraries
              .filter((item) => item.id !== library.id)
              .map((candidate) => (
                <Button
                  key={candidate.id}
                  variant="outline"
                  className="h-auto w-full flex-col items-start whitespace-normal py-3 text-left"
                  disabled={busy}
                  onClick={() => choose(candidate)}
                >
                  <span className="break-words">{candidate.name}</span>
                  <span className="break-all text-xs font-normal text-muted-foreground">
                    {candidate.path}
                  </span>
                </Button>
              ))}
          </div>
          <Button variant="outline" disabled={busy} onClick={browse}>
            <FolderOpen />
            {t("libraryRecovery.browse")}
          </Button>
        </DialogContent>
      </Dialog>
      <CreateLibraryDialog
        open={creating}
        onOpenChange={setCreating}
        onCreated={async (replacement) => {
          setCreating(false);
          await recover(() => Promise.resolve(replacement));
        }}
      />
    </section>
  );
}
