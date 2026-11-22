import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { errorCode, explainError } from "@/lib/errors";
import { useAppStore } from "@/stores/app";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
export default function LibraryUpgradeDialog() {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [choosing, setChoosing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const libraries = useAppStore((s) => s.libraries);
  useEffect(() => {
    const show = () => {
      setOpen(true);
      setChoosing(false);
      setError("");
      void api
        .libraryList()
        .then((libs) => useAppStore.getState().setLibraries(libs))
        .catch(() => {});
    };
    window.addEventListener("library-upgrade-required", show);
    return () => window.removeEventListener("library-upgrade-required", show);
  }, []);
  const choose = async (id: string) => {
    setBusy(true);
    setError("");
    try {
      const library = await api.librarySwitch(id);
      useAppStore.getState().setCurrent(library);
      setOpen(false);
      window.dispatchEvent(new Event("library-upgrade-resolved"));
    } catch (cause) {
      setError(
        errorCode(cause) === "library_upgrade_required"
          ? t("errors.library_upgrade_required")
          : explainError(cause),
      );
      setChoosing(true);
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>{t("indexManagement.upgradeTitle")}</DialogTitle>
            <DialogDescription>{t("errors.library_upgrade_required")}</DialogDescription>
          </DialogHeader>
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" onClick={() => setChoosing(true)}>
              {t("indexManagement.chooseLibrary")}
            </Button>
          </div>

          {choosing && (
            <div className="max-h-48 space-y-2 overflow-auto">
              {libraries.map((library) => (
                <Button
                  key={library.id}
                  variant="outline"
                  className="w-full justify-start"
                  disabled={busy}
                  onClick={() => void choose(library.id)}
                >
                  {library.name}
                </Button>
              ))}
            </div>
          )}
          {error && (
            <p role="alert" className="text-sm text-destructive">
              {error}
            </p>
          )}
        </DialogContent>
      </Dialog>
    </>
  );
}
