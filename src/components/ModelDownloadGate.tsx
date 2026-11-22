import { useEffect, useEffectEvent, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { CircleAlert, Download, Loader2, ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { explainError } from "@/lib/errors";
import type { ModelProgress } from "@/types";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import FeatureShowcase from "@/components/wizard/FeatureShowcase";
function formatBytes(value: number) {
  if (value >= 1024 ** 3) return `${(value / 1024 ** 3).toFixed(1)} GB`;
  return `${Math.round(value / 1024 ** 2)} MB`;
}
function formatSpeed(bps: number) {
  if (bps >= 1024 ** 2) return `${(bps / 1024 ** 2).toFixed(1)} MB/s`;
  if (bps >= 1024) return `${Math.round(bps / 1024)} KB/s`;
  return "";
}
function formatEta(seconds: number) {
  const m = Math.floor(seconds / 60);
  const s = Math.round(seconds % 60);
  if (m >= 60) {
    const h = Math.floor(m / 60);
    return `${h}:${String(m % 60).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }
  return `${m}:${String(s).padStart(2, "0")}`;
}
const FILE_LABELS: Record<string, string> = {
  "vocab.txt": "modelDownload.fileVocab",
  "model_config.json": "modelDownload.fileConfig",
  "text.onnx": "modelDownload.fileText",
  "visual.clean.b8.onnx": "modelDownload.fileVisual",
  "visual.clean.onnx": "modelDownload.fileVisual",
};
export default function ModelDownloadGate({
  onReady,
  embedded = false,
  showcase = false,
}: {
  onReady: () => void;
  embedded?: boolean;
  showcase?: boolean;
}) {
  const { t } = useTranslation();
  const alive = useRef(false);
  const readyCallback = useRef(onReady);
  useEffect(() => {
    readyCallback.current = onReady;
  }, [onReady]);
  const observation = useRef({
    startedAt: 0,
    lastGrowthAt: 0,
    bytes: 0,
    samples: [] as {
      time: number;
      bytes: number;
    }[],
  });
  const [elapsed, setElapsed] = useState(0);
  const [warning, setWarning] = useState<"stalled" | "slow" | null>(null);
  const [progress, setProgress] = useState<ModelProgress | null>(null);
  const [error, setError] = useState("");
  const [downloading, setDownloading] = useState(false);
  const start = async () => {
    const now = Date.now();
    observation.current = { startedAt: now, lastGrowthAt: now, bytes: 0, samples: [] };
    setElapsed(0);
    setWarning(null);
    setProgress(null);
    setDownloading(true);
    setError("");
    try {
      const status = await api.modelDownload();
      if (!alive.current) return;
      if (status.ready) readyCallback.current();
      else setError(t("modelDownload.notReady"));
    } catch (cause) {
      if (alive.current) setError(explainError(cause));
    } finally {
      if (alive.current) setDownloading(false);
    }
  };
  const startDownload = useEffectEvent(start);
  useEffect(() => {
    alive.current = true;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    // Subscribe before starting so a fast download cannot outrun the listener.
    void (async () => {
      try {
        const cleanup = await listen<ModelProgress>("model://progress", ({ payload }) => {
          if (disposed) return;
          const state = observation.current;
          // Count growth in bytes, not heartbeat events or an old smoothed speed.
          if (payload.downloadedBytes !== state.bytes) {
            state.lastGrowthAt = Date.now();
            setWarning((current) => (current === "stalled" ? null : current));
          }
          state.bytes = payload.downloadedBytes;
          setProgress(payload);
        });
        if (disposed) {
          cleanup();
          return;
        }
        unlisten = cleanup;
        void startDownload();
      } catch (cause) {
        if (!disposed) setError(explainError(cause));
      }
    })();
    return () => {
      disposed = true;
      alive.current = false;
      unlisten?.();
    };
    // The download owns its lifetime; parent callback changes must not restart it.
  }, []);
  const totalBytes = progress?.totalBytes ?? 0;
  useEffect(() => {
    if (!downloading) return;
    const timer = window.setInterval(() => {
      const now = Date.now();
      const state = observation.current;
      setElapsed(Math.floor((now - state.startedAt) / 1000));
      state.samples.push({ time: now, bytes: state.bytes });
      while (state.samples.length > 1 && state.samples[1].time <= now - 60000)
        state.samples.shift();
      const first = state.samples[0];
      const duration = now - first.time;
      const complete = totalBytes > 0 && state.bytes >= totalBytes;
      if (complete) {
        setWarning(null);
        return;
      }
      if (now - state.lastGrowthAt >= 30000) setWarning("stalled");
      else if (duration >= 60000 && (state.bytes - first.bytes) / (duration / 1000) < 128 * 1024)
        setWarning("slow");
      else setWarning(null);
    }, 1000);
    return () => window.clearInterval(timer);
  }, [downloading, totalBytes]);
  const percent = progress?.totalBytes ? (progress.downloadedBytes / progress.totalBytes) * 100 : 0;
  const verifying =
    progress !== null && progress.totalBytes > 0 && progress.downloadedBytes >= progress.totalBytes;
  const fileLabel = progress?.currentFile
    ? t(FILE_LABELS[progress.currentFile] ?? "modelDownload.connecting")
    : t("modelDownload.connecting");
  const speedText = [
    formatSpeed(progress?.speedBps ?? 0),
    progress?.etaSeconds ? t("modelDownload.eta", { time: formatEta(progress.etaSeconds) }) : "",
  ]
    .filter(Boolean)
    .join(" · ");
  return (
    <div
      className={
        embedded
          ? "w-full"
          : "flex h-full items-center justify-center overflow-y-auto bg-background p-8"
      }
    >
      <div
        className={
          showcase
            ? "mx-auto grid w-full max-w-5xl items-center gap-8 md:grid-cols-[1.2fr_1fr] lg:gap-12"
            : "mx-auto w-full max-w-md"
        }
      >
        {showcase && (
          <div className="rounded-2xl border bg-muted/30 p-5 sm:p-7">
            <FeatureShowcase />
          </div>
        )}
        <div className="mx-auto w-full max-w-md space-y-6 text-center">
          <div className="mx-auto flex size-14 items-center justify-center rounded-2xl bg-primary text-primary-foreground">
            {downloading ? (
              <Loader2 className="size-7 animate-spin" />
            ) : (
              <Download className="size-7" />
            )}
          </div>
          <div className="space-y-2">
            <h1 className="text-xl font-semibold">{t("modelDownload.title")}</h1>
            <p className="text-sm text-muted-foreground">{t("modelDownload.description")}</p>
          </div>
          <div className="space-y-2 text-left">
            <Progress
              value={Math.min(100, percent)}
              aria-label={t("modelDownload.title")}
              aria-valuenow={Math.floor(Math.min(100, percent))}
              className="h-2"
            />
            <div className="flex justify-between text-xs text-muted-foreground">
              <span>{verifying ? t("modelDownload.verifying") : fileLabel}</span>
              <span>{progress ? `${Math.floor(percent)}%` : ""}</span>
            </div>
            <div className="flex justify-between text-xs text-muted-foreground">
              <span>{speedText}</span>
              <span>
                {progress
                  ? `${formatBytes(progress.downloadedBytes)} / ${formatBytes(progress.totalBytes)}`
                  : ""}
              </span>
            </div>
            <p className="text-xs text-muted-foreground">
              {t("modelDownload.elapsed", { time: formatEta(elapsed) })}
            </p>
          </div>
          {warning && downloading && !error && (
            <div
              role="status"
              className="flex gap-2 rounded-lg border border-amber-500/30 bg-amber-500/10 p-3 text-left text-sm"
            >
              <CircleAlert className="mt-0.5 size-4 shrink-0 text-amber-600 dark:text-amber-400" />
              <div>
                <p className="font-medium">{t(`modelDownload.${warning}Title`)}</p>
                <p className="mt-1 text-muted-foreground">{t("modelDownload.networkAdvice")}</p>
              </div>
            </div>
          )}
          {error && (
            <div role="alert" className="rounded-lg bg-destructive/10 p-3 text-left text-sm">
              <p className="text-destructive">{error}</p>
              <p className="mt-2 text-muted-foreground">{t("modelDownload.errorAdvice")}</p>
            </div>
          )}
          {error && (
            <Button onClick={() => void start()} disabled={downloading}>
              {t("common.retry")}
            </Button>
          )}
          <p className="text-xs leading-relaxed text-muted-foreground">
            {t("modelDownload.keepOpen")}
          </p>
          {showcase && (
            <p className="flex items-center justify-center gap-1.5 text-xs text-muted-foreground">
              <ShieldCheck className="size-3.5" />
              {t("modelDownload.showcase.local")}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
