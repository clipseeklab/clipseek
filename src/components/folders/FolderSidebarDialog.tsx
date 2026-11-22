import { queryKeys } from "@/lib/queryKeys";
import { useCallback, useEffect, useRef, useState, type PointerEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ArrowDown, ArrowUp, ChevronDown, CircleHelp, GripVertical, X } from "lucide-react";
import { useAppStore } from "@/stores/app";
import { api } from "@/lib/api";
import { explainError } from "@/lib/errors";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
export default function FolderSidebarDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const library = useAppStore((s) => s.current?.id);
  const client = useQueryClient();
  const result = useQuery({
    queryKey: queryKeys.folders.sidebar(library),
    queryFn: api.folderSidebar,
    enabled: open,
    retry: false,
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [dragged, setDragged] = useState<{
    id: number;
    index: number;
  } | null>(null);
  const dragRef = useRef<{
    id: number;
    pointerId: number;
    startY: number;
    y: number;
    index: number;
    moved: boolean;
    ids: number[];
  } | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const rowsRef = useRef(new Map<number, HTMLDivElement>());
  const frameRef = useRef<number | null>(null);
  const stopFrame = useCallback(() => {
    if (frameRef.current != null) cancelAnimationFrame(frameRef.current);
    frameRef.current = null;
  }, []);
  const cancelDrag = useCallback(() => {
    stopFrame();
    dragRef.current = null;
    setDragged(null);
  }, [stopFrame]);
  useEffect(() => {
    cancelDrag();
    return () => {
      stopFrame();
      dragRef.current = null;
    };
  }, [library, open, cancelDrag, stopFrame]);
  const items = result.data?.items ?? [];
  const save = async (ids: number[], displayLimit?: number) => {
    if (!library || busy) return;
    setBusy(true);
    setError("");
    try {
      const next = await (displayLimit === undefined
        ? api.folderSidebarSet(library, ids)
        : api.folderSidebarSet(library, ids, displayLimit));
      client.setQueryData(queryKeys.folders.sidebar(library), next);
      await client.invalidateQueries({ queryKey: queryKeys.folders.library(library) });
    } catch (cause) {
      setError(explainError(cause));
    } finally {
      setBusy(false);
    }
  };
  const move = (from: number, to: number) => {
    if (from === to || to < 0 || to >= items.length) return;
    const ids = items.map((item) => item.id);
    const [id] = ids.splice(from, 1);
    ids.splice(to, 0, id);
    void save(ids);
  };
  // Pointer capture keeps sorting inside the WebView, independent of native file drag/drop.
  const updateTarget = () => {
    const drag = dragRef.current;
    if (!drag?.moved) return;
    const remaining = drag.ids.filter((id) => id !== drag.id);
    const before = remaining.findIndex((id) => {
      const rect = rowsRef.current.get(id)?.getBoundingClientRect();
      return rect && drag.y < rect.top + rect.height / 2;
    });
    drag.index = before < 0 ? remaining.length : before;
    setDragged((previous) =>
      previous?.id === drag.id && previous.index === drag.index
        ? previous
        : { id: drag.id, index: drag.index },
    );
  };
  const scrollFrame = () => {
    frameRef.current = null;
    const drag = dragRef.current;
    const list = listRef.current;
    if (!drag?.moved || !list) return;
    const rect = list.getBoundingClientRect();
    if (drag.y < rect.top + 32) list.scrollTop -= 8;
    else if (drag.y > rect.bottom - 32) list.scrollTop += 8;
    updateTarget();
    frameRef.current = requestAnimationFrame(scrollFrame);
  };
  const startDrag = (event: PointerEvent<HTMLButtonElement>, id: number, index: number) => {
    if (busy || event.button !== 0 || dragRef.current) return;
    event.preventDefault();
    event.currentTarget.focus();
    event.currentTarget.setPointerCapture(event.pointerId);
    dragRef.current = {
      id,
      pointerId: event.pointerId,
      startY: event.clientY,
      y: event.clientY,
      index,
      moved: false,
      ids: items.map((item) => item.id),
    };
  };
  const moveDrag = (event: PointerEvent<HTMLButtonElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    drag.y = event.clientY;
    if (!drag.moved && Math.abs(drag.y - drag.startY) < 4) return;
    drag.moved = true;
    updateTarget();
    if (frameRef.current == null) frameRef.current = requestAnimationFrame(scrollFrame);
  };
  const finishDrag = (event: PointerEvent<HTMLButtonElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    if (drag.moved) {
      drag.y = event.clientY;
      updateTarget();
    }
    cancelDrag();
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
    if (!drag.moved || items.map((item) => item.id).join() !== drag.ids.join()) return;
    const ids = drag.ids.filter((id) => id !== drag.id);
    ids.splice(drag.index, 0, drag.id);
    if (ids.some((id, index) => id !== drag.ids[index])) void save(ids);
  };
  const beforeId = dragged
    ? items.filter((item) => item.id !== dragged.id)[dragged.index]?.id
    : null;
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        onEscapeKeyDown={(event) => {
          if (dragRef.current) {
            event.preventDefault();
            cancelDrag();
          }
        }}
        disableAnimation
        className="flex max-h-[85vh] flex-col gap-3 sm:max-w-lg"
      >
        <DialogHeader className="shrink-0 pr-6">
          <DialogTitle>{t("folders.managePinned")}</DialogTitle>
        </DialogHeader>
        <div className="flex shrink-0 flex-wrap items-center gap-3 rounded-lg bg-muted/40 px-3 py-2">
          <DialogDescription className="mr-auto text-xs tabular-nums">
            {t("folders.pinnedCount", { count: items.length })}
          </DialogDescription>
          <div className="flex items-center gap-2">
            <label htmlFor="pinned-folder-display-limit" className="text-xs text-muted-foreground">
              {t("folders.displayLimit")}
            </label>
            <div className="relative">
              <select
                id="pinned-folder-display-limit"
                className="h-8 w-16 appearance-none rounded-md border border-input bg-background pl-2.5 pr-7 text-sm tabular-nums focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
                value={result.data?.displayLimit ?? 10}
                disabled={busy || !!dragged || !result.data}
                onChange={(event) =>
                  void save(
                    items.map((item) => item.id),
                    Number(event.target.value),
                  )
                }
              >
                {Array.from({ length: 26 }, (_, index) => index + 5).map((value) => (
                  <option key={value} value={value}>
                    {value}
                  </option>
                ))}
              </select>
              <ChevronDown
                aria-hidden
                className="pointer-events-none absolute right-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground"
              />
            </div>
            <Popover>
              <PopoverTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t("folders.displayLimitHelp")}
                  className="size-7 text-muted-foreground"
                >
                  <CircleHelp className="size-4" />
                </Button>
              </PopoverTrigger>
              <PopoverContent
                align="end"
                className="max-w-[calc(100vw-2rem)] p-3 text-xs leading-relaxed text-muted-foreground"
              >
                {t("folders.displayLimitHint")}
              </PopoverContent>
            </Popover>
          </div>
        </div>
        {result.isPending && <p role="status">{t("common.loading")}</p>}
        {result.isError && (
          <div role="alert">
            {explainError(result.error)}
            <Button variant="ghost" onClick={() => void result.refetch()}>
              {t("common.retry")}
            </Button>
          </div>
        )}
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
        <div
          ref={listRef}
          className={`relative max-h-[60vh] min-h-0 space-y-0.5 overflow-y-auto rounded-lg border p-1 [scrollbar-gutter:stable] ${dragged ? "select-none" : ""}`}
        >
          {items.map((item, index) => (
            <div
              key={item.id}
              ref={(element) => {
                if (element) rowsRef.current.set(item.id, element);
                else rowsRef.current.delete(item.id);
              }}
              className={`relative flex items-center gap-2 rounded-md px-2 py-1 text-sm hover:bg-muted/60 ${dragged?.id === item.id ? "bg-muted opacity-50" : ""}`}
            >
              {dragged && item.id === beforeId && (
                <div
                  aria-hidden
                  className="pointer-events-none absolute top-0 left-0 right-0 h-0.5 rounded bg-primary"
                />
              )}
              <button
                type="button"
                aria-label={t("folders.dragReorder", { name: item.name })}
                disabled={busy}
                className="-ml-1 flex h-7 w-7 shrink-0 touch-none select-none items-center justify-center rounded text-muted-foreground hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring cursor-grab active:cursor-grabbing disabled:opacity-50"
                onPointerDown={(event) => startDrag(event, item.id, index)}
                onPointerMove={moveDrag}
                onPointerUp={finishDrag}
                onPointerCancel={cancelDrag}
                onLostPointerCapture={cancelDrag}
                onDragStart={(event) => event.preventDefault()}
              >
                <GripVertical className="pointer-events-none h-4 w-4" />
              </button>
              <span className="min-w-0 flex-1 truncate">{item.name}</span>
              <Button
                variant="ghost"
                size="icon-sm"
                disabled={busy || !!dragged || index === 0}
                aria-label={t("folders.moveUp", { name: item.name })}
                onClick={() => move(index, index - 1)}
              >
                <ArrowUp className="h-3.5 w-3.5" />
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                disabled={busy || !!dragged || index === items.length - 1}
                aria-label={t("folders.moveDown", { name: item.name })}
                onClick={() => move(index, index + 1)}
              >
                <ArrowDown className="h-3.5 w-3.5" />
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                disabled={busy || !!dragged}
                aria-label={t("folders.unpinNamed", { name: item.name })}
                onClick={() =>
                  void save(items.filter((value) => value.id !== item.id).map((value) => value.id))
                }
              >
                <X className="h-3.5 w-3.5" />
              </Button>
            </div>
          ))}
          {dragged && beforeId == null && (
            <div
              aria-hidden
              className="pointer-events-none sticky bottom-0 h-0.5 rounded bg-primary"
            />
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
