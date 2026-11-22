import SingleFolderPage from "@/components/folders/SingleFolderPage";
import { FOLDER_SORTS, useFolderList } from "@/hooks/useFolderList";
import SelectedFoldersDialog from "@/components/folders/SelectedFoldersDialog";
import { Input } from "@/components/ui/input";
import IndexBuildButton from "@/components/indexing/IndexBuildButton";
import TargetPicker from "@/components/TargetPicker";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { isWindows } from "@/lib/platform";
/** Folder routes show the folder list or a specific folder's assets. */
import { useParams } from "react-router-dom";
import {
  ArrowDownWideNarrow,
  Check,
  ChevronDown,
  FolderOpen,
  FolderPlus,
  ListChecks,
  ArrowRightLeft,
  Pin,
  PinOff,
  X,
  Loader2,
  PenLine,
  Trash2,
} from "lucide-react";
import FolderNameDialog from "@/components/folders/FolderNameDialog";
import { Button } from "@/components/ui/button";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { explainError } from "@/lib/errors";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
export default function FoldersPage() {
  const { folderId } = useParams();
  if (folderId) return <SingleFolderPage folderId={Number(folderId)} />;
  return <FolderListPage />;
}
/** Folder listing. */
function FolderListPage() {
  const {
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
  } = useFolderList();
  return (
    <div className="flex h-full flex-col">
      <header
        data-tauri-drag-region={isWindows ? undefined : true}
        data-page-toolbar
        className="flex items-center gap-3 border-b px-6 py-3"
      >
        <h1 data-tauri-drag-region={isWindows ? undefined : true} className="text-lg font-semibold">
          {t("folders.title")}
        </h1>
        <Input
          className="ml-3 h-8 w-64 min-w-24"
          aria-label={t("search.searchFolders")}
          placeholder={t("search.searchFolders")}
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            setPage(1);
          }}
        />
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" size="sm" className="shrink-0">
              {result.isFetching ? (
                <Loader2 aria-hidden className="h-4 w-4 animate-spin motion-reduce:animate-none" />
              ) : (
                <ArrowDownWideNarrow aria-hidden className="h-4 w-4" />
              )}
              {t(`folders.sort.${sort}`)}
              <ChevronDown aria-hidden className="h-3.5 w-3.5 text-muted-foreground" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            <DropdownMenuRadioGroup value={sort} onValueChange={changeSort}>
              {FOLDER_SORTS.map((value, index) => (
                <div key={value}>
                  {index === 2 && <DropdownMenuSeparator />}
                  <DropdownMenuRadioItem value={value} className="[&_.lucide-circle]:hidden">
                    {sort === value && <Check aria-hidden className="absolute left-2 size-4" />}
                    {t(`folders.sort.${value}`)}
                  </DropdownMenuRadioItem>
                </div>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
        <div data-tauri-drag-region={isWindows ? undefined : true} className="h-8 flex-1" />
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            setSelectMode(!selectMode);
            setSelected([]);
          }}
        >
          <ListChecks className="h-4 w-4" />
          {t("assets.batchActions")}
        </Button>
        <Button size="sm" onClick={() => setCreating(true)}>
          <FolderPlus className="h-4 w-4" />
          {t("folders.create")}
        </Button>
      </header>

      <div
        ref={listRef}
        aria-busy={result.isFetching}
        className="min-h-0 flex-1 overflow-y-auto p-6"
      >
        {isLoading ? (
          <p role="status" className="p-6 text-sm text-muted-foreground">
            {t("common.loading")}
          </p>
        ) : result.isError ? (
          <div role="alert" className="p-6 text-sm text-destructive">
            {explainError(result.error)}
            <Button variant="ghost" onClick={() => void result.refetch()}>
              {t("common.retry")}
            </Button>
          </div>
        ) : folders.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 text-center">
            <div className="flex h-20 w-20 items-center justify-center rounded-full bg-muted">
              <FolderOpen className="h-9 w-9 text-muted-foreground" />
            </div>
            <div>
              <div className="font-medium">
                {t(debounced ? "search.noResults" : "folders.empty")}
              </div>
              <div className="mt-1 text-sm text-muted-foreground">
                {t(debounced ? "search.noResultsDesc" : "folders.emptyDesc")}
              </div>
              <div className="mt-1 text-sm text-muted-foreground">{t("folders.dropFolder")}</div>
            </div>
            <Button onClick={() => setCreating(true)}>
              <FolderPlus className="h-4 w-4" />
              {t("folders.create")}
            </Button>
          </div>
        ) : (
          <div
            key={result.data?.sort}
            className="grid grid-cols-2 gap-3 animate-in fade-in-0 duration-150 motion-reduce:animate-none md:grid-cols-3 xl:grid-cols-4"
          >
            {folders.map((f) => (
              <div
                key={f.id}
                className="group cursor-pointer rounded-xl border bg-card p-4 transition-colors hover:bg-accent/50"
                onClick={() => {
                  if (selectMode) toggleFolder(f.id);
                  else void navigate(`/folders/${f.id}`);
                }}
              >
                <div className="mb-3 flex h-8 items-center justify-between gap-2">
                  <FolderOpen className="h-8 w-8 shrink-0 text-muted-foreground" />
                  {selectMode && (
                    <Checkbox
                      aria-label={f.name}
                      checked={selected.includes(f.id)}
                      className="pointer-events-none"
                    />
                  )}
                  {!selectMode && (
                    <div
                      className="flex shrink-0 gap-1 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-7 w-7 p-0"
                        title={t(
                          sidebar?.items.some((item) => item.id === f.id)
                            ? "folders.unpin"
                            : "folders.pin",
                        )}
                        onClick={() => void togglePin(f.id)}
                      >
                        {sidebar?.items.some((item) => item.id === f.id) ? (
                          <PinOff className="h-3.5 w-3.5" />
                        ) : (
                          <Pin className="h-3.5 w-3.5" />
                        )}
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7"
                        title={t("folders.moveAssets")}
                        onClick={() => {
                          setOperation({ ids: [f.id], move: true });
                          setTarget("");
                          setOperationError("");
                        }}
                      >
                        <ArrowRightLeft className="h-3.5 w-3.5" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7"
                        title={t("folders.rename")}
                        onClick={() => setRenaming(f)}
                      >
                        <PenLine className="h-3.5 w-3.5" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 text-destructive"
                        title={t("folders.delete")}
                        onClick={() => setDeleting(f)}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </Button>
                    </div>
                  )}
                </div>
                <div className="truncate font-medium">{f.name}</div>
                <div className="mt-0.5 text-xs text-muted-foreground">
                  {t("library.assetCount", { count: f.assetCount })}
                </div>
                {progress?.folderId === f.id && progress.stage !== "finish" && (
                  <div className="mt-3">
                    <div className="mb-1 flex items-center gap-1.5 text-xs text-muted-foreground">
                      <Loader2 className="h-3 w-3 animate-spin" />
                      {t("folders.processing", { done: progress.done, total: progress.total })}
                    </div>
                    <div className="h-1.5 overflow-hidden rounded-full bg-muted">
                      <div
                        className="h-full rounded-full bg-primary transition-[width]"
                        style={{
                          width: `${progress.total ? (progress.done / progress.total) * 100 : 0}%`,
                        }}
                      />
                    </div>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="flex items-center justify-between border-t px-6 py-3 text-xs text-muted-foreground">
        <span>{t("folders.pageInfo", { page, pages, count: result.data?.total ?? 0 })}</span>
        <div className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={page <= 1 || result.isFetching}
            onClick={() => {
              setPage((value) => value - 1);
            }}
          >
            {t("folders.previousPage")}
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={!result.data?.hasMore || result.isFetching}
            onClick={() => {
              setPage((value) => value + 1);
            }}
          >
            {t("folders.nextPage")}
          </Button>
        </div>
      </div>
      {selectMode && (
        <div className="flex flex-wrap items-center gap-2 border-t px-6 py-3">
          <Button
            variant="ghost"
            size="sm"
            aria-label={t("folders.viewSelected", { count: selected.length })}
            disabled={!selected.length}
            onClick={() => setSelectedOpen(true)}
          >
            {t("assets.selection", { count: selected.length })}
            <ListChecks className="h-3.5 w-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={!folders.length || result.isFetching || query !== debounced}
            onClick={togglePage}
          >
            {t(allPageSelected ? "folders.deselectPage" : "folders.selectPage")}
          </Button>
          <div className="flex-1" />
          <IndexBuildButton scope={{ folderIds: selected }} disabled={!selected.length} />
          <Button
            variant="outline"
            size="sm"
            disabled={!selected.length}
            onClick={() => begin(true)}
          >
            <ArrowRightLeft className="h-4 w-4" />
            {t("folders.moveAssets")}
          </Button>
          <Button
            variant="outline"
            size="sm"
            className="text-destructive"
            disabled={!selected.length}
            onClick={() => begin(false)}
          >
            <Trash2 className="h-4 w-4" />
            {t("common.delete")}
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t("common.close")}
            onClick={() => {
              setSelectMode(false);
              setSelected([]);
            }}
          >
            <X />
          </Button>
        </div>
      )}
      {selectedOpen && (
        <SelectedFoldersDialog
          key={libraryId}
          ids={selected}
          onRemove={(id) => setSelected((values) => values.filter((value) => value !== id))}
          onClear={() => setSelected([])}
          onOpenChange={setSelectedOpen}
        />
      )}
      <Dialog
        open={!!operation}
        onOpenChange={(open) => {
          if (!open && !busy) setOperation(null);
        }}
      >
        <DialogContent disableAnimation>
          <DialogHeader>
            <DialogTitle>
              {t(operation?.move ? "folders.moveAssets" : "folders.batchDelete")}
            </DialogTitle>
            <DialogDescription>
              {t(operation?.move ? "folders.moveHelp" : "folders.batchDeleteHelp", {
                count: operation?.ids.length,
              })}
            </DialogDescription>
          </DialogHeader>
          {operation?.move && (
            <TargetPicker
              kind="folders"
              exclude={operation.ids}
              value={target}
              onChange={setTarget}
              disabled={busy}
              label={t("folders.targetFolder")}
            />
          )}
          {operationError && (
            <p role="alert" className="text-sm text-destructive">
              {operationError}
            </p>
          )}
          <DialogFooter>
            <Button variant="outline" disabled={busy} onClick={() => setOperation(null)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant={operation?.move ? "default" : "destructive"}
              disabled={busy || !!(operation?.move && !target)}
              onClick={() => void apply()}
            >
              {t("common.confirm")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      {/* Create. */}
      <FolderNameDialog
        open={creating}
        onOpenChange={setCreating}
        mode="create"
        onSubmit={create}
      />

      {/* Rename. */}
      <FolderNameDialog
        open={!!renaming}
        onOpenChange={(v) => !v && setRenaming(null)}
        mode="rename"
        initialName={renaming?.name ?? ""}
        onSubmit={rename}
      />

      {/* Deletion confirmation. */}
      <AlertDialog open={!!deleting} onOpenChange={(v) => !v && setDeleting(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("folders.delete")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("folders.deleteConfirm", {
                name: deleting?.name,
                count: deleting?.assetCount ?? 0,
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
              onClick={() => {
                if (deleting) void remove(deleting);
              }}
            >
              {t("common.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
