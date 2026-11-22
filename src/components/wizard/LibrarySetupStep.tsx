import { useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { homeDir } from "@tauri-apps/api/path";
import { ArrowRight, FileInput, FolderInput, FolderOpen, Loader2, ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import {
  DEFAULT_LIBRARY_COLOR,
  LibraryColorIcon,
  LibraryColorPicker,
} from "@/components/library/LibraryBadge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useAppStore } from "@/stores/app";
import { errorCode, explainError } from "@/lib/errors";
import { libraryPath as buildLibraryPath } from "@/lib/libraryPath";
import { useLibraryNameInput } from "@/hooks/useLibraryNameInput";
import type { LibraryMeta } from "@/types";
export default function LibrarySetupStep({
  onCreated,
  onImported,
}: {
  onCreated: () => Promise<void>;
  onImported: () => Promise<void>;
}) {
  const { t } = useTranslation();
  const { setLibraries, setCurrent } = useAppStore();
  const [customName, setCustomName] = useState<string | null>(null);
  const name = customName ?? t("wizard.defaultLibraryName");
  const [dir, setDir] = useState("");
  const [color, setColor] = useState<string>(DEFAULT_LIBRARY_COLOR);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<{
    message: string;
    code?: string;
  } | null>(null);
  const nameInput = useLibraryNameInput(name, (value) => {
    setCustomName(value);
    setError(null);
  });
  // If the library was created but persisting the next step failed, retry that
  // transition instead of attempting to create the same directory again.
  const created = useRef<LibraryMeta | null>(null);
  const libraryPath = useMemo(() => buildLibraryPath(dir, name), [dir, name]);
  useEffect(() => {
    let disposed = false;
    void homeDir()
      .then((value) => {
        if (!disposed) setDir((current) => current || value);
      })
      .catch(() => {});
    return () => {
      disposed = true;
    };
  }, []);
  const showError = (cause: unknown) =>
    setError({ message: explainError(cause), code: errorCode(cause) });
  const chooseDir = async () => {
    setBusy(true);
    try {
      const picked = await openDialog({ directory: true, title: t("library.pathLabel") });
      if (typeof picked === "string") {
        setDir(picked);
        setError(null);
      }
    } catch (cause) {
      showError(cause);
    } finally {
      setBusy(false);
    }
  };
  const syncLibrary = async (library: LibraryMeta) => {
    setCurrent(library);
    setLibraries(await api.libraryList());
  };
  const submit = async () => {
    if (busy || !nameInput.canSubmit() || !dir) return;
    setError(null);
    setBusy(true);
    try {
      if (!created.current)
        created.current = await api.libraryCreate(name.trim(), libraryPath, color);
      await syncLibrary(created.current);
      await onCreated();
    } catch (cause) {
      showError(cause);
    } finally {
      setBusy(false);
    }
  };
  const importExisting = async (configFile: boolean) => {
    setError(null);
    setBusy(true);
    try {
      const picked = await openDialog(
        configFile
          ? {
              title: t("library.importConfig"),
              filters: [{ name: "ClipSeek Library", extensions: ["json"] }],
            }
          : { directory: true, title: t("library.importDirectory") },
      );
      if (typeof picked !== "string") return;
      await syncLibrary(await api.libraryImport(picked));
      await onImported();
    } catch (cause) {
      showError(cause);
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="w-full max-w-xl space-y-6" aria-labelledby="library-setup-title">
      <div className="text-center">
        <div className="mx-auto mb-4 flex size-12 items-center justify-center rounded-2xl bg-primary/5">
          <FolderOpen className="size-6" />
        </div>
        <h1 id="library-setup-title" className="text-2xl font-semibold tracking-tight">
          {t("wizard.libraryTitle")}
        </h1>
        <p className="mx-auto mt-2 max-w-md text-sm leading-relaxed text-muted-foreground">
          {t("wizard.librarySetupHint")}
        </p>
      </div>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
        className="space-y-5 rounded-2xl border bg-card p-6 shadow-sm"
      >
        <div className="flex items-center gap-3">
          <div className="flex size-11 shrink-0 items-center justify-center rounded-xl bg-muted">
            <LibraryColorIcon color={color} size={24} />
          </div>
          <div className="min-w-0 flex-1 space-y-1.5">
            <Label htmlFor="lib-name">{t("library.nameLabel")}</Label>
            <Input
              id="lib-name"
              value={name}
              disabled={busy || !!created.current}
              placeholder={t("library.namePlaceholder")}
              {...nameInput.inputProps}
              aria-invalid={!!nameInput.error}
              aria-describedby={nameInput.error ? "lib-name-error" : undefined}
            />
          </div>
        </div>
        {nameInput.error && (
          <p id="lib-name-error" role="alert" className="text-sm text-destructive">
            {t(`library.nameValidation.${nameInput.error}`)}
          </p>
        )}
        <div className="space-y-2">
          <div className="flex items-center justify-between gap-3">
            <Label htmlFor="lib-path">{t("library.pathLabel")}</Label>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              disabled={busy || !!created.current}
              onClick={() => void chooseDir()}
            >
              <FolderOpen className="size-3.5" />
              {t("wizard.changeLocation")}
            </Button>
          </div>
          <Input
            id="lib-path"
            value={dir}
            readOnly
            placeholder={t("library.pathPlaceholder")}
            className="text-xs text-muted-foreground"
          />
          <p className="text-xs text-muted-foreground">{t("wizard.newFolderHint")}</p>
          {dir && nameInput.valid && (
            <p className="break-all rounded-lg bg-muted/60 px-3 py-2 font-mono text-xs leading-relaxed text-muted-foreground">
              {libraryPath}
            </p>
          )}
        </div>
        <details className="border-t pt-3">
          <summary className="w-fit cursor-pointer rounded py-1 text-xs text-muted-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring">
            {t("wizard.customizeColor")}
          </summary>
          <div className="mt-3" inert={busy || !!created.current}>
            <LibraryColorPicker value={color} onChange={setColor} />
          </div>
        </details>
        {error && (
          <div
            role="alert"
            className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive"
          >
            <p>{error.message}</p>
            {error.code === "library_dir_not_empty" && (
              <p className="mt-1">{t("library.existingDirectoryHint")}</p>
            )}
          </div>
        )}
        <Button type="submit" className="h-11 w-full" disabled={!nameInput.valid || !dir || busy}>
          {busy && <Loader2 className="size-4 animate-spin" />}
          {created.current
            ? t("wizard.continueSetup")
            : busy
              ? t("library.creating")
              : t("wizard.createAndContinue")}
          {!busy && <ArrowRight className="size-4" />}
        </Button>
      </form>
      <details className="rounded-xl border bg-muted/20 px-4 py-3">
        <summary className="cursor-pointer rounded text-sm text-muted-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring">
          {t("wizard.openExisting")}
        </summary>
        <div className="mt-3 flex flex-wrap gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={busy || !!created.current}
            onClick={() => void importExisting(false)}
          >
            <FolderInput className="size-4" />
            {t("library.importDirectory")}
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={busy || !!created.current}
            onClick={() => void importExisting(true)}
          >
            <FileInput className="size-4" />
            {t("library.importConfig")}
          </Button>
        </div>
      </details>
      <p className="flex items-center justify-center gap-1.5 text-xs text-muted-foreground">
        <ShieldCheck className="size-3.5" />
        {t("wizard.localLibraryHint")}
      </p>
    </section>
  );
}
