/** Localize AppError by code, falling back to English diagnostics for unknown codes. */
import i18n from "@/i18n";
import type { AppErrorShape } from "@/types";
/** Serialized backend error, including optional structured byte-count parameters. */
export interface AppErrorDetail extends AppErrorShape {
  usedBytes?: number;
  incomingBytes?: number;
  limitBytes?: number;
  remainingBytes?: number;
  availableBytes?: number;
  requiredBytes?: number;
}
export function errorCode(error: unknown): string | undefined {
  return error && typeof error === "object" ? (error as AppErrorDetail).code : undefined;
}
export function formatErrorBytes(bytes?: number) {
  if (!bytes) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** index).toFixed(index > 2 ? 1 : 0)} ${units[index]}`;
}
/** Interpolation parameters for structured capacity and disk-space errors. */
function errorParams(detail: AppErrorDetail) {
  const firstImport = detail.code === "first_import_size_exceeded";
  const lowMemory = detail.code === "ai_subtitle_memory_insufficient";
  const gib = (bytes = 0) => `${(bytes / 2 ** 30).toFixed(2)} GiB`;
  const decimalMB = (bytes = 0) => `${Math.ceil(bytes / 10000) / 100} MB`;
  return {
    message: detail.message ?? "",
    used: formatErrorBytes(detail.usedBytes),
    incoming: firstImport
      ? decimalMB(detail.incomingBytes)
      : formatErrorBytes(detail.incomingBytes),
    limit: firstImport ? decimalMB(detail.limitBytes) : formatErrorBytes(detail.limitBytes),
    remaining: formatErrorBytes(detail.remainingBytes),
    required: lowMemory ? gib(detail.requiredBytes) : formatErrorBytes(detail.incomingBytes),
    available: lowMemory ? gib(detail.availableBytes) : formatErrorBytes(detail.availableBytes),
  };
}
/** Convert invoke failures to readable text: preserve string errors, localize known codes, and
 * otherwise use the backend's English diagnostic message.
 */
export function explainError(error: unknown): string {
  if (typeof error === "string") return error;
  if (!error || typeof error !== "object") return String(error);
  const detail = error as AppErrorDetail;
  if (detail.code === "library_upgrade_required")
    window.dispatchEvent(new Event("library-upgrade-required"));
  if (detail.code && i18n.exists(`errors.${detail.code}`)) {
    return i18n.t(`errors.${detail.code}`, errorParams(detail));
  }
  return detail.message ?? String(error);
}
