import { useProcessingQueueSummary } from "@/hooks/useProcessingQueueSummary";
import { queryKeys } from "@/lib/queryKeys";
/** Durable per-material processing queue; the in-memory store only dispatches new batches. */
import { useEffect, useId, useState } from "react";
import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { ListChecks, Loader2, Pause, Play, Trash2, History, ExternalLink } from "lucide-react";
import { api } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import { useImportStore } from "@/stores/import";
import { activeProcessing, currentProcessingTasks } from "@/lib/processing";
import { explainError } from "@/lib/errors";
import type { ProcessingItem } from "@/types";
import { cn } from "@/lib/utils";
import {
  QueueToolbar,
  QueueList,
  QueueEmpty,
  QueueCard,
  QueueStatusIcon,
} from "@/components/tasks/QueuePrimitives";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
const PAGE_SIZE = 12;
export default function ImportQueueButton({ compact = false }: { compact?: boolean }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const summary = useProcessingQueueSummary(open);
  const pending = useImportStore((s) => s.tasks.filter((task) => task.status === "pending").length);
  const counts = summary.data?.counts ?? {};
  const activeCount = summary.data?.activeCount ?? counts.processing ?? 0;
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          className={cn(
            "relative flex items-center gap-2 rounded-md text-sm",
            compact
              ? "h-8 border bg-background px-3"
              : "w-full px-3 py-2 text-muted-foreground hover:bg-accent",
          )}
        >
          {activeCount > 0 ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <ListChecks className="h-4 w-4" />
          )}
          {t("importQueue.title")}
          {activeCount + pending > 0 && (
            <span className="rounded-full bg-primary px-1.5 text-xs text-primary-foreground">
              {activeCount + pending}
            </span>
          )}
          {(counts.attention ?? 0) > 0 && (
            <span className="rounded bg-destructive px-1.5 text-xs text-destructive-foreground">
              {counts.attention}
            </span>
          )}
        </button>
      </PopoverTrigger>
      <PopoverContent
        aria-label={t("importQueue.title")}
        align={compact ? "end" : "start"}
        side={compact ? "bottom" : "right"}
        className="w-[min(540px,95vw)] p-0"
      >
        <ImportQueueContent open={open} onClose={() => setOpen(false)} summary={summary} />
      </PopoverContent>
    </Popover>
  );
}
export function ImportQueueContent({
  open,
  onClose,
  summary,
}: {
  open: boolean;
  onClose: () => void;
  summary: ReturnType<typeof useProcessingQueueSummary>;
}) {
  const { t } = useTranslation();
  const client = useQueryClient();
  const navigate = useNavigate();
  const libraryId = useAppStore((s) => s.current?.id);
  const pending = useImportStore((s) => s.tasks.filter((task) => task.status === "pending").length);
  const paused = useImportStore((s) => s.queuePaused);
  const setPaused = useImportStore((s) => s.setQueuePaused);
  const [status, setStatus] = useState("processing");
  const [kind, setKind] = useState("all");
  const [page, setPage] = useState(1);
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState("");
  const [actionMessage, setActionMessage] = useState("");
  const filter = { status, kind, page, pageSize: PAGE_SIZE };
  const query = useQuery({
    queryKey: queryKeys.processing.list(libraryId, filter),
    queryFn: () => api.processingList(libraryId!, filter),
    enabled: !!libraryId && open,
    retry: false,
  });
  const data = query.data;
  const counts = summary.data?.counts ?? data?.counts ?? {};
  const pages = Math.max(1, Math.ceil((data?.total ?? 0) / PAGE_SIZE));
  useEffect(() => {
    setPage(1);
    setActionError("");
    setActionMessage("");
  }, [status, kind, libraryId]);
  useEffect(() => {
    if (data) setPage((p) => Math.min(p, pages));
  }, [data, pages]);
  const run = async (action: () => Promise<unknown>) => {
    if (busy) return;
    setBusy(true);
    setActionError("");
    setActionMessage("");
    try {
      await action();
      await Promise.all(
        [
          queryKeys.processing.library(libraryId),
          queryKeys.assets.library(libraryId),
          queryKeys.assetDetail.library(libraryId),
          queryKeys.indexTasks.library(libraryId),
          queryKeys.indexSummary.library(libraryId),
        ].map((queryKey) => client.invalidateQueries({ queryKey })),
      );
    } catch (error) {
      setActionError(explainError(error));
    } finally {
      void client.invalidateQueries({ queryKey: queryKeys.processing.library(libraryId) });
      setBusy(false);
    }
  };
  const showAsset = (id: number) => {
    onClose();
    void navigate(`/assets?assetId=${id}`);
  };
  return (
    <>
      <QueueToolbar
        actions={
          <>
            <Button
              size="xs"
              variant="ghost"
              disabled={busy || !Object.values(counts).some(Boolean)}
              title={t("processingQueue.clearHint")}
              onClick={() =>
                void run(async () => {
                  const count = await api.processingClear(libraryId!);
                  setActionMessage(t("processingQueue.cleared", { count }));
                })
              }
            >
              <Trash2 className="h-3 w-3" />
              {t("processingQueue.clearFinished")}
            </Button>
            <Button
              size="xs"
              variant="ghost"
              onClick={() => setPaused(!paused)}
              title={t("processingQueue.pauseHint")}
            >
              {paused ? <Play className="h-3 w-3" /> : <Pause className="h-3 w-3" />}
              {t(paused ? "importQueue.resume" : "importQueue.pause")}
            </Button>
          </>
        }
      >
        {t("processingQueue.materialCount", {
          count: data?.total ?? Object.values(counts).reduce((a, b) => a + b, 0),
        })}
      </QueueToolbar>
      <div
        className="flex flex-wrap items-center gap-1 border-b bg-muted/20 px-3 py-2"
        role="tablist"
        aria-label={t("processingQueue.filter")}
      >
        {["processing", "attention", "success", "all"].map((tab) => (
          <button
            key={tab}
            role="tab"
            aria-selected={status === tab}
            className={cn(
              "rounded-md px-2 py-1 text-xs text-muted-foreground hover:text-foreground",
              status === tab && "bg-muted text-foreground font-medium",
            )}
            onClick={() => setStatus(tab)}
          >
            {t(`processingQueue.tab.${tab}`)} ·{" "}
            {tab === "all" ? Object.values(counts).reduce((a, b) => a + b, 0) : (counts[tab] ?? 0)}
          </button>
        ))}
        <select
          aria-label={t("processingQueue.type")}
          value={kind}
          onChange={(e) => setKind(e.target.value)}
          className="ml-auto h-7 rounded-md border bg-background px-2 text-xs"
        >
          {["all", "import", "visual", "subtitles", "warning"].map((value) => (
            <option key={value} value={value}>
              {t(`processingQueue.kind.${value}`)}
            </option>
          ))}
        </select>
      </div>
      {!!pending && (
        <p role="status" className="border-b px-3 py-2 text-xs">
          {t("processingQueue.pendingDispatch", { count: pending })}
          {paused && ` · ${t("importQueue.pause")}`}
        </p>
      )}
      {(query.error || summary.error || actionError) && (
        <div role="alert" className="p-3 text-xs text-destructive">
          {actionError || explainError(query.error || summary.error)}
          {(query.isError || summary.isError) && (
            <Button
              size="sm"
              variant="outline"
              onClick={() => {
                void query.refetch();
                void summary.refetch();
              }}
            >
              {t("common.retry")}
            </Button>
          )}
        </div>
      )}
      {actionMessage && (
        <p role="status" className="px-3 py-2 text-xs text-muted-foreground">
          {actionMessage}
        </p>
      )}
      <QueueList>
        {query.isPending && (
          <p role="status" className="p-4 text-sm">
            {t("common.loading")}
          </p>
        )}
        {data?.items.length === 0 && <QueueEmpty>{t("importQueue.empty")}</QueueEmpty>}
        {data?.items.map((item) => (
          <QueueItem
            key={`${libraryId}:${item.key}`}
            libraryId={libraryId!}
            item={item}
            busy={busy}
            onShow={item.assetId == null ? undefined : () => showAsset(item.assetId!)}
            onRetry={(id) => void run(() => api.processingRetry(libraryId!, id))}
            onCancel={(id) => void run(() => api.processingCancel(libraryId!, id))}
            onHide={() => void run(() => api.processingHide(libraryId!, item.key))}
          />
        ))}
      </QueueList>
      <div className="flex min-h-10 items-center justify-between border-t px-4 py-2 text-xs text-muted-foreground">
        <span>
          {page} / {pages} · {t("processingQueue.materialCount", { count: data?.total ?? 0 })}
        </span>
        <div className="flex gap-1"></div>
      </div>
    </>
  );
}
function QueueItem({
  libraryId,
  item,
  busy,
  onShow,
  onRetry,
  onHide,
  onCancel,
}: {
  libraryId: string;
  item: ProcessingItem;
  busy: boolean;
  onShow?: () => void;
  onRetry: (id: number) => void;
  onCancel: (id: number) => void;
  onHide: () => void;
}) {
  const { t } = useTranslation();
  const [historyOpen, setHistoryOpen] = useState(false);
  const historyId = useId();
  const history = useInfiniteQuery({
    queryKey: queryKeys.processing.history(libraryId, item.key),
    queryFn: ({ pageParam }) => api.processingHistory(libraryId, item.key, pageParam),
    initialPageParam: undefined as number | undefined,
    getNextPageParam: (last) => last.nextCursor ?? undefined,
    enabled: historyOpen,
    retry: false,
  });
  const tasks = currentProcessingTasks(item.tasks);
  const active = tasks.some(activeProcessing);
  return (
    <QueueCard label={item.fileName} className="flex items-start gap-3">
      <QueueStatusIcon
        status={item.status === "attention" ? "failed" : active ? "processing" : "success"}
      />
      <div className="min-w-0 flex-1 space-y-1">
        <div className="flex items-start justify-between gap-3">
          <p className="min-w-0 truncate font-medium" title={item.sourcePath}>
            {item.fileName}
          </p>
          <span
            className={cn(
              "shrink-0",
              item.status === "attention" ? "text-destructive" : "text-muted-foreground",
            )}
          >
            {t(`processingQueue.tab.${item.status}`)}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <div className="flex min-w-0 flex-1 flex-wrap gap-x-3 gap-y-0.5 text-xs leading-5 text-muted-foreground">
            {tasks.map((task) => (
              <span key={task.id} className="inline-flex max-w-full flex-wrap items-center gap-1">
                <span>
                  {t(`processingQueue.kind.${task.kind}`)} ·{" "}
                  {t(`processingQueue.state.${task.status}`)}
                  {task.kind === "import" &&
                    activeProcessing(task) &&
                    ` · ${t(`stage.${task.phase}`, { defaultValue: t("stage.processing") })}`}
                </span>
                {activeProcessing(task) && task.indexTaskId != null && (
                  <Button
                    size="xs"
                    className="px-1.5"
                    variant="ghost"
                    disabled={busy}
                    onClick={() => onCancel(task.id)}
                  >
                    {t("common.cancel")}
                  </Button>
                )}
                {["failed", "interrupted", "cancelled", "no_source"].includes(task.status) &&
                  task.kind !== "warning" && (
                    <Button
                      size="xs"
                      className="px-1.5"
                      variant="ghost"
                      disabled={busy}
                      onClick={() => onRetry(task.id)}
                    >
                      {t("common.retry")}
                    </Button>
                  )}
              </span>
            ))}
          </div>
          <div className="flex shrink-0 items-center text-muted-foreground">
            <Button
              size="icon-xs"
              variant="ghost"
              aria-label={t("processingQueue.history")}
              title={t("processingQueue.history")}
              aria-expanded={historyOpen}
              aria-controls={historyId}
              onClick={() => setHistoryOpen(!historyOpen)}
            >
              <History />
            </Button>
            {onShow && (
              <Button
                size="icon-xs"
                variant="ghost"
                aria-label={t("assets.details")}
                title={t("assets.details")}
                onClick={onShow}
              >
                <ExternalLink />
              </Button>
            )}
            <Button
              size="icon-xs"
              variant="ghost"
              aria-label={t("processingQueue.clearRecord")}
              title={t("processingQueue.clearRecord")}
              disabled={active || busy}
              onClick={onHide}
            >
              <Trash2 />
            </Button>
          </div>
        </div>
        {tasks
          .filter((task) => task.error)
          .map((task) => (
            <p key={task.id} className="whitespace-pre-wrap break-words text-xs text-destructive">
              {task.error}
            </p>
          ))}
        {(item.warningCount ?? 0) > 1 && (
          <p className="text-xs text-destructive">
            {t("processingQueue.unresolvedWarnings", { count: item.warningCount })}
          </p>
        )}
        <div
          id={historyId}
          hidden={!historyOpen}
          className="space-y-1 border-t pt-2 text-xs text-muted-foreground"
        >
          <p className="break-all">{item.sourcePath}</p>
          {historyOpen && history.isPending && <p role="status">{t("common.loading")}</p>}
          {history.error && (
            <p role="alert">
              {explainError(history.error)}{" "}
              <Button size="xs" variant="ghost" onClick={() => void history.refetch()}>
                {t("common.retry")}
              </Button>
            </p>
          )}
          {history.data?.pages
            .flatMap((p) => p.tasks)
            .map((task) => (
              <p key={task.id} className="break-words">
                <time className="mr-2">
                  {new Date((task.updatedAt ?? item.createdAt) * 1000).toLocaleString()}
                </time>
                {t(`processingQueue.kind.${task.kind}`)} · #{task.attempt} ·{" "}
                {t(`processingQueue.state.${task.status}`)}
                {task.error && ` · ${task.error}`}
              </p>
            ))}
          {history.hasNextPage && (
            <Button
              size="xs"
              variant="ghost"
              disabled={history.isFetchingNextPage}
              onClick={() => void history.fetchNextPage()}
            >
              {t("assets.loadMore")}
            </Button>
          )}
        </div>
      </div>
    </QueueCard>
  );
}
