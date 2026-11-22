import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "@/lib/api";
import { explainError } from "@/lib/errors";
import type { OperationProgress } from "@/types";
/** Manage the progress subscription and snapshot for an open library dialog. */
export function useLibraryOperations(open: boolean) {
  const [operation, setOperation] = useState<OperationProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (!open) return;
    let disposed = false;
    let receivedProgress = false;
    let stop: (() => void) | undefined;
    setError(null);
    const load = async () => {
      try {
        const unlisten = await listen<OperationProgress>("operation://progress", ({ payload }) => {
          if (disposed) return;
          receivedProgress = true;
          setOperation(payload);
        });
        if (disposed) {
          unlisten();
          return;
        }
        stop = unlisten;
      } catch (cause) {
        if (!disposed) setError(explainError(cause));
      }
      if (disposed) return;
      try {
        const items = await api.libraryOperations();
        if (!disposed && !receivedProgress) setOperation(items[items.length - 1] ?? null);
      } catch (cause) {
        if (!disposed) setError(explainError(cause));
      }
    };
    void load();
    return () => {
      disposed = true;
      stop?.();
    };
  }, [open, attempt]);
  return { operation, error, retry: () => setAttempt((value) => value + 1) };
}
