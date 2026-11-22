import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useAppStore } from "@/stores/app";
import { useImportStore } from "@/stores/import";
import { useDebouncedValue } from "@/hooks/useDebouncedValue";
import { api } from "@/lib/api";
import { queryKeys } from "@/lib/queryKeys";
import { toast } from "@/components/Toaster";
import { explainError } from "@/lib/errors";
import type { Folder, FolderSort } from "@/types";
export const FOLDER_SORTS: FolderSort[] = ["name_asc", "name_desc", "created_desc", "created_asc"];
function savedFolderSort(): FolderSort {
  try {
    const saved = localStorage.getItem("clipseek:folder-sort") as FolderSort;
    if (FOLDER_SORTS.includes(saved)) return saved;
  } catch {
    /* Use the default when storage is unavailable. */
  }
  return "created_desc";
}
export function useFolderList() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const libraryId = useAppStore((state) => state.current?.id);
  const listRef = useRef<HTMLDivElement>(null);
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(1);
  const [sort, setSort] = useState<FolderSort>(savedFolderSort);
  const debounced = useDebouncedValue(query, 200);
  const result = useQuery({
    queryKey: queryKeys.folders.page(libraryId, debounced, page, sort),
    queryFn: async () => ({ ...(await api.folderPage(debounced, page, sort)), sort }),
    placeholderData: (previous, previousQuery) =>
      previousQuery?.queryKey[1] === libraryId ? previous : undefined,
    retry: false,
  });
  const changeSort = (next: string) => {
    if (!FOLDER_SORTS.includes(next as FolderSort) || next === sort) return;
    setSort(next as FolderSort);
    setPage(1);
    try {
      localStorage.setItem("clipseek:folder-sort", next);
    } catch {
      /* Keep the selected order in memory. */
    }
  };
  const { data: sidebar } = useQuery({
    queryKey: queryKeys.folders.sidebar(libraryId),
    queryFn: api.folderSidebar,
  });
  const folders = result.data?.items ?? [];
  const isLoading = result.isPending;
  const pages = Math.max(1, Math.ceil((result.data?.total ?? 0) / 40));
  useEffect(() => {
    if (result.data && page > pages) setPage(pages);
  }, [result.data, page, pages]);
  useEffect(() => {
    setQuery("");
    setPage(1);
    setSelected([]);
    setSelectedOpen(false);
    setSelectMode(false);
  }, [libraryId]);
  const togglePin = async (id: number) => {
    if (!libraryId) return;
    try {
      await api.folderPin(libraryId, id, !sidebar?.items.some((item) => item.id === id));
      await queryClient.invalidateQueries({ queryKey: queryKeys.folders.library(libraryId) });
    } catch (error) {
      toast.error(explainError(error));
    }
  };
  const [selectMode, setSelectMode] = useState(false);
  const [selected, setSelected] = useState<number[]>([]);
  const [selectedOpen, setSelectedOpen] = useState(false);
  const allPageSelected =
    folders.length > 0 && folders.every((folder) => selected.includes(folder.id));
  const toggleFolder = (id: number) => {
    if (selected.includes(id)) {
      setSelected((values) => values.filter((value) => value !== id));
      return;
    }
    if (selected.length >= 1000) {
      toast.error(t("folders.selectionLimit"));
      return;
    }
    setSelected((values) => [...values, id]);
  };
  const togglePage = () => {
    const pageIds = new Set(folders.map((folder) => folder.id));
    if (allPageSelected) {
      setSelected((values) => values.filter((id) => !pageIds.has(id)));
      return;
    }
    const next = [...new Set([...selected, ...pageIds])];
    if (next.length > 1000) {
      toast.error(t("folders.selectionLimit"));
      return;
    }
    setSelected(next);
  };
  useEffect(() => {
    if (listRef.current) listRef.current.scrollTop = 0;
  }, [debounced, page, sort]);
  const [operation, setOperation] = useState<{
    ids: number[];
    move: boolean;
  } | null>(null);
  const [target, setTarget] = useState("");
  const [busy, setBusy] = useState(false);
  const [operationError, setOperationError] = useState("");
  const begin = (move: boolean) => {
    setOperation({ ids: selected, move });
    setTarget("");
    setOperationError("");
  };
  async function apply() {
    if (!operation || busy || (operation.move && !target)) return;
    setBusy(true);
    setOperationError("");
    try {
      await api.folderBatch(operation.ids, operation.move ? Number(target) : null);
      setOperation(null);
      setSelected([]);
      toast.success(t("common.operationSuccess"));
    } catch (e) {
      setOperationError(explainError(e));
    } finally {
      clearLiveAssets();
      await queryClient.invalidateQueries({ queryKey: queryKeys.folders.library(libraryId) });
      await queryClient.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
      setBusy(false);
    }
  }
  const [creating, setCreating] = useState(false);
  const [renaming, setRenaming] = useState<Folder | null>(null);
  const [deleting, setDeleting] = useState<Folder | null>(null);
  const setDropMode = useImportStore((s) => s.setDropMode);
  const clearLiveAssets = useImportStore((s) => s.clearLiveAssets);
  const progress = useImportStore((s) => s.progress);
  useEffect(() => {
    setDropMode("folder-import");
    return () => setDropMode("import");
  }, [setDropMode]);
  const create = async (name: string) => {
    try {
      await api.folderCreate(name);
      if (sort === "created_desc") setPage(1);
      void queryClient.invalidateQueries({ queryKey: queryKeys.folders.library(libraryId) });
    } catch (e) {
      toast.error(explainError(e));
      throw e;
    }
  };
  const rename = async (name: string) => {
    if (!renaming) return;
    try {
      await api.folderRename(renaming.id, name);
      void queryClient.invalidateQueries({ queryKey: queryKeys.folders.library(libraryId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
    } catch (e) {
      toast.error(explainError(e));
      throw e;
    }
  };
  const remove = async (f: Folder) => {
    try {
      await api.folderDelete(f.id);
      clearLiveAssets();
      setDeleting(null);
      void queryClient.invalidateQueries({ queryKey: queryKeys.folders.library(libraryId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
      toast.success(t("common.operationSuccess"));
    } catch (e) {
      toast.error(explainError(e));
    }
  };
  return {
    t,
    navigate,
    libraryId,
    listRef,
    query,
    setQuery,
    page,
    setPage,
    sort,
    debounced,
    result,
    changeSort,
    sidebar,
    folders,
    isLoading,
    pages,
    togglePin,
    selectMode,
    setSelectMode,
    selected,
    setSelected,
    selectedOpen,
    setSelectedOpen,
    allPageSelected,
    toggleFolder,
    togglePage,
    operation,
    setOperation,
    target,
    setTarget,
    busy,
    operationError,
    setOperationError,
    begin,
    apply,
    creating,
    setCreating,
    renaming,
    setRenaming,
    deleting,
    setDeleting,
    progress,
    create,
    rename,
    remove,
  };
}
