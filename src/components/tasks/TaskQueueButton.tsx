import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Tabs } from "radix-ui";
import { CircleAlert, ListChecks, Loader2 } from "lucide-react";
import { useAppStore } from "@/stores/app";
import { useImportStore } from "@/stores/import";
import { useProcessingQueueSummary } from "@/hooks/useProcessingQueueSummary";
import { ImportQueueContent } from "@/components/import/ImportQueueSheet";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
export default function TaskQueueButton() {
  const libraryId = useAppStore((s) => s.current?.id);
  return <LibraryTaskQueueButton key={libraryId ?? "no-library"} />;
}
function LibraryTaskQueueButton() {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [tab, setTab] = useState("import");
  const libraryId = useAppStore((s) => s.current?.id);
  const pending = useImportStore((s) => s.tasks.filter((task) => task.status === "pending").length);
  const summary = useProcessingQueueSummary(open);
  const counts = summary.data?.counts ?? {};
  const total = Object.values(counts).reduce((a, b) => a + b, 0) + pending;
  const running = pending > 0 || (summary.data?.activeCount ?? 0) > 0;
  const attention = !!summary.error || (counts.attention ?? 0) > 0;
  const tabs = [{ value: "import", label: t("taskQueue.import"), count: total, icon: ListChecks }];
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          className="relative h-9"
          aria-label={t("taskQueue.title")}
        >
          {running ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            <ListChecks className="size-4" />
          )}
          {t("taskQueue.title")}
          {total > 0 && (
            <Badge
              className="absolute -top-2 -right-2 h-5 min-w-5 px-1 text-[10px] tabular-nums ring-2 ring-background"
              title={t("taskQueue.count", { count: total })}
            >
              {total}
            </Badge>
          )}
          {attention && (
            <CircleAlert
              className="size-3.5 text-destructive"
              aria-label={t("taskQueue.attention")}
            />
          )}
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        aria-label={t("taskQueue.title")}
        className="w-[44rem] max-w-[calc(100vw-2rem)] overflow-hidden rounded-xl p-0"
      >
        <Tabs.Root
          value={tab}
          onValueChange={setTab}
          orientation="vertical"
          className="grid max-h-[min(75vh,var(--radix-popover-content-available-height))] grid-cols-[minmax(0,1fr)_auto] grid-rows-[minmax(0,1fr)]"
        >
          <Tabs.List
            aria-label={t("taskQueue.title")}
            className="col-start-2 row-start-1 flex w-12 min-w-0 flex-col gap-1 overflow-y-auto border-l bg-muted/25 p-1.5 min-[480px]:w-40"
          >
            {tabs.map(({ value, label, count, icon: Icon }) => (
              <Tabs.Trigger
                key={value}
                value={value}
                title={label}
                aria-label={`${label} ${count}`}
                className="group relative flex min-h-10 shrink-0 flex-col items-center justify-center gap-0.5 rounded-md border-r-2 border-transparent px-1.5 py-1 text-left text-sm font-medium text-muted-foreground outline-none hover:bg-accent/70 hover:text-foreground focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring data-[state=active]:border-primary data-[state=active]:bg-accent data-[state=active]:text-accent-foreground min-[480px]:flex-row min-[480px]:justify-start min-[480px]:gap-2 min-[480px]:px-2 min-[480px]:py-2"
              >
                <Icon className="size-4 shrink-0" />
                <span className="sr-only min-[480px]:not-sr-only min-[480px]:flex-1">{label}</span>
                <span className="shrink-0 rounded-full bg-muted px-1 text-[10px] leading-4 tabular-nums group-data-[state=active]:bg-primary/10 group-data-[state=active]:text-foreground min-[480px]:px-1.5 min-[480px]:text-[11px]">
                  {count}
                </span>
              </Tabs.Trigger>
            ))}
          </Tabs.List>

          <Tabs.Content
            value="import"
            forceMount
            hidden={tab !== "import"}
            className="col-start-1 row-start-1 min-h-0 min-w-0 overflow-y-auto outline-none"
          >
            {libraryId ? (
              <ImportQueueContent
                open={tab === "import"}
                onClose={() => setOpen(false)}
                summary={summary}
              />
            ) : (
              <p className="p-6 text-center text-sm text-muted-foreground">
                {t("online.openLibrary")}
              </p>
            )}
          </Tabs.Content>
        </Tabs.Root>
      </PopoverContent>
    </Popover>
  );
}
