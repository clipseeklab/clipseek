import { queryKeys } from "@/lib/queryKeys";
import SidebarFolders from "@/components/layout/SidebarFolders";
import { isWindows } from "@/lib/platform";
/** Sidebar with library switcher, navigation, folders, and settings. */
import { NavLink } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { ChevronDown, FolderOpen, Images, Inbox, Search, Settings, Tags } from "lucide-react";
import { assetPagesOptions } from "@/hooks/useAssetPages";
import { api } from "@/lib/api";
import { explainError } from "@/lib/errors";
import { withLibraryAuthorization } from "@/lib/libraryAccess";
import { toast } from "@/components/Toaster";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/stores/app";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import LibraryManageDialog from "@/components/library/LibraryManageDialog";
import { LibraryColorIcon } from "@/components/library/LibraryBadge";
import SettingsDialog from "@/components/settings/SettingsDialog";
import { useState } from "react";
import FolderSidebarDialog from "@/components/folders/FolderSidebarDialog";
import UsageTips from "@/components/search/UsageTips";
const navItems = [
  { to: "/assets", icon: Images, key: "assets" },
  { to: "/search", icon: Search, key: "search" },
  { to: "/tags", icon: Tags, key: "tags" },
  { to: "/uncategorized", icon: Inbox, key: "uncategorized" },
  { to: "/folders", icon: FolderOpen, key: "folders" },
] as const;
export default function Sidebar() {
  const { t } = useTranslation();
  const { current, libraries, setCurrent } = useAppStore();
  const [manageOpen, setManageOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [pinsOpen, setPinsOpen] = useState(false);
  const hasUpdate = false;
  const { data: folderSidebar } = useQuery({
    queryKey: queryKeys.folders.sidebar(current?.id),
    queryFn: api.folderSidebar,
  });
  const { data: tags = [] } = useQuery({
    queryKey: queryKeys.tags.library(current?.id),
    queryFn: api.tagList,
  });
  // Reuse the first page for both the badge and the first visit to the list.
  const { data: assetCount = 0 } = useInfiniteQuery({
    ...assetPagesOptions({}, current?.id),
    select: (data) => data.pages[0]?.total ?? 0,
  });
  const { data: uncategorizedCount = 0 } = useInfiniteQuery({
    ...assetPagesOptions({ uncategorized: true }, current?.id),
    select: (data) => data.pages[0]?.total ?? 0,
  });
  const { data: trashCount } = useInfiniteQuery({
    ...assetPagesOptions({ inTrash: true }, current?.id),
    select: (data) => data.pages[0]?.total ?? 0,
  });
  const trashLabel =
    trashCount == null
      ? t("nav.trash")
      : trashCount > 0
        ? t("nav.trashFull", { count: trashCount })
        : t("nav.trashEmpty");
  const navCounts: Partial<Record<(typeof navItems)[number]["key"], number>> = {
    assets: assetCount,
    tags: tags.length,
    uncategorized: uncategorizedCount,
    folders: folderSidebar?.total ?? 0,
  };
  const switchTo = async (id: string) => {
    const library = libraries.find((item) => item.id === id);
    if (!library) return;
    try {
      const selected = await withLibraryAuthorization(
        () => api.librarySwitch(id),
        [library],
        (item) => t("library.reauthorizeTitle", { name: item.name }),
      );
      if (selected) setCurrent(selected);
    } catch (error) {
      toast.error(explainError(error));
    }
  };
  return (
    <aside
      onDragStartCapture={(event) => event.preventDefault()}
      className="flex h-full w-56 shrink-0 select-none flex-col border-r bg-card"
    >
      {/* Library switcher. */}
      <div
        data-tauri-drag-region={isWindows ? undefined : true}
        className="sidebar-library-row flex items-center px-3 pb-3 pt-12"
      >
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button className="flex min-w-0 flex-1 items-center gap-2 rounded-md border bg-background px-2.5 py-1.5 text-left text-sm font-medium hover:bg-accent">
              <LibraryColorIcon color={current?.color} size={18} />
              <span className="min-w-0 flex-1 truncate">{current?.name}</span>
              <ChevronDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-52">
            <DropdownMenuLabel>{t("library.switch")}</DropdownMenuLabel>
            {libraries.map((lib) => (
              <DropdownMenuItem
                key={lib.id}
                disabled={lib.id === current?.id}
                onClick={() => void switchTo(lib.id)}
              >
                <LibraryColorIcon color={lib.color} size={16} />
                <span className="min-w-0 flex-1 truncate">{lib.name}</span>
              </DropdownMenuItem>
            ))}
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={() => setManageOpen(true)}>
              {t("library.manage")}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {/* Main navigation. */}
      <nav className="flex-1 overflow-y-auto px-3 pb-3">
        {navItems.map(({ to, icon: Icon, key }) => (
          <NavLink
            draggable={false}
            key={key}
            to={to}
            end={to === "/search"}
            className={({ isActive }) =>
              cn(
                "mb-0.5 flex items-center gap-2.5 rounded-md px-3 py-2 text-sm transition-colors",
                isActive
                  ? "bg-accent font-medium text-accent-foreground"
                  : "text-muted-foreground hover:bg-accent/50 hover:text-foreground",
              )
            }
          >
            <Icon className="h-4 w-4 shrink-0" />
            <span className="min-w-0 flex-1 truncate">{t(`nav.${key}`)}</span>
            {navCounts[key] != null && (
              <span className="w-6 shrink-0 text-center text-xs tabular-nums text-muted-foreground/70">
                {navCounts[key]}
              </span>
            )}
          </NavLink>
        ))}

        {/* Folder navigation. */}
        <div className={folderSidebar?.items.length ? "mt-3" : undefined}>
          {!!folderSidebar?.items.length && (
            <div className="mb-1 flex items-center justify-between px-3 text-xs font-medium text-muted-foreground">
              {t("nav.folders")}
              <button
                title={t("folders.managePinned")}
                aria-label={t("folders.managePinned")}
                className="flex h-6 w-6 shrink-0 items-center justify-center rounded hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                onClick={() => setPinsOpen(true)}
              >
                <Settings className="h-3.5 w-3.5" />
              </button>
            </div>
          )}
          <SidebarFolders
            key={current?.id}
            folders={folderSidebar?.items.slice(0, folderSidebar.displayLimit ?? 10)}
          />
        </div>
      </nav>

      {/* Settings entry. */}
      <div className="flex items-center gap-1 border-t p-3">
        <button
          className="relative flex min-w-0 flex-1 items-center gap-2.5 rounded-md px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
          onClick={() => setSettingsOpen(true)}
        >
          <Settings className="h-4 w-4" />
          {t("settings.title")}
          {hasUpdate && (
            <span className="absolute right-2.5 top-1/2 h-2 w-2 -translate-y-1/2 rounded-full bg-red-500" />
          )}
        </button>
        <UsageTips />
        <NavLink
          draggable={false}
          to="/trash"
          aria-label={trashLabel}
          title={trashLabel}
          className={({ isActive }) =>
            cn(
              "relative flex h-9 w-9 shrink-0 items-center justify-center rounded-md transition-colors",
              isActive
                ? "bg-accent text-accent-foreground"
                : "text-muted-foreground hover:bg-accent/50 hover:text-foreground",
            )
          }
        >
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.7"
            strokeLinecap="round"
            strokeLinejoin="round"
            className="h-5 w-5"
            aria-hidden="true"
          >
            <path d="M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14" />
            {!!trashCount && (
              <>
                <path d="m8 7 1-3 3 1M13 7l2-5 3 2-1 3" />
                <path d="M9 11h6v6H9z" fill="currentColor" fillOpacity=".2" />
              </>
            )}
          </svg>
          {!!trashCount && (
            <span className="absolute right-0 top-0 flex min-w-3.5 items-center justify-center rounded-full bg-primary px-0.5 text-[9px] leading-3.5 text-primary-foreground">
              {trashCount > 99 ? "99+" : trashCount}
            </span>
          )}
        </NavLink>
      </div>

      {pinsOpen && <FolderSidebarDialog key={current?.id} open onOpenChange={setPinsOpen} />}
      <LibraryManageDialog open={manageOpen} onOpenChange={setManageOpen} />
      <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
    </aside>
  );
}
