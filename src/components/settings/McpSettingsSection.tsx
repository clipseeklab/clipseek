import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Bot, Copy, RefreshCw } from "lucide-react";
import { api } from "@/lib/api";
import { explainError } from "@/lib/errors";
import type { McpStatus } from "@/types";
import { useAppStore } from "@/stores/app";
import { toast } from "@/components/Toaster";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { SettingsPatch } from "@/hooks/useSettingsUpdate";

type McpPatch = Pick<SettingsPatch, "mcpEnabled" | "mcpWriteEnabled" | "mcpListenMode">;
export default function McpSettingsSection({
  open,
  update,
}: {
  open: boolean;
  update: (patch: SettingsPatch) => Promise<void>;
}) {
  const { t } = useTranslation();
  const settings = useAppStore((state) => state.settings);
  const [mcpStatus, setMcpStatus] = useState<McpStatus | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const revision = useRef(0);
  const reload = useCallback(async () => {
    const request = ++revision.current;
    setStatusError(null);
    try {
      const status = await api.mcpStatus();
      if (request === revision.current) setMcpStatus(status);
    } catch (error) {
      if (request === revision.current) {
        setMcpStatus(null);
        setStatusError(explainError(error));
      }
    }
  }, []);
  useEffect(() => {
    if (!open) return;
    void reload();
    return () => {
      revision.current += 1;
    };
  }, [open, reload]);
  const change = async (patch: McpPatch) => {
    setBusy(true);
    try {
      await update(patch);
      await reload();
    } finally {
      setBusy(false);
    }
  };
  const copyToken = async (rotate = false) => {
    const token = rotate ? await api.mcpTokenRotate() : await api.mcpTokenReveal();
    await navigator.clipboard.writeText(token);
    toast.success(t(rotate ? "settings.mcpTokenRotated" : "settings.mcpTokenCopied"));
  };
  return (
    <section
      id="settings-agent"
      className="scroll-mt-6 space-y-2 rounded-lg border bg-muted/20 p-3"
    >
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2 font-medium">
          <Bot className="size-4 text-muted-foreground" />
          {t("settings.mcpTitle")}
        </div>
        <span
          className={
            mcpStatus?.running
              ? "rounded-full bg-emerald-500/10 px-2 py-0.5 text-xs font-medium text-emerald-600"
              : "rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground"
          }
        >
          {t(
            statusError
              ? "settings.mcpUnavailable"
              : !mcpStatus
                ? "common.loading"
                : mcpStatus.running
                  ? "settings.mcpRunning"
                  : "settings.mcpStopped",
          )}
        </span>
      </div>
      <p className="text-xs leading-relaxed text-muted-foreground">{t("settings.mcpHint")}</p>
      <div className="divide-y rounded-md border bg-background">
        <div className="grid min-h-10 grid-cols-[minmax(0,1fr)_auto] items-center justify-end gap-3 px-3 py-1.5">
          <Label>{t("settings.mcpEnable")}</Label>
          <Switch
            aria-label={t("settings.mcpEnable")}
            disabled={busy}
            checked={settings.mcpEnabled ?? false}
            onCheckedChange={(checked) =>
              void change({ mcpEnabled: checked }).catch((error) =>
                toast.error(explainError(error)),
              )
            }
          />
        </div>
        <div className="flex min-h-10 flex-wrap items-center justify-between gap-x-3 gap-y-2 px-3 py-1.5">
          <Label htmlFor="mcp-listen-mode">{t("settings.mcpListenMode")}</Label>
          <Select
            disabled={busy}
            value={settings.mcpListenMode ?? "loopback"}
            onValueChange={(value: "loopback" | "lan") =>
              void change({ mcpListenMode: value }).catch((error) =>
                toast.error(explainError(error)),
              )
            }
          >
            <SelectTrigger
              id="mcp-listen-mode"
              size="sm"
              className="w-auto max-w-full data-[size=sm]:h-auto [&>[data-slot=select-value]]:line-clamp-none [&>[data-slot=select-value]]:whitespace-normal"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="loopback">{t("settings.mcpLoopback")}</SelectItem>
              <SelectItem value="lan">{t("settings.mcpLan")}</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div className="grid min-h-11 grid-cols-[minmax(0,1fr)_auto] items-center justify-end gap-3 px-3 py-1.5">
          <div className="min-w-0">
            <Label>{t("settings.mcpWrite")}</Label>
            <p className="truncate text-xs text-muted-foreground">{t("settings.mcpWriteHint")}</p>
          </div>
          <Switch
            className="shrink-0"
            disabled={busy || !settings.mcpEnabled}
            aria-label={t("settings.mcpWrite")}
            checked={settings.mcpWriteEnabled ?? false}
            onCheckedChange={(checked) =>
              void change({ mcpWriteEnabled: checked }).catch((error) =>
                toast.error(explainError(error)),
              )
            }
          />
        </div>
      </div>
      {settings.mcpListenMode === "lan" && (
        <p className="text-xs text-amber-600">{t("settings.mcpLanWarning")}</p>
      )}
      {settings.mcpEnabled && (
        <>
          <div className="divide-y rounded-md border bg-background text-xs">
            {mcpStatus?.localUrl ? (
              <div className="grid grid-cols-[4rem_minmax(0,1fr)] items-center px-3 py-2">
                <span className="text-muted-foreground">{t("settings.mcpLocalUrl")}</span>
                <code className="truncate">{mcpStatus.localUrl}</code>
              </div>
            ) : (
              <div className="grid grid-cols-[4rem_minmax(0,1fr)] items-center px-3 py-2">
                <span className="text-muted-foreground">{t("settings.mcpEndpoint")}</span>
                <code className="truncate">{mcpStatus?.url ?? t("settings.mcpUnavailable")}</code>
              </div>
            )}
            {mcpStatus?.lanUrl && (
              <div className="grid grid-cols-[4rem_minmax(0,1fr)] items-center px-3 py-2">
                <span className="text-muted-foreground">LAN</span>
                <code className="truncate">{mcpStatus.lanUrl}</code>
              </div>
            )}
          </div>
          <div className="grid grid-cols-2 gap-2">
            <Button
              className="h-8"
              size="sm"
              variant="outline"
              onClick={() =>
                void copyToken(false).catch((error) => toast.error(explainError(error)))
              }
            >
              <Copy className="mr-1 size-3.5" />
              {t("settings.mcpCopyToken")}
            </Button>
            <Button
              className="h-8"
              size="sm"
              variant="outline"
              onClick={() =>
                void copyToken(true).catch((error) => toast.error(explainError(error)))
              }
            >
              <RefreshCw className="mr-1 size-3.5" />
              {t("settings.mcpRotateToken")}
            </Button>
          </div>
          {mcpStatus?.error && <p className="text-xs text-destructive">{mcpStatus.error}</p>}
        </>
      )}
      {statusError && (
        <div role="alert" className="space-y-2 text-xs text-destructive">
          <p>{statusError}</p>
          <Button size="sm" variant="outline" onClick={() => void reload()}>
            {t("common.retry")}
          </Button>
        </div>
      )}
    </section>
  );
}
