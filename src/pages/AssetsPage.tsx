import AssetSelectionToolbar from "@/components/assets/AssetSelectionToolbar";
import ImportProgressBanner from "@/components/import/ImportProgressBanner";
import { useAssetBrowser } from "@/hooks/useAssetBrowser";
import StorageStatsButton from "@/components/assets/StorageStatsButton";
import SelectedAssetsDialog from "@/components/assets/SelectedAssetsDialog";
import IndexBuildButton from "@/components/indexing/IndexBuildButton";
import { isWindows } from "@/lib/platform";
import { useEffect } from "react";
import { ArrowLeft, FolderOpen, Tag, Filter, Images, Import, Loader2 } from "lucide-react";
import { toast } from "@/components/Toaster";
import { useImportStore } from "@/stores/import";
import { Button } from "@/components/ui/button";
import MasonryGrid from "@/components/assets/MasonryGrid";
import VirtualAssetGrid from "@/components/assets/VirtualAssetGrid";
import AssetCard from "@/components/assets/AssetCard";
import TaskQueueButton from "@/components/tasks/TaskQueueButton";
import GridSizeControl from "@/components/assets/GridSizeControl";
import { AssetFilters } from "@/components/assets/AssetFilters";
import {
  AddTagsDialog,
  AssetDetailPanel,
  AssetSourcePreview,
  MoveToFolderDialog,
  MoveToLibraryDialog,
} from "@/components/assets/AssetDialogs";
export default function AssetsPage() {
  return <AssetBrowserPage filter={{}} emptyHint="assets" importFolder={null} showStorageStats />;
}
/** Assets associated with a tag at /tag/:tagName. */
export function TagAssetsRoute({ tagName }: { tagName: string }) {
  useEffect(() => {
    useImportStore.getState().setDropTags([tagName]);
    return () => useImportStore.getState().setDropTags([]);
  }, [tagName]);
  return (
    <AssetBrowserPage
      filter={{ tags: [tagName] }}
      emptyHint="assets"
      importFolder={null}
      backTo="/tags"
      scopeName={tagName}
    />
  );
}
/** Shared asset browser for all, uncategorized, and folder pages. importFolder is null for
 * uncategorized, numeric for a folder, or undefined to hide import actions.
 */
export function AssetBrowserPage(props: Parameters<typeof useAssetBrowser>[0]) {
  const {
    filter,
    emptyHint,
    importFolder,
    showStorageStats,
    backTo,
    scopeName,
    t,
    location,
    navigate,
    searchParams,
    missingOnly,
    listFilter,
    setListFilter,
    showFilters,
    setShowFilters,
    canFilterFolder,
    activeFilters,
    total,
    isLoading,
    isError,
    isFetchingNextPage,
    fetchNextPage,
    hasNextPage,
    refetch,
    scrollRef,
    selected,
    setSelected,
    selectMode,
    setSelectMode,
    selectedListOpen,
    setSelectedListOpen,
    detailId,
    setDetailId,
    previewAsset,
    previewPagination,
    setPreviewAsset,
    tagsOpen,
    setTagsOpen,
    moveOpen,
    setMoveOpen,
    tagTargets,
    setTagTargets,
    moveTargets,
    setMoveTargets,
    moveLibOpen,
    setMoveLibOpen,
    indexBuildOpen,
    setIndexBuildOpen,
    progress,
    etaSeconds,
    pendingPreview,
    pendingFiles,
    mergedItems,
    selectedAssets,
    allLoadedSelected,
    toggleSelect,
    importViaDialog,
    doTrash,
    empty,
  } = useAssetBrowser(props);
  return (
    <div className="flex h-full min-w-0">
      <div className="relative flex min-w-0 flex-1 flex-col">
        {/* Toolbar. */}
        <header
          data-tauri-drag-region={isWindows ? undefined : true}
          data-page-toolbar
          className="border-b px-6"
        >
          <div className="flex h-16 items-center gap-3">
            {backTo && (
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t("common.back")}
                title={t("common.back")}
                onClick={() => {
                  void navigate(backTo);
                }}
              >
                <ArrowLeft className="h-4 w-4" />
              </Button>
            )}

            <h1
              data-tauri-drag-region={isWindows ? undefined : true}
              title={scopeName}
              className="flex min-w-0 items-center gap-2 text-lg font-semibold"
            >
              {scopeName ? (
                <>
                  {filter.folderId != null ? (
                    <FolderOpen className="h-5 w-5 shrink-0" />
                  ) : (
                    <Tag className="h-5 w-5 shrink-0" />
                  )}
                  <span className="max-w-64 truncate">{scopeName}</span>
                </>
              ) : filter.uncategorized ? (
                t("assets.uncategorized")
              ) : filter.folderId != null ? (
                t("folders.title")
              ) : (
                t("assets.title")
              )}
            </h1>
            {showStorageStats && <StorageStatsButton />}
            {!empty && (
              <span
                data-tauri-drag-region={isWindows ? undefined : true}
                className="text-sm text-muted-foreground"
              >
                {t("assets.total", { count: total })}
              </span>
            )}
            <div data-tauri-drag-region={isWindows ? undefined : true} className="h-8 flex-1" />
            {detailId == null && <GridSizeControl />}
            <Button
              variant={showFilters || activeFilters > 0 ? "default" : "outline"}
              size="sm"
              onClick={() => setShowFilters((value) => !value)}
            >
              <Filter className="h-4 w-4" />
              {t("search.filters")}
              {activeFilters > 0 && ` (${activeFilters})`}
            </Button>
            <TaskQueueButton />
            {importFolder !== undefined && (
              <>
                {detailId == null && (
                  <Button variant="outline" size="sm" onClick={() => void importViaDialog()}>
                    <Import className="h-4 w-4" />
                    {t("assets.import")}
                  </Button>
                )}
              </>
            )}
          </div>
          {showFilters && (
            <div className="pb-3">
              <AssetFilters
                value={listFilter}
                onChange={setListFilter}
                showFolder={canFilterFolder}
                fixedTags={filter.tags ?? []}
                hideFixedTags
              />
            </div>
          )}
        </header>

        {/* Progress occupies its own row and disappears when processing ends. */}
        <div className="relative flex min-h-0 flex-1 flex-col">
          {progress && <ImportProgressBanner progress={progress} etaSeconds={etaSeconds} />}
          <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto p-4">
            {missingOnly && (
              <div className="mb-3 flex items-center justify-between gap-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs">
                <span className="text-amber-600 dark:text-amber-400">
                  {t("assets.missingFilterBanner")}
                </span>
                <button
                  className="rounded p-0.5 text-amber-600 hover:bg-amber-500/20 dark:text-amber-400"
                  onClick={() => {
                    searchParams.delete("missing");
                    const query = searchParams.toString();
                    void navigate(
                      { pathname: location.pathname, search: query ? `?${query}` : "" },
                      { replace: true },
                    );
                  }}
                >
                  {t("assets.missingFilterExit")}
                </button>
              </div>
            )}
            {pendingFiles.length > 0 && (
              <div className="mb-3">
                {pendingPreview.total > pendingFiles.length && (
                  <div className="mb-2 text-xs text-muted-foreground">
                    {t("assets.pendingPreview", {
                      shown: pendingFiles.length,
                      total: pendingPreview.total,
                    })}
                  </div>
                )}
                <MasonryGrid
                  items={pendingFiles}
                  getKey={(file) => file.key}
                  renderItem={(file) => (
                    <div className="overflow-hidden rounded-lg border bg-muted" aria-busy="true">
                      <div className="flex aspect-video items-center justify-center gap-2 text-sm text-muted-foreground">
                        <Loader2 className="h-5 w-5 animate-spin" />
                        {t("stage.pending")}
                      </div>
                      <div
                        className="truncate px-2 py-1.5 text-xs text-muted-foreground"
                        title={file.name}
                      >
                        {file.name}
                      </div>
                    </div>
                  )}
                />
              </div>
            )}
            {isError && !pendingFiles.length && !mergedItems.length ? (
              <div
                role="alert"
                className="flex h-40 flex-col items-center justify-center gap-3 text-center"
              >
                <p className="text-sm text-muted-foreground">{t("assets.loadFailed")}</p>
                <Button variant="outline" size="sm" onClick={() => void refetch()}>
                  {t("common.retry")}
                </Button>
              </div>
            ) : isLoading && !pendingFiles.length && !mergedItems.length ? (
              <div className="flex h-40 items-center justify-center text-muted-foreground">
                <Loader2 className="mr-2 h-5 w-5 animate-spin" />
                {t("common.loading")}
              </div>
            ) : empty ? (
              <div className="flex h-full flex-col items-center justify-center gap-4 py-16 text-center">
                <div className="flex h-20 w-20 items-center justify-center rounded-full bg-muted">
                  {emptyHint === "folders" ? (
                    <FolderOpen className="h-9 w-9 text-muted-foreground" />
                  ) : (
                    <Images className="h-9 w-9 text-muted-foreground" />
                  )}
                </div>
                <div>
                  <div className="font-medium">
                    {t(emptyHint === "folders" ? "folders.emptyAssets" : `${emptyHint}.empty`)}
                  </div>
                  <div className="mt-1 text-sm text-muted-foreground">
                    {t(
                      emptyHint === "folders"
                        ? "folders.emptyAssetsDesc"
                        : `${emptyHint}.emptyDesc`,
                    )}
                  </div>
                </div>
                {importFolder !== undefined && (
                  <Button onClick={() => void importViaDialog()}>
                    <Import className="h-4 w-4" />
                    {t("assets.import")}
                  </Button>
                )}
              </div>
            ) : (
              <VirtualAssetGrid
                items={mergedItems}
                scrollRef={scrollRef}
                onLoadMore={() => {
                  void fetchNextPage({ cancelRefetch: false });
                }}
                hasMore={hasNextPage}
                isLoadingMore={isFetchingNextPage}
                renderItem={(asset) => (
                  <AssetCard
                    asset={asset}
                    selected={selected.has(asset.id)}
                    selectMode={selectMode}
                    onSelect={(id) => {
                      if (!selectMode) setSelectMode(true);
                      toggleSelect(id, true);
                    }}
                    onOpen={(a) => setDetailId(a.id)}
                    onPreview={setPreviewAsset}
                    onAddTags={(a) => {
                      setTagTargets([a]);
                      setTagsOpen(true);
                    }}
                    onMove={(a) => {
                      setMoveTargets([a]);
                      setMoveOpen(true);
                    }}
                    onBatch={(a) => {
                      setSelected(new Set([a.id]));
                      setSelectMode(true);
                    }}
                    onTrash={(a) => {
                      void doTrash([a]);
                    }}
                    onSimilar={(a) => {
                      if (!a.thumbPath) {
                        toast.error(t("search.thumbnailUnavailable"));
                        return;
                      }
                      void navigate("/search", {
                        state: { imageSearchPath: a.thumbPath, backgroundLocation: location },
                      });
                    }}
                  />
                )}
              />
            )}
          </div>
        </div>

        {selectMode && (
          <AssetSelectionToolbar
            count={selected.size}
            hasItems={mergedItems.length > 0}
            allSelected={allLoadedSelected}
            onToggleAll={() =>
              setSelected(
                allLoadedSelected ? new Set() : new Set(mergedItems.map((asset) => asset.id)),
              )
            }
            onShowSelected={() => setSelectedListOpen(true)}
            onAddTags={() => {
              setTagTargets(selectedAssets);
              setTagsOpen(true);
            }}
            onMoveToFolder={() => {
              setMoveTargets(selectedAssets);
              setMoveOpen(true);
            }}
            onBuildIndex={() => setIndexBuildOpen(true)}
            onMoveToLibrary={() => setMoveLibOpen(true)}
            onTrash={() => void doTrash(selectedAssets)}
            onClose={() => {
              setSelected(new Set());
              setSelectMode(false);
            }}
          />
        )}

        {/* Dialogs. */}
        <IndexBuildButton
          scope={{ assetIds: [...selected] }}
          visualOnly={
            selectedAssets.length > 0 &&
            selectedAssets.length === selected.size &&
            selectedAssets.every((asset) => asset.kind !== "video")
          }
          open={indexBuildOpen}
          onOpenChange={setIndexBuildOpen}
        />
        <AddTagsDialog
          assets={tagTargets}
          open={tagsOpen}
          onOpenChange={(open) => {
            setTagsOpen(open);
            if (!open) setTagTargets([]);
          }}
        />
        <MoveToFolderDialog
          assets={moveTargets}
          open={moveOpen}
          onOpenChange={(open) => {
            setMoveOpen(open);
            if (!open) setMoveTargets([]);
          }}
        />
        <MoveToLibraryDialog
          assets={selectedAssets}
          open={moveLibOpen}
          onOpenChange={setMoveLibOpen}
        />
        <SelectedAssetsDialog
          assets={selectedAssets}
          open={selectedListOpen}
          onOpenChange={setSelectedListOpen}
          onRemove={(id) =>
            setSelected((current) => {
              const next = new Set(current);
              next.delete(id);
              return next;
            })
          }
        />
        <AssetSourcePreview
          asset={previewAsset}
          assets={mergedItems}
          pagination={previewPagination}
          open={previewAsset != null}
          onOpenChange={(open) => !open && setPreviewAsset(null)}
        />
      </div>
      <AssetDetailPanel
        assetId={detailId}
        previewAssets={mergedItems}
        previewPagination={previewPagination}
        open={detailId != null}
        onOpenChange={(v) => !v && setDetailId(null)}
        onTrash={(a) => {
          void doTrash([a]);
        }}
      />
    </div>
  );
}
