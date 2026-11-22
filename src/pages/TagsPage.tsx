import { useTagList } from "@/hooks/useTagList";
import SelectedTagsDialog from "@/components/tags/SelectedTagsDialog";
import IndexBuildButton from "@/components/indexing/IndexBuildButton";
import TargetPicker from "@/components/TargetPicker";
import { Checkbox } from "@/components/ui/checkbox";
import { isWindows } from "@/lib/platform";
/** Group tags by initial with a navigable letter index; Chinese labels use pinyin in #. Support
 * search, renaming, deletion, and associated asset views.
 */
import {
  Search,
  Plus,
  Images,
  Pencil,
  ListChecks,
  ArrowRightLeft,
  Trash2,
  X,
  Tags as TagsIcon,
} from "lucide-react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
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
const LETTERS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ#".split("");
export default function TagsPage() {
  const {
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
  } = useTagList();
  return (
    <div className="relative flex h-full flex-col">
      <header
        data-tauri-drag-region={isWindows ? undefined : true}
        data-page-toolbar
        className="flex items-center gap-3 border-b px-6 py-3"
      >
        <h1 data-tauri-drag-region={isWindows ? undefined : true} className="text-lg font-semibold">
          {t("tags.title")}
        </h1>
        <span className="text-sm text-muted-foreground">
          {t("tags.total", { count: tags.length })}
        </span>
        <div data-tauri-drag-region={isWindows ? undefined : true} className="h-8 flex-1" />
        <div className="relative w-64">
          <Search className="absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            className="h-9 pl-8"
            placeholder={t("tags.searchPlaceholder")}
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
          />
        </div>
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
        <Button
          size="sm"
          onClick={() => {
            setAdding(true);
            setAddError("");
          }}
        >
          <Plus className="h-4 w-4" />
          {t("tags.batchAdd")}
        </Button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
        {isLoading ? null : tags.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 text-center">
            <div className="flex h-20 w-20 items-center justify-center rounded-full bg-muted">
              <TagsIcon className="h-9 w-9 text-muted-foreground" />
            </div>
            <div>
              <div className="font-medium">{t("tags.empty")}</div>
              <div className="mt-1 text-sm text-muted-foreground">{t("tags.emptyDesc")}</div>
            </div>
          </div>
        ) : groups.length === 0 ? (
          <div className="py-16 text-center text-sm text-muted-foreground">
            {t("search.noResults")}
          </div>
        ) : (
          <div className="pr-10">
            {groups.map(([letter, list]) => (
              <section key={letter} id={`tag-group-${letter}`} className="mb-6 scroll-mt-4">
                <div className="mb-2 text-sm font-semibold text-muted-foreground">{letter}</div>
                <div className="flex flex-wrap gap-2">
                  {list.map((tag) => (
                    <ContextMenu key={tag.id}>
                      <ContextMenuTrigger asChild>
                        <div
                          className="flex cursor-pointer items-center gap-1.5 rounded-full border bg-card px-3 py-1.5 text-sm transition-colors hover:bg-accent"
                          onClick={() =>
                            selectMode
                              ? toggleTag(tag.id)
                              : void navigate(`/tag/${encodeURIComponent(tag.name)}`)
                          }
                        >
                          {selectMode && (
                            <Checkbox
                              aria-label={tag.name}
                              checked={selected.includes(tag.id)}
                              className="pointer-events-none"
                            />
                          )}
                          <span className="max-w-48 truncate">{tag.name}</span>
                          <span className="text-xs tabular-nums text-muted-foreground">
                            {tag.assetCount}
                          </span>
                        </div>
                      </ContextMenuTrigger>
                      <ContextMenuContent>
                        <ContextMenuItem
                          onClick={() => void navigate(`/tag/${encodeURIComponent(tag.name)}`)}
                        >
                          <Images />
                          {t("tags.viewAssets")}
                        </ContextMenuItem>
                        <ContextMenuItem onSelect={() => beginOperation([tag.id], true)}>
                          <ArrowRightLeft />
                          {t("tags.moveAssets")}
                        </ContextMenuItem>
                        <ContextMenuSeparator />
                        <ContextMenuItem
                          onClick={() => {
                            setEditing(tag);
                            setEditName(tag.name);
                          }}
                        >
                          <Pencil />
                          {t("common.rename")}
                        </ContextMenuItem>
                        <ContextMenuItem variant="destructive" onClick={() => setDeleting(tag)}>
                          <Trash2 />
                          {t("common.delete")}
                        </ContextMenuItem>
                      </ContextMenuContent>
                    </ContextMenu>
                  ))}
                </div>
              </section>
            ))}
          </div>
        )}
      </div>

      {selectMode && (
        <div className="flex flex-wrap items-center gap-2 border-t px-6 py-3">
          <Button
            variant="ghost"
            size="sm"
            aria-label={t("tags.viewSelected", { count: selected.length })}
            disabled={!selected.length}
            onClick={() => setSelectedOpen(true)}
          >
            {t("assets.selection", { count: selected.length })}
            <ListChecks className="h-3.5 w-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={!shownIds.length || isLoading}
            onClick={toggleShown}
          >
            {t(allShownSelected ? "tags.deselectShown" : "assets.selectAll")}
          </Button>
          <div className="flex-1" />
          <IndexBuildButton scope={{ tagIds: selected }} disabled={!selected.length} />
          <Button
            variant="outline"
            size="sm"
            disabled={!selected.length}
            onClick={() => beginOperation(selected, true)}
          >
            <ArrowRightLeft className="h-4 w-4" />
            {t("tags.moveAssets")}
          </Button>
          <Button
            variant="outline"
            size="sm"
            className="text-destructive"
            disabled={!selected.length}
            onClick={() => beginOperation(selected, false)}
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
        <SelectedTagsDialog
          key={libraryId}
          ids={selected}
          tags={tags}
          onRemove={(id) => setSelected((values) => values.filter((value) => value !== id))}
          onClear={() => setSelected([])}
          onOpenChange={setSelectedOpen}
        />
      )}
      <Dialog
        open={!!operation}
        onOpenChange={(open) => {
          if (!open && !operationBusy) setOperation(null);
        }}
      >
        <DialogContent disableAnimation>
          <DialogHeader>
            <DialogTitle>{t(operation?.move ? "tags.moveAssets" : "tags.batchDelete")}</DialogTitle>
            <DialogDescription>
              {t(operation?.move ? "tags.moveHelp" : "tags.batchDeleteHelp", {
                count: operation?.ids.length,
              })}
            </DialogDescription>
          </DialogHeader>
          {operation?.move && (
            <TargetPicker
              kind="tags"
              exclude={operation.ids}
              value={target}
              onChange={setTarget}
              disabled={operationBusy}
              label={t("tags.targetTag")}
            />
          )}
          {operationError && (
            <p role="alert" className="text-sm text-destructive">
              {operationError}
            </p>
          )}
          <DialogFooter>
            <Button variant="outline" disabled={operationBusy} onClick={() => setOperation(null)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant={operation?.move ? "default" : "destructive"}
              disabled={operationBusy || !!(operation?.move && !target)}
              onClick={() => void applyOperation()}
            >
              {t("common.confirm")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      {/* Letter navigation. */}
      {tags.length > 0 && (
        <div className="absolute right-1 top-1/2 flex max-h-[70%] -translate-y-1/2 flex-col items-center gap-0 overflow-y-auto py-1 text-[10px] leading-tight">
          {LETTERS.filter((l) => l === "#" || groups.some(([g]) => g === l)).map((l) => (
            <button
              key={l}
              className="w-5 rounded px-0.5 py-px text-muted-foreground hover:bg-accent hover:text-foreground"
              onClick={() => scrollTo(l)}
            >
              {l}
            </button>
          ))}
        </div>
      )}

      <Dialog
        open={adding}
        onOpenChange={(value) => {
          if (!busy) setAdding(value);
        }}
      >
        <DialogContent disableAnimation>
          <DialogHeader>
            <DialogTitle>{t("tags.batchAdd")}</DialogTitle>
            <DialogDescription>{t("tags.batchHelp")}</DialogDescription>
          </DialogHeader>
          <textarea
            autoFocus
            aria-label={t("tags.batchNames")}
            disabled={busy}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            className="min-h-40 w-full resize-y rounded-md border bg-background p-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
            placeholder={t("tags.batchPlaceholder")}
          />
          <p className="text-xs text-muted-foreground">
            {t("tags.batchCount", { count: names.length })}
          </p>
          {addError && (
            <p role="alert" className="text-sm text-destructive">
              {addError}
            </p>
          )}
          <DialogFooter>
            <Button variant="outline" disabled={busy} onClick={() => setAdding(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              disabled={busy || !names.length || names.length > 1000}
              onClick={() => void addMany()}
            >
              {t(busy ? "common.loading" : "common.confirm")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <AlertDialog open={!!deleting} onOpenChange={(v) => !v && setDeleting(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("tags.delete")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("tags.deleteConfirm", { name: deleting?.name, count: deleting?.assetCount ?? 0 })}
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

      <Dialog open={!!editing} onOpenChange={(open) => !open && setEditing(null)}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t("tags.renameTitle")}</DialogTitle>
          </DialogHeader>
          <Input
            autoFocus
            value={editName}
            onChange={(event) => setEditName(event.target.value)}
            onKeyDown={(event) => event.key === "Enter" && void rename()}
          />
          <div className="flex justify-end">
            <Button
              size="sm"
              disabled={!editName.trim() || editName.trim() === editing?.name}
              onClick={() => void rename()}
            >
              {t("common.confirm")}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
