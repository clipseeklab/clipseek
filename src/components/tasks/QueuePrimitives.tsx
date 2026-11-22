import type { ReactNode } from "react";
import { CheckCircle2, CircleAlert, Clock3, ListChecks, Loader2 } from "lucide-react";
import { cn } from "@/lib/utils";
export function QueueToolbar({ children, actions }: { children: ReactNode; actions?: ReactNode }) {
  return (
    <div className="flex min-h-11 items-center justify-between gap-3 border-b px-4 py-2">
      <div className="min-w-0 text-xs text-muted-foreground">{children}</div>
      {actions && <div className="flex shrink-0 items-center gap-1">{actions}</div>}
    </div>
  );
}
export function QueueList({ children }: { children: ReactNode }) {
  return <div className="max-h-[50vh] min-h-36 space-y-2 overflow-y-auto p-3">{children}</div>;
}
export function QueueEmpty({ children }: { children: ReactNode }) {
  return (
    <div className="flex flex-col items-center gap-2 py-9 text-center text-xs text-muted-foreground">
      <ListChecks className="size-5 opacity-50" />
      <p>{children}</p>
    </div>
  );
}
export function QueueCard({
  children,
  label,
  className,
}: {
  children: ReactNode;
  label: string;
  className?: string;
}) {
  return (
    <article
      aria-label={label}
      className={cn("rounded-lg border bg-background p-3 text-xs", className)}
    >
      {children}
    </article>
  );
}
export function QueueStatusIcon({
  status,
}: {
  status: "pending" | "processing" | "success" | "failed";
}) {
  const Icon =
    status === "processing"
      ? Loader2
      : status === "success"
        ? CheckCircle2
        : status === "failed"
          ? CircleAlert
          : Clock3;
  return (
    <span
      aria-hidden="true"
      className={cn(
        "flex size-7 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground",
        status === "success" && "bg-emerald-500/10 text-emerald-600",
        status === "failed" && "bg-destructive/10 text-destructive",
      )}
    >
      <Icon className={cn("size-3.5", status === "processing" && "animate-spin")} />
    </span>
  );
}
