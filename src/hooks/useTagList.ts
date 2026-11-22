import { queryKeys } from "@/lib/queryKeys";
import { useAppStore } from "@/stores/app";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { toast } from "@/components/Toaster";
import type { Tag } from "@/types";
import { explainError } from "@/lib/errors";
/** Group ASCII tag initials by uppercase letter; other scripts use the # group. */
function groupKey(name: string): string {
  const c = name[0]?.toUpperCase() ?? "#";
  return /[A-Z]/.test(c) ? c : "#";
}
/** Sort Chinese labels by pinyin. */
const zhCollator = new Intl.Collator("zh-Hans-CN", { sensitivity: "base" });
export function useTagList() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const libraryId = useAppStore((state) => state.current?.id);
  const [keyword, setKeyword] = useState("");
  const [selectMode, setSelectMode] = useState(false);
  const [selected, setSelected] = useState<number[]>([]);
  const [selectedOpen, setSelectedOpen] = useState(false);
  useEffect(() => {
    setKeyword("");
    setSelected([]);
    setSelectedOpen(false);
    setSelectMode(false);
  }, [libraryId]);
  const toggleTag = (id: number) => {
    if (selected.includes(id)) {
      setSelected((values) => values.filter((value) => value !== id));
      return;
    }
    if (selected.length >= 1000) {
      toast.error(t("tags.selectionLimit"));
      return;
    }
    setSelected((values) => [...values, id]);
  };
  const [operation, setOperation] = useState<{
    ids: number[];
    move: boolean;
  } | null>(null);
  const [target, setTarget] = useState("");
  const [operationBusy, setOperationBusy] = useState(false);
  const [operationError, setOperationError] = useState("");
  const beginOperation = (ids: number[], move: boolean) => {
    setOperation({ ids, move });
    setTarget("");
    setOperationError("");
  };
  async function applyOperation() {
    if (!operation || operationBusy || (operation.move && !target)) return;
    setOperationBusy(true);
    setOperationError("");
    try {
      await api.tagBatch(operation.ids, operation.move ? Number(target) : null);
      await queryClient.invalidateQueries({ queryKey: queryKeys.tags.library(libraryId) });
      await queryClient.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
      setOperation(null);
      setSelected([]);
      toast.success(t("common.operationSuccess"));
    } catch (e) {
      setOperationError(explainError(e));
    } finally {
      setOperationBusy(false);
    }
  }
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [addError, setAddError] = useState("");
  const names = [
    ...new Set(
      draft
        .split(/[\n\r,，;；]+/)
        .map((n) => n.trim())
        .filter(Boolean),
    ),
  ];
  async function addMany() {
    if (busy || !names.length || names.length > 1000) return;
    setBusy(true);
    setAddError("");
    try {
      const count = await api.tagCreateMany(names);
      await queryClient.invalidateQueries({ queryKey: queryKeys.tags.library(libraryId) });
      toast.success(t("tags.batchSuccess", { count, skipped: names.length - count }));
      setAdding(false);
      setDraft("");
    } catch (e) {
      setAddError(explainError(e));
    } finally {
      setBusy(false);
    }
  }
  const [deleting, setDeleting] = useState<Tag | null>(null);
  const [editing, setEditing] = useState<Tag | null>(null);
  const [editName, setEditName] = useState("");
  const { data: tags = [], isLoading } = useQuery({
    queryKey: queryKeys.tags.library(libraryId),
    queryFn: api.tagList,
  });
  // Filter and group tags.
  const groups = useMemo(() => {
    const kw = keyword.trim().toLowerCase();
    const filtered = kw ? tags.filter((tag) => tag.name.toLowerCase().includes(kw)) : tags;
    const map = new Map<string, Tag[]>();
    for (const tag of filtered) {
      const key = groupKey(tag.name);
      map.set(key, [...(map.get(key) ?? []), tag]);
    }
    // Use locale ordering for letter groups and pinyin for Chinese labels in #.
    for (const [key, list] of map) {
      list.sort((a, b) =>
        key === "#" ? zhCollator.compare(a.name, b.name) : a.name.localeCompare(b.name),
      );
    }
    return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  }, [tags, keyword]);
  const shownIds = groups.flatMap(([, list]) => list.map((tag) => tag.id));
  const allShownSelected = shownIds.length > 0 && shownIds.every((id) => selected.includes(id));
  const toggleShown = () => {
    if (allShownSelected) {
      const shown = new Set(shownIds);
      setSelected((values) => values.filter((id) => !shown.has(id)));
      return;
    }
    const next = [...new Set([...selected, ...shownIds])];
    if (next.length > 1000) {
      toast.error(t("tags.selectionLimit"));
      return;
    }
    setSelected(next);
  };
  const rename = async () => {
    if (!editing || !editName.trim() || editName.trim() === editing.name) return;
    try {
      await api.tagRename(editing.id, editName.trim());
      void queryClient.invalidateQueries({ queryKey: queryKeys.tags.library(libraryId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
      setEditing(null);
    } catch (e) {
      toast.error(explainError(e));
    }
  };
  const remove = async (tag: Tag) => {
    try {
      await api.tagDelete(tag.id);
      setDeleting(null);
      void queryClient.invalidateQueries({ queryKey: queryKeys.tags.library(libraryId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.assets.library(libraryId) });
      toast.success(t("common.operationSuccess"));
    } catch (e) {
      toast.error(explainError(e));
    }
  };
  const scrollTo = (letter: string) => {
    document
      .getElementById(`tag-group-${letter}`)
      ?.scrollIntoView({ behavior: "smooth", block: "start" });
  };
  return {
    t,
    navigate,
    libraryId,
    keyword,
    setKeyword,
    selectMode,
    setSelectMode,
    selected,
    setSelected,
    selectedOpen,
    setSelectedOpen,
    toggleTag,
    operation,
    setOperation,
    target,
    setTarget,
    operationBusy,
    operationError,
    beginOperation,
    applyOperation,
    adding,
    setAdding,
    draft,
    setDraft,
    busy,
    addError,
    setAddError,
    names,
    addMany,
    deleting,
    setDeleting,
    editing,
    setEditing,
    editName,
    setEditName,
    tags,
    isLoading,
    groups,
    shownIds,
    allShownSelected,
    toggleShown,
    rename,
    remove,
    scrollTo,
  };
}
