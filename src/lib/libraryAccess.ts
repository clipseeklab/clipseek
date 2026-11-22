import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api } from "@/lib/api";
import type { AppErrorShape, LibraryMeta } from "@/types";
function databaseOpenFailed(error: unknown): error is AppErrorShape {
  return Boolean(
    error &&
    typeof error === "object" &&
    (error as AppErrorShape).code === "library_database_open_failed",
  );
}
/** Recover lost directory authorization by reselecting the original library, then retry. Return
 * null for cancellation and propagate other errors.
 */
export async function withLibraryAuthorization<T>(
  operation: () => Promise<T>,
  candidates: LibraryMeta[],
  dialogTitle: (library: LibraryMeta) => string,
): Promise<T | null> {
  const remaining = [...candidates];
  while (true) {
    try {
      return await operation();
    } catch (error) {
      if (!databaseOpenFailed(error) || remaining.length === 0) throw error;
      const message = error.message ?? "";
      const index = remaining.findIndex((library) => message.includes(library.path));
      const [library] = remaining.splice(index >= 0 ? index : 0, 1);
      const selected = await openDialog({
        directory: true,
        defaultPath: library.path,
        title: dialogTitle(library),
      });
      if (typeof selected !== "string") return null;
      await api.libraryReauthorize(library.id, selected);
    }
  }
}
