import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { NavLink } from "react-router-dom";
import { Folder as FolderIcon } from "lucide-react";
import type { Folder } from "@/types";
import { cn } from "@/lib/utils";
type Row = {
  folder: Folder;
  phase: "idle" | "enter" | "exit";
};
export default function SidebarFolders({ folders }: { folders?: Folder[] }) {
  const initialized = useRef(folders !== undefined);
  const [rows, setRows] = useState<Row[]>(() =>
    (folders ?? []).map((folder) => ({ folder, phase: "idle" })),
  );
  useLayoutEffect(() => {
    if (!folders) return;
    if (!initialized.current) {
      initialized.current = true;
      setRows(folders.map((folder) => ({ folder, phase: "idle" })));
      return;
    }
    setRows((previous) => {
      const byId = new Map(previous.map((row) => [row.folder.id, row]));
      const active = new Set(folders.map((folder) => folder.id));
      const next: Row[] = folders.map((folder) => {
        const old = byId.get(folder.id);
        return { folder, phase: old && old.phase !== "exit" ? old.phase : "enter" };
      });
      // Keep departing rows in their old gap until they collapse; active order stays authoritative.
      previous.forEach((row, index) => {
        if (active.has(row.folder.id)) return;
        const following = previous.slice(index + 1).find((item) => active.has(item.folder.id));
        const position = following
          ? next.findIndex((item) => item.folder.id === following.folder.id)
          : next.length;
        next.splice(position, 0, { folder: row.folder, phase: "exit" });
      });
      return next;
    });
  }, [folders]);
  const settle = (id: number, phase: Row["phase"]) =>
    setRows((current) =>
      current.flatMap((row) => {
        if (row.folder.id !== id || row.phase !== phase) return [row];
        return phase === "exit" ? [] : [{ ...row, phase: "idle" as const }];
      }),
    );
  return (
    <div>
      {rows.map((row) => (
        <FolderRow key={row.folder.id} row={row} onSettled={settle} />
      ))}
    </div>
  );
}
function FolderRow({
  row: { folder, phase },
  onSettled,
}: {
  row: Row;
  onSettled: (id: number, phase: Row["phase"]) => void;
}) {
  const callback = useRef(onSettled);
  callback.current = onSettled;
  useEffect(() => {
    if (phase === "idle") return;
    // Also finish when CSS animations are disabled or the WebView does not dispatch animationend.
    const timer = window.setTimeout(() => callback.current(folder.id, phase), 240);
    return () => window.clearTimeout(timer);
  }, [folder.id, phase]);
  return (
    <div
      className="sidebar-folder-row"
      data-phase={phase}
      aria-hidden={phase === "exit" || undefined}
      onAnimationEnd={(event) => {
        if (
          event.target === event.currentTarget &&
          event.animationName === `sidebar-folder-${phase}`
        )
          onSettled(folder.id, phase);
      }}
    >
      <NavLink
        draggable={false}
        onDragStart={(event) => event.preventDefault()}
        to={`/folders/${folder.id}`}
        aria-label={`${folder.name} ${folder.assetCount}`}
        tabIndex={phase === "exit" ? -1 : undefined}
        className={({ isActive }) =>
          cn(
            "mb-0.5 flex items-center gap-2.5 rounded-md px-3 py-1.5 text-sm transition-colors",
            phase === "exit" && "pointer-events-none",
            isActive
              ? "bg-accent font-medium text-accent-foreground"
              : "text-muted-foreground hover:bg-accent/50 hover:text-foreground",
          )
        }
      >
        <FolderIcon aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />
        <span className="min-w-0 flex-1 truncate">{folder.name}</span>
        <span className="w-6 shrink-0 text-center text-xs tabular-nums text-muted-foreground/70">
          {folder.assetCount}
        </span>
      </NavLink>
    </div>
  );
}
