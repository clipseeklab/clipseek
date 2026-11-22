import { queryKeys } from "@/lib/queryKeys";
import { useState } from "react";
import { ScanSearch } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import { explainError } from "@/lib/errors";
import { Switch } from "@/components/ui/switch";
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
import { toast } from "@/components/Toaster";
import IndexBuildButton from "./IndexBuildButton";
import SearchIndexSection from "@/components/settings/SearchIndexSection";
import type { Settings } from "@/types";
export default function IndexManagementSection() {
  const { t } = useTranslation();
  const { settings, setSettings, current } = useAppStore();
  const summary = useQuery({
    queryKey: queryKeys.indexSummary.library(current?.id),
    queryFn: () => api.indexSummary(),
    enabled: !!current,
    refetchInterval: (query) =>
      (query.state.data?.tasks.queued ?? 0) + (query.state.data?.tasks.running ?? 0) > 0
        ? 1500
        : false,
  });
  const [saving, setSaving] = useState(false);
  const [pendingDisable, setPendingDisable] = useState<"autoVisualIndex" | "autoSubtitleIndex">(
    "autoVisualIndex",
  );
  const [disableOpen, setDisableOpen] = useState(false);
  const update = async (patch: Partial<Settings>) => {
    if (saving) return false;
    setSaving(true);
    try {
      setSettings(await api.settingsSet(patch));
      return true;
    } catch (error) {
      toast.error(explainError(error));
      return false;
    } finally {
      setSaving(false);
    }
  };
  const toggle = (key: "autoVisualIndex" | "autoSubtitleIndex", enabled: boolean) => {
    if (!enabled) {
      setPendingDisable(key);
      setDisableOpen(true);
    } else void update({ [key]: true });
  };
  const confirmDisable = async () => {
    if (await update({ [pendingDisable]: false })) setDisableOpen(false);
  };
  return (
    <section
      id="settings-indexing"
      className="scroll-mt-6 space-y-5 rounded-lg border bg-muted/20 p-4"
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="flex shrink-0 items-center gap-2 font-medium">
          <ScanSearch className="size-4 text-muted-foreground" />
          {t("indexManagement.title")}
        </h2>
        <div className="ml-auto flex min-w-0 max-w-full items-center justify-end gap-2 text-xs text-muted-foreground">
          <span className="max-w-56 truncate" title={current?.name}>
            {current?.name ?? t("settings.searchIndexNeedsLibrary")}
          </span>
          {summary.data && (
            <>
              <span aria-hidden="true">·</span>
              <span className="shrink-0">
                {t("indexManagement.assetCount", { count: summary.data.assets })}
              </span>
            </>
          )}
        </div>
      </div>
      <section aria-labelledby="visual-index-heading" className="space-y-3 border-t pt-4">
        <div className="flex items-center justify-between gap-6">
          <h3 id="visual-index-heading" className="text-sm font-medium">
            <label htmlFor="auto-visual-index">{t("indexManagement.visual")}</label>
          </h3>
          <Switch
            aria-label={t("indexManagement.autoVisual")}
            disabled={saving}
            id="auto-visual-index"
            checked={settings.autoVisualIndex !== false}
            onCheckedChange={(v) => toggle("autoVisualIndex", v)}
          />
        </div>
        <p className="text-xs leading-relaxed text-muted-foreground">
          {t("indexManagement.visualHint")}
        </p>
        <div className="flex items-center justify-between gap-3">
          <span className="text-xs text-muted-foreground">
            {summary.data && t("indexManagement.indexedCount", { count: summary.data.visual })}
          </span>
          <IndexBuildButton scope={{ all: true }} visualOnly disabled={!current} />
        </div>
        <details className="text-xs text-muted-foreground">
          <summary className="w-fit cursor-pointer rounded-sm py-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
            {t("indexManagement.repair")}
          </summary>
          <SearchIndexSection libraryId={current?.id} libraryName={current?.name} />
        </details>
      </section>

      <p className="text-xs leading-relaxed text-muted-foreground">
        {t("indexManagement.settingsHint")}
      </p>
      <AlertDialog
        open={disableOpen}
        onOpenChange={(open) => {
          if (!saving) setDisableOpen(open);
        }}
      >
        <AlertDialogContent
          onCloseAutoFocus={(event) => {
            event.preventDefault();
            document
              .getElementById(
                pendingDisable === "autoVisualIndex" ? "auto-visual-index" : "auto-subtitle-index",
              )
              ?.focus();
          }}
        >
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t(
                pendingDisable === "autoVisualIndex"
                  ? "indexManagement.disableVisualTitle"
                  : "indexManagement.disableSubtitleTitle",
              )}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t(
                pendingDisable === "autoVisualIndex"
                  ? "indexManagement.disableVisualHint"
                  : "indexManagement.disableSubtitleHint",
              )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={saving}>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              disabled={saving}
              onClick={(event) => {
                event.preventDefault();
                void confirmDisable();
              }}
            >
              {t("indexManagement.confirmDisable")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </section>
  );
}
