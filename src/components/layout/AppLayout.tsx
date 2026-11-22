/** Main layout with sidebar, routed content, and global drag/drop overlay. */
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import AppContentRoutes from "./AppContentRoutes";
import { AlertTriangle, PanelLeftClose, PanelLeftOpen, Upload, X } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import Sidebar from "./Sidebar";
import { Toaster } from "@/components/Toaster";
import { useGlobalDnd } from "@/hooks/useGlobalDnd";
import { useImportStore } from "@/stores/import";
import { useAppStore } from "@/stores/app";
import { Button } from "@/components/ui/button";
import { getCurrentWindow } from "@tauri-apps/api/window";
import ImportMenuButton from "@/components/import/ImportMenuButton";
import { ManagedAssetRevealDialog } from "@/components/assets/ManagedAssetReveal";
import ImportRecoveryDialog from "@/components/import/ImportRecoveryDialog";
import { api } from "@/lib/api";
import { isWindows } from "@/lib/platform";
import VectorRecoveryDialog from "@/components/VectorRecoveryDialog";
export default function AppLayout() {
  const { t } = useTranslation();
  const dragging = useImportStore((s) => s.dragging);
  const setDropFolderId = useImportStore((s) => s.setDropFolderId);
  const sidebarHidden = useAppStore((s) => s.sidebarHidden);
  const setSidebarHidden = useAppStore((s) => s.setSidebarHidden);
  const volumeWarning = useAppStore((s) => s.volumeWarning);
  const setVolumeWarning = useAppStore((s) => s.setVolumeWarning);
  useGlobalDnd();
  // Default drops are uncategorized; folder pages override the target ID.
  useEffect(() => {
    setDropFolderId(null);
  }, [setDropFolderId]);
  // Suggest relocation once per library when the backend detects network storage.
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      unlisten = await listen("library://volume-warning", ({ payload }) => {
        const meta = payload as {
          id?: string;
        };
        if (meta?.id) setVolumeWarning(meta.id);
      });
      const warning = await api.libraryVolumeWarning().catch(() => null);
      if (!disposed && warning?.libraryId) setVolumeWarning(warning.libraryId);
      if (disposed) {
        unlisten();
        unlisten = undefined;
      }
    })();
    return () => {
      disposed = true;
      const stop = unlisten;
      unlisten = undefined;
      stop?.();
    };
  }, [setVolumeWarning]);
  return (
    <div className="relative flex h-full w-full" data-sidebar-hidden={sidebarHidden}>
      {volumeWarning && (
        <div className="absolute inset-x-0 top-0 z-[70] flex items-center gap-2 border-b border-amber-500/30 bg-amber-500/10 px-4 py-1.5 text-xs text-amber-600 dark:text-amber-400">
          <AlertTriangle className="size-3.5 shrink-0" />
          <span className="min-w-0 flex-1 truncate">{t("volumeWarning.text")}</span>
          <button
            className="rounded p-0.5 hover:bg-amber-500/20"
            aria-label={t("volumeWarning.dismiss")}
            onClick={() => setVolumeWarning(null)}
          >
            <X className="size-3.5" />
          </button>
        </div>
      )}
      {!isWindows && (
        <div
          data-tauri-drag-region
          className="absolute inset-x-24 top-0 z-30 h-4 cursor-default"
          onMouseDown={(event) => {
            if (event.button === 0) void getCurrentWindow().startDragging();
          }}
        />
      )}
      {!isWindows && (
        <Button
          variant="ghost"
          size="icon"
          className={`absolute top-1 z-40 h-8 w-8 transition-[left] ${sidebarHidden ? "left-[100px]" : "left-[184px]"}`}
          title={t(sidebarHidden ? "layout.showSidebar" : "layout.hideSidebar")}
          onClick={() => setSidebarHidden(!sidebarHidden)}
        >
          {sidebarHidden ? (
            <PanelLeftOpen className="h-4 w-4" />
          ) : (
            <PanelLeftClose className="h-4 w-4" />
          )}
        </Button>
      )}
      {!isWindows && (
        <ImportMenuButton
          className={`absolute top-1 z-40 h-8 w-8 transition-[left] ${sidebarHidden ? "left-[64px]" : "left-[148px]"}`}
        />
      )}
      {!sidebarHidden && <Sidebar />}
      <main className="relative min-w-0 flex-1 overflow-hidden">
        <AppContentRoutes />
      </main>
      <Toaster />
      <ManagedAssetRevealDialog />
      <ImportRecoveryDialog />
      <VectorRecoveryDialog />

      {/* Drop overlay. */}
      {dragging && (
        <div className="pointer-events-none absolute inset-0 z-50 flex items-center justify-center bg-primary/10 backdrop-blur-[2px]">
          <div className="flex items-center gap-2 rounded-xl border-2 border-dashed border-primary bg-card px-6 py-4 text-sm font-medium shadow-lg">
            <Upload className="h-5 w-5" />
            {t("assets.dropHere")}
          </div>
        </div>
      )}
    </div>
  );
}
