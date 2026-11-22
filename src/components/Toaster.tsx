/** Lightweight toast notifications without an extra dependency. */
import { create } from "zustand";
import { createPortal } from "react-dom";
import { CheckCircle2, AlertCircle, X } from "lucide-react";
import { cn } from "@/lib/utils";
type ToastKind = "success" | "error" | "info";
interface ToastItem {
  id: number;
  kind: ToastKind;
  message: string;
  persistent: boolean;
}
interface ToastStore {
  toasts: ToastItem[];
  push: (kind: ToastKind, message: string, persistent?: boolean) => void;
  remove: (id: number) => void;
}
let nextId = 1;
export const useToastStore = create<ToastStore>((set) => ({
  toasts: [],
  push: (kind, message, persistent = false) => {
    const id = nextId++;
    set((s) => ({ toasts: [...s.toasts, { id, kind, message, persistent }] }));
    if (!persistent) {
      setTimeout(() => {
        set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
      }, 3500);
    }
  },
  remove: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));
export const toast = {
  success: (m: string) => useToastStore.getState().push("success", m),
  error: (m: string) => useToastStore.getState().push("error", m),
  info: (m: string) => useToastStore.getState().push("info", m),
  persistentSuccess: (m: string) => useToastStore.getState().push("success", m, true),
  persistentError: (m: string) => useToastStore.getState().push("error", m, true),
};
const icons = {
  success: <CheckCircle2 className="h-4 w-4 shrink-0 text-green-600" />,
  error: <AlertCircle className="h-4 w-4 shrink-0 text-red-600" />,
  info: <AlertCircle className="h-4 w-4 shrink-0 text-muted-foreground" />,
};
export function Toaster() {
  const { toasts, remove } = useToastStore();
  // Portal to body above Radix dialog/dropdown layers so toasts remain visible.
  return createPortal(
    <div className="pointer-events-none fixed bottom-4 right-4 z-[100] flex w-80 flex-col gap-2">
      {toasts.map((t) =>
        t.persistent ? (
          <button
            key={t.id}
            type="button"
            className="pointer-events-auto flex w-full items-start gap-2 rounded-md border bg-popover px-3 py-2 text-left text-sm shadow-md transition-colors hover:bg-accent"
            onClick={() => remove(t.id)}
          >
            {icons[t.kind]}
            <span className="min-w-0 flex-1 break-words">{t.message}</span>
            <X className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          </button>
        ) : (
          <div
            key={t.id}
            className={cn(
              "pointer-events-auto flex items-start gap-2 rounded-md border bg-popover px-3 py-2 text-sm shadow-md",
            )}
          >
            {icons[t.kind]}
            <span className="min-w-0 flex-1 break-words">{t.message}</span>
            <button
              className="text-muted-foreground hover:text-foreground"
              onClick={() => remove(t.id)}
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        ),
      )}
    </div>,
    document.body,
  );
}
