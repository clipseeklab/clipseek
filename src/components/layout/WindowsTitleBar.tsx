import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLocation } from "react-router-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, PanelLeftClose, PanelLeftOpen, Square, X } from "lucide-react";
import appIcon from "../../../src-tauri/icons/32x32.png";
import { useAppStore } from "@/stores/app";
import { api } from "@/lib/api";
import ImportMenuButton from "@/components/import/ImportMenuButton";
function pageTitle(pathname: string): string {
  if (pathname === "/assets") return "nav.assets";
  if (pathname === "/tags" || pathname.startsWith("/tag/")) return "nav.tags";
  if (pathname === "/uncategorized") return "nav.uncategorized";
  if (pathname === "/folders" || pathname.startsWith("/folders/")) return "nav.folders";
  if (pathname === "/trash") return "nav.trash";
  return "nav.search";
}
export default function WindowsTitleBar({ mainActions }: { mainActions: boolean }) {
  const { t, i18n } = useTranslation();
  const { pathname } = useLocation();
  const sidebarHidden = useAppStore((state) => state.sidebarHidden);
  const setSidebarHidden = useAppStore((state) => state.setSidebarHidden);
  const [maximized, setMaximized] = useState(false);
  const appWindow = useMemo(getCurrentWindow, []);
  useEffect(() => {
    void api.traySetLabels(t("window.trayShow"), t("window.trayQuit")).catch(() => {});
  }, [i18n.resolvedLanguage, t]);
  useEffect(() => {
    void appWindow.isMaximized().then(setMaximized);
    const unlisten = appWindow.onResized(() => {
      void appWindow.isMaximized().then(setMaximized);
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, [appWindow]);
  const toggleMaximize = async () => {
    await appWindow.toggleMaximize();
    setMaximized(await appWindow.isMaximized());
  };
  return (
    <header
      className="windows-titlebar flex h-12 shrink-0 items-center border-b bg-card"
      onMouseDown={(event) => {
        if (
          event.button === 0 &&
          event.currentTarget.contains(event.target as Node) &&
          !(event.target as Element).closest("[data-no-drag]")
        )
          void appWindow.startDragging();
      }}
      onDoubleClick={(event) => {
        if (
          event.currentTarget.contains(event.target as Node) &&
          !(event.target as Element).closest("[data-no-drag]")
        )
          void toggleMaximize();
      }}
    >
      <div className="flex h-full w-56 shrink-0 items-center gap-2 px-3">
        <img src={appIcon} alt="" className="size-5 shrink-0" />
        <span className="truncate text-sm font-semibold">ClipSeek</span>
        {mainActions && (
          <div className="ml-auto flex items-center gap-1" data-no-drag>
            <ImportMenuButton className="size-8" />
            <button
              type="button"
              className="flex size-8 items-center justify-center rounded-md text-foreground hover:bg-accent"
              title={t(sidebarHidden ? "layout.showSidebar" : "layout.hideSidebar")}
              aria-label={t(sidebarHidden ? "layout.showSidebar" : "layout.hideSidebar")}
              onClick={() => setSidebarHidden(!sidebarHidden)}
            >
              {sidebarHidden ? (
                <PanelLeftOpen className="size-4" />
              ) : (
                <PanelLeftClose className="size-4" />
              )}
            </button>
          </div>
        )}
      </div>
      <div className="flex h-full min-w-0 flex-1 items-center px-6">
        <span className="truncate text-sm font-medium text-muted-foreground pointer-events-none">
          {mainActions ? t(pageTitle(pathname)) : ""}
        </span>
      </div>
      <div className="flex h-full shrink-0" data-no-drag>
        <button
          type="button"
          className="window-control"
          aria-label={t("window.minimize")}
          title={t("window.minimize")}
          onClick={() => void appWindow.minimize()}
        >
          <Minus className="size-4" />
        </button>
        <button
          type="button"
          className="window-control"
          aria-label={t(maximized ? "window.restore" : "window.maximize")}
          title={t(maximized ? "window.restore" : "window.maximize")}
          onClick={() => void toggleMaximize()}
        >
          {maximized ? <Copy className="size-3.5" /> : <Square className="size-3.5" />}
        </button>
        <button
          type="button"
          className="window-control window-control-close"
          aria-label={t("window.close")}
          title={t("window.close")}
          onClick={() => void appWindow.hide()}
        >
          <X className="size-4" />
        </button>
      </div>
    </header>
  );
}
