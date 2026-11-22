/** Settings cover general preferences, storage, models, MCP, indexes, and help. */
import { useEffect, useState } from "react";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { save } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import { Button } from "@/components/ui/button";
import { toast } from "@/components/Toaster";
import IndexManagementSection from "@/components/indexing/IndexManagementSection";
import {
  BookOpen,
  FileArchive,
  FolderOpen,
  HardDrive,
  MessageSquare,
  Settings2,
} from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { useNavigate } from "react-router-dom";
import { explainError } from "@/lib/errors";
import { useSettingsUpdate } from "@/hooks/useSettingsUpdate";
interface Props {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  initialSection?: "settings-storage" | "settings-indexing";
}
import McpSettingsSection from "./McpSettingsSection";
import EmbeddingSettingsSection from "./EmbeddingSettingsSection";
export default function SettingsDialog({ open, onOpenChange, initialSection }: Props) {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const { settings, current: currentLibrary } = useAppStore();
  const [exporting, setExporting] = useState(false);
  const [trashRetentionDays, setTrashRetentionDays] = useState(settings.trashRetentionDays);
  const [sourceCheck, setSourceCheck] = useState<{
    done: number;
    total: number;
  } | null>(null);
  const [sourceResult, setSourceResult] = useState<{
    total: number;
    missing: number;
    cancelled: boolean;
  } | null>(null);
  useEffect(() => {
    if (open) setTrashRetentionDays(settings.trashRetentionDays);
  }, [open, settings.trashRetentionDays]);
  useEffect(() => {
    if (!open) return;
    let disposed = false;
    let stop: (() => void) | undefined;
    void listen<{
      done: number;
      total: number;
    }>("source://check", ({ payload }) => {
      if (!disposed) setSourceCheck({ done: payload.done, total: payload.total });
    })
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      })
      .catch((error) => {
        if (!disposed) toast.error(explainError(error));
      });
    return () => {
      disposed = true;
      stop?.();
    };
  }, [open]);
  const update = useSettingsUpdate();
  const showError = (error: unknown) => toast.error(explainError(error));
  const openPage = (url: string) => {
    void openUrl(url).catch(showError);
  };
  const exportDiagnostics = async () => {
    setExporting(true);
    try {
      const targetPath = await save({
        defaultPath: `clip-studio-logs-${new Date().toISOString().slice(0, 10)}.zip`,
        filters: [{ name: "ZIP", extensions: ["zip"] }],
      });
      if (!targetPath) return;
      const path = await api.diagnosticsExport(targetPath);
      toast.success(t("settings.diagnosticsExported"));
      await revealItemInDir(path);
    } catch (error) {
      toast.error(t("common.operationFailed", { message: explainError(error) }));
    } finally {
      setExporting(false);
    }
  };
  const runSourceCheck = async () => {
    setSourceResult(null);
    setSourceCheck({ done: 0, total: 0 });
    try {
      const result = await api.sourceCheck(currentLibrary!.id);
      setSourceResult(result);
    } catch (error) {
      toast.error(t("common.operationFailed", { message: explainError(error) }));
    } finally {
      setSourceCheck(null);
    }
  };
  const openLogsDirectory = async () => {
    try {
      await api.diagnosticsOpenLogDir();
    } catch (error) {
      toast.error(t("common.operationFailed", { message: explainError(error) }));
    }
  };
  const jumpToSection = (id: string) => {
    document.getElementById(id)?.scrollIntoView({ block: "start" });
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        onOpenAutoFocus={(event) => {
          if (!initialSection) return;
          event.preventDefault();
          document.getElementById(initialSection)?.focus({ preventScroll: true });
          jumpToSection(initialSection);
        }}
        disableAnimation
        className="max-h-[85vh] grid-rows-[auto_minmax(0,1fr)] gap-0 overflow-hidden p-0 sm:max-w-4xl"
      >
        <DialogHeader className="relative z-10 border-b bg-background px-6 py-5 pr-14">
          <DialogTitle>{t("settings.title")}</DialogTitle>
        </DialogHeader>

        <div className="flex min-h-0">
          <nav className="w-44 shrink-0 border-r bg-muted/10 p-3" aria-label={t("settings.title")}>
            <div className="sticky top-0 space-y-1">
              {[
                ["settings-general", t("settings.general")],
                ["settings-storage", t("settings.storage")],
                ["settings-model", t("settings.modelManagement")],
                ["settings-agent", t("settings.mcpTitle")],
                ["settings-indexing", t("indexManagement.title")],
                ["settings-support", t("settings.support")],
              ].map(([id, label]) => (
                <button
                  key={id}
                  type="button"
                  className="w-full whitespace-nowrap rounded-md px-3 py-2 text-left text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                  onClick={() => jumpToSection(id)}
                >
                  {label}
                </button>
              ))}
            </div>
          </nav>

          <div className="min-w-0 flex-1 space-y-4 overflow-y-auto p-6">
            <section
              id="settings-general"
              className="scroll-mt-6 space-y-4 rounded-lg border bg-muted/20 p-4"
            >
              <div className="flex items-center gap-2 font-medium">
                <Settings2 className="size-4 text-muted-foreground" />
                {t("settings.general")}
              </div>
              <div className="grid gap-4 sm:grid-cols-2">
                <div className="space-y-1.5">
                  <Label>{t("settings.language")}</Label>
                  <Select
                    value={settings.language}
                    onValueChange={(v) => void update({ language: v }).catch(showError)}
                  >
                    <SelectTrigger className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="auto">{t("settings.autoLang")}</SelectItem>
                      <SelectItem value="zh">中文</SelectItem>
                      <SelectItem value="en">English</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div className="space-y-1.5">
                  <Label>{t("settings.theme")}</Label>
                  <Select
                    value={settings.theme}
                    onValueChange={(v) => void update({ theme: v }).catch(showError)}
                  >
                    <SelectTrigger className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="auto">{t("settings.autoTheme")}</SelectItem>
                      <SelectItem value="light">{t("settings.light")}</SelectItem>
                      <SelectItem value="dark">{t("settings.dark")}</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>
              <div className="space-y-2.5 border-t pt-4">
                <div className="flex items-center justify-between">
                  <Label>{t("settings.trashRetention")}</Label>
                  <span className="text-sm tabular-nums text-muted-foreground">
                    {t("settings.days", { count: trashRetentionDays })}
                  </span>
                </div>
                <Slider
                  value={[trashRetentionDays]}
                  min={1}
                  max={90}
                  step={1}
                  onValueChange={(value) => setTrashRetentionDays(value[0])}
                  onValueCommit={(value) => {
                    void update({ trashRetentionDays: value[0] }).catch((error) => {
                      setTrashRetentionDays(settings.trashRetentionDays);
                      toast.error(t("common.operationFailed", { message: explainError(error) }));
                    });
                  }}
                />
              </div>
            </section>

            <section
              id="settings-storage"
              tabIndex={-1}
              className="scroll-mt-6 space-y-3 rounded-lg border bg-muted/20 p-4"
            >
              <div className="flex items-center gap-2 font-medium">
                <HardDrive className="size-4 text-muted-foreground" />
                {t("settings.storage")}
              </div>
              <div className="space-y-2">
                <div className="flex items-center justify-between gap-3">
                  <div className="min-w-0">
                    <Label>{t("settings.sourceCheck")}</Label>
                    <p className="text-xs text-muted-foreground">
                      {currentLibrary
                        ? t("settings.sourceCheckDesc", { library: currentLibrary.name })
                        : t("settings.sourceCheckNeedsLibrary")}
                    </p>
                  </div>
                  {sourceCheck ? (
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => void api.sourceCheckCancel().catch(showError)}
                    >
                      {t("settings.sourceCheckCancel")}
                    </Button>
                  ) : (
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={!currentLibrary}
                      title={currentLibrary ? undefined : t("settings.sourceCheckNeedsLibrary")}
                      onClick={() => void runSourceCheck()}
                    >
                      {t("settings.sourceCheckRun")}
                    </Button>
                  )}
                </div>
                {sourceCheck && (
                  <p className="text-xs tabular-nums text-muted-foreground">
                    {t("settings.sourceCheckProgress", {
                      done: sourceCheck.done,
                      total: sourceCheck.total,
                    })}
                  </p>
                )}
                {sourceResult && !sourceCheck && (
                  <p className="text-xs">
                    {sourceResult.missing > 0 ? (
                      <>
                        <span className="text-amber-600">
                          {t("settings.sourceCheckMissing", {
                            missing: sourceResult.missing,
                            total: sourceResult.total,
                          })}
                        </span>{" "}
                        <button
                          className="underline underline-offset-2"
                          onClick={() => {
                            onOpenChange(false);
                            void Promise.resolve(navigate("/assets?missing=1")).catch(showError);
                          }}
                        >
                          {t("settings.sourceCheckView")}
                        </button>
                      </>
                    ) : (
                      <span className="text-emerald-600">
                        {t("settings.sourceCheckOk", { total: sourceResult.total })}
                      </span>
                    )}
                  </p>
                )}
              </div>
            </section>

            <EmbeddingSettingsSection />
            <McpSettingsSection open={open} update={update} />
            <IndexManagementSection />

            <section
              id="settings-support"
              className="scroll-mt-6 space-y-3 rounded-lg border bg-muted/20 p-4"
            >
              <div className="flex items-center gap-2 font-medium">
                <FileArchive className="size-4 text-muted-foreground" />
                {t("settings.support")}
              </div>
              <p className="text-sm leading-relaxed text-muted-foreground">
                {t("settings.diagnosticsHint")}
              </p>
              <div className="flex items-center gap-2 rounded-md bg-background px-3 py-2 text-sm">
                <BookOpen className="size-4 text-muted-foreground" />
                <span className="text-muted-foreground">{t("settings.userGuide")}</span>
                <button
                  className="font-medium hover:underline"
                  onClick={() =>
                    openPage(
                      i18n.language.startsWith("zh")
                        ? "https://github.com/clipseeklab/clipseek/blob/main/README.zh-CN.md"
                        : "https://github.com/clipseeklab/clipseek/blob/main/README.md",
                    )
                  }
                >
                  GitHub README
                </button>
              </div>
              <div className="flex items-center gap-2 rounded-md bg-background px-3 py-2 text-sm">
                <MessageSquare className="size-4 text-muted-foreground" />
                <span className="text-muted-foreground">{t("settings.feedbackIssues")}</span>
                <button
                  className="font-medium hover:underline"
                  onClick={() => openPage("https://github.com/clipseeklab/clipseek/issues")}
                >
                  GitHub Issues
                </button>
              </div>
              <div className="grid grid-cols-2 gap-2">
                <Button size="sm" variant="outline" onClick={() => void openLogsDirectory()}>
                  <FolderOpen className="mr-1 size-3.5" />
                  {t("settings.openLogsDirectory")}
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={exporting}
                  onClick={() => void exportDiagnostics()}
                >
                  {exporting ? t("settings.exportingLogs") : t("settings.exportLogs")}
                </Button>
              </div>
            </section>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
