import { useState } from "react";
import { create } from "zustand";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { AlertTriangle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "@/components/Toaster";
import { Checkbox } from "@/components/ui/checkbox";
import { explainError } from "@/lib/errors";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogMedia,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
const SKIP_WARNING_KEY = "skip-managed-asset-finder-warning";
interface RevealStore {
  path: string | null;
  setPath: (path: string | null) => void;
}
const useRevealStore = create<RevealStore>((set) => ({
  path: null,
  setPath: (path) => set({ path }),
}));
/** Reveal source files directly. Legacy managed files require a warning because external edits
 * can damage library integrity.
 */
export function revealManagedAsset(path: string, direct = false) {
  if (direct || localStorage.getItem(SKIP_WARNING_KEY) === "1") {
    revealItemInDir(path).catch((error) => toast.error(explainError(error)));
    return;
  }
  useRevealStore.getState().setPath(path);
}
export function ManagedAssetRevealDialog() {
  const { t } = useTranslation();
  const { path, setPath } = useRevealStore();
  const [skipWarning, setSkipWarning] = useState(false);
  const close = () => {
    setPath(null);
    setSkipWarning(false);
  };
  const confirm = () => {
    if (!path) return;
    if (skipWarning) localStorage.setItem(SKIP_WARNING_KEY, "1");
    revealItemInDir(path).catch((error) => toast.error(explainError(error)));
    close();
  };
  return (
    <AlertDialog
      open={path != null}
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogMedia className="rounded-full bg-amber-500/15 text-amber-600 dark:text-amber-400">
            <AlertTriangle className="size-8" />
          </AlertDialogMedia>
          <AlertDialogTitle>{t("finderWarning.title")}</AlertDialogTitle>
          <AlertDialogDescription className="leading-relaxed">
            {t("finderWarning.description")}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <label className="flex cursor-pointer items-center gap-2 text-sm">
          <Checkbox
            checked={skipWarning}
            onCheckedChange={(checked) => setSkipWarning(checked === true)}
          />
          {t("finderWarning.doNotAskAgain")}
        </label>
        <AlertDialogFooter>
          <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
          <AlertDialogAction onClick={confirm}>{t("finderWarning.open")}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
