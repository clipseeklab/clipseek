import { queryKeys } from "@/lib/queryKeys";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { useAppStore } from "@/stores/app";
import { api } from "@/lib/api";
import type { ProcessingTask } from "@/types";
import { currentProcessingTasks } from "@/lib/processing";
import { Badge } from "@/components/ui/badge";
export default function AssetIndexStatus({
  id,
  kind,
  indexed,
  available = true,
  label,
  tasks,
}: {
  id: number;
  kind: "visual" | "subtitles";
  indexed?: boolean;
  available?: boolean;
  label?: string;
  tasks?: ProcessingTask[];
}) {
  const { t } = useTranslation();
  const libraryId = useAppStore((s) => s.current?.id);
  const { data } = useQuery({
    queryKey: queryKeys.indexTasks.asset(libraryId, id),
    queryFn: () => api.indexTasks(id),
    enabled: tasks === undefined,
    refetchInterval: (query) =>
      query.state.data?.some((task) => task.status === "queued" || task.status === "running")
        ? 1500
        : false,
  });
  const task =
    tasks !== undefined
      ? currentProcessingTasks(tasks).find((task) => task.kind === kind)
      : data?.filter((item) => item.kind === kind).sort((a, b) => b.id - a.id)[0];
  const state =
    task && ["queued", "running"].includes(task.status)
      ? task.status
      : indexed
        ? "indexed"
        : task && ["failed", "interrupted", "cancelled", "no_source"].includes(task.status)
          ? task.status
          : !available
            ? "no_source"
            : "unindexed";
  return (
    <span className="inline-flex items-center gap-2">
      {label !== "" && (
        <span className="text-muted-foreground">{label ?? t(`indexManagement.${kind}`)}</span>
      )}
      <Badge
        variant="secondary"
        className="bg-muted/80 font-normal text-muted-foreground backdrop-blur-sm"
        title={task?.error ?? undefined}
      >
        {t(`indexManagement.${state}`, { defaultValue: t(`processingQueue.state.${state}`) })}
      </Badge>
    </span>
  );
}
