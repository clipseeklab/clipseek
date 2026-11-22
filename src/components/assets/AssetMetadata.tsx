import { useTranslation } from "react-i18next";
import { formatDuration, formatFileSize } from "@/lib/utils";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { AssetDetail } from "@/types";
import AssetTechnicalMetadata from "./AssetTechnicalMetadata";
function webSourceDomain(value: string): string {
  try {
    return new URL(value).hostname || value;
  } catch {
    return value;
  }
}
export default function AssetMetadata({
  detail,
  err,
}: {
  detail: AssetDetail;
  err: string | null;
}) {
  const { t, i18n } = useTranslation();
  return (
    <div className="select-text space-y-1.5 border-t pt-4 text-sm">
      <div className="font-medium">{t("assets.meta")}</div>
      <div className="grid min-w-0 grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 text-xs">
        <span className="text-muted-foreground">{t("assets.assetId")}</span>
        <span className="font-mono tabular-nums">{detail.id}</span>
        <span className="text-muted-foreground">{t("assetMetadata.fileType")}</span>
        <span className="min-w-0 break-all">{detail.mimeType}</span>
        <span className="text-muted-foreground">{t("assets.resolution")}</span>
        <span className="tabular-nums">
          {detail.width ?? "?"} × {detail.height ?? "?"}
        </span>
        <span className="text-muted-foreground">{t("assets.size")}</span>
        <span>{formatFileSize(detail.fileSize)}</span>
        {detail.durationMs != null && (
          <>
            <span className="text-muted-foreground">{t("assets.duration")}</span>
            <span>{formatDuration(detail.durationMs)}</span>
          </>
        )}
        <span className="text-muted-foreground">{t("assets.importedAt")}</span>
        <span className="min-w-0">
          {new Date(detail.importedAt * 1000).toLocaleString(
            i18n.resolvedLanguage === "zh" ? "zh-CN" : "en-US",
          )}
        </span>
        <span className="text-muted-foreground">{t("assets.filePath")}</span>
        <span className="min-w-0 break-all [overflow-wrap:anywhere]">{detail.absPath}</span>
        <span className="text-muted-foreground">{t("assets.fileName")}</span>
        <span className="min-w-0 break-all [overflow-wrap:anywhere]">{detail.fileName}</span>
        {detail.webSourceUrl && (
          <>
            <span className="text-muted-foreground">{t("assets.webSource")}</span>
            <button
              type="button"
              className="min-w-0 truncate text-left text-primary hover:underline"
              title={detail.webSourceUrl}
              onClick={() => void openUrl(detail.webSourceUrl!)}
            >
              {webSourceDomain(detail.webSourceUrl)}
            </button>
          </>
        )}
      </div>
      <AssetTechnicalMetadata detail={detail} />
      {err && <div className="text-xs text-destructive">⚠ {err}</div>}
    </div>
  );
}
