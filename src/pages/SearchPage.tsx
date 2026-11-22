import { useVisualSearch } from "@/hooks/useVisualSearch";
import { SearchResultCard } from "@/components/search/SearchResultCard";
import SearchModePage from "@/components/search/SearchModePage";
import ModelDownloadGate from "@/components/ModelDownloadGate";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import ClearSearchButton from "@/components/search/ClearSearchButton";
import { isWindows } from "@/lib/platform";
/** Search starts with a central text/image query area, then shows the query, structured filters,
 * and paginated results. Supports text and selected/dropped image queries.
 */
import { ArrowLeft, ImageUp, Loader2, Search, X } from "lucide-react";
import { toast } from "@/components/Toaster";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import MasonryGrid from "@/components/assets/MasonryGrid";
import GridSizeControl from "@/components/assets/GridSizeControl";
import { AssetFilters } from "@/components/assets/AssetFilters";
import {
  AddTagsDialog,
  AssetDetailPanel,
  AssetSourcePreview,
  MoveToFolderDialog,
  MoveToLibraryDialog,
} from "@/components/assets/AssetDialogs";
import SearchLanding from "@/components/search/SearchLanding";
import SearchImageQueryPreview from "@/components/search/SearchImageQueryPreview";
function VisualSearchPage(props: Parameters<typeof useVisualSearch>[0]) {
  const {
    modeSwitch,
    active,
    t,
    location,
    scopeId,
    query,
    setQuery,
    searchMode,
    input,
    setInput,
    imageQuery,
    setImageQuery,
    results,
    setResults,
    previewAssets,
    page,
    loading,
    needsModels,
    setNeedsModels,
    hasMore,
    filter,
    setFilter,
    showFilters,
    setShowFilters,
    detailId,
    setDetailId,
    previewTarget,
    setPreviewTarget,
    tagTargets,
    setTagTargets,
    moveTargets,
    setMoveTargets,
    tagsOpen,
    setTagsOpen,
    moveOpen,
    setMoveOpen,
    moveLibOpen,
    setMoveLibOpen,
    selected,
    setSelected,
    selectMode,
    setSelectMode,
    similarHistory,
    resultScrollRef,
    setDropMode,
    runSearch,
    doTextSearch,
    findSimilar,
    goBack,
    pickImage,
    searched,
    scopeBadge,
    activeFilters,
    selectedAssets,
    trashAssets,
  } = useVisualSearch(props);
  return (
    <div className="flex h-full min-w-0">
      <div className="flex min-w-0 flex-1 flex-col">
        {/* Initial centered search. */}
        {!searched && (
          <SearchLanding
            title={t("search.visualTitle")}
            modeSwitch={modeSwitch}
            tools={
              <>
                {scopeBadge}
                {/* Image-query drop area. */}
                <button
                  className="flex h-28 w-full max-w-xl flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed text-sm text-muted-foreground transition-colors hover:border-primary hover:text-foreground"
                  onMouseEnter={() => setDropMode("image-search")}
                  onMouseLeave={() => setDropMode("import")}
                  onClick={() => void pickImage()}
                >
                  <ImageUp className="h-6 w-6" />
                  {t("search.dropImage")}
                </button>
              </>
            }
          >
            <div className="flex w-full max-w-xl gap-2">
              <div className="relative min-w-0 flex-1">
                <Input
                  autoFocus={active}
                  value={input}
                  placeholder={t(
                    scopeId != null ? "search.videoPlaceholder" : "search.placeholder",
                  )}
                  className="h-12 pr-10 text-base"
                  onChange={(e) => setInput(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && doTextSearch(input)}
                />
                {input && <ClearSearchButton floating onClear={() => setInput("")} />}
              </div>
              <Button className="h-12 px-5" onClick={() => doTextSearch(input)}>
                <Search className="h-4 w-4" />
                {t("search.searchBtn")}
              </Button>
            </div>
          </SearchLanding>
        )}

        {/* Search results. */}
        {searched && (
          <>
            <header
              data-tauri-drag-region={isWindows ? undefined : true}
              data-page-toolbar
              className="border-b px-6"
            >
              <div className="flex h-16 items-center gap-2">
                {!imageQuery && modeSwitch}
                {(scopeId != null ||
                  imageQuery ||
                  similarHistory.length > 0 ||
                  location.state?.backgroundLocation) && (
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-9 w-9 shrink-0"
                    aria-label={t("common.back")}
                    title={t("common.back")}
                    onClick={goBack}
                  >
                    <ArrowLeft className="h-4 w-4" />
                  </Button>
                )}
                {imageQuery ? (
                  <div className="flex h-9 items-center gap-2 rounded-md border px-2">
                    <SearchImageQueryPreview key={imageQuery} path={imageQuery} />
                    <span className="text-xs text-muted-foreground">{t("search.imageMode")}</span>
                    <button
                      onClick={() => {
                        setImageQuery(null);
                        setResults([]);
                      }}
                    >
                      <X className="h-3.5 w-3.5" />
                    </button>
                  </div>
                ) : (
                  <div
                    data-usage-search
                    className="flex h-9 min-w-0 flex-1 items-center gap-2 rounded-md border px-3"
                  >
                    <Search className="h-4 w-4 shrink-0 text-muted-foreground" />
                    <input
                      autoFocus={active}
                      placeholder={t(
                        scopeId != null ? "search.videoPlaceholder" : "search.placeholder",
                      )}
                      className="h-full min-w-0 flex-1 bg-transparent text-sm outline-none"
                      value={query}
                      onChange={(e) => setQuery(e.target.value)}
                      onKeyDown={(e) => e.key === "Enter" && doTextSearch(query)}
                    />
                    {query && <ClearSearchButton onClear={() => setQuery("")} />}
                  </div>
                )}
                <Button
                  variant="outline"
                  size="sm"
                  className="h-9"
                  title={t("search.imageMode")}
                  onClick={() => void pickImage()}
                >
                  <ImageUp className="h-4 w-4" />
                </Button>
                <Button
                  variant="default"
                  size="sm"
                  className="h-9"
                  aria-expanded={showFilters}
                  onClick={() => setShowFilters((v) => !v)}
                >
                  {t("search.filters")}
                  {activeFilters > 0 && ` (${activeFilters})`}
                </Button>
                {detailId == null && <GridSizeControl />}
              </div>

              {scopeId != null && <div className="pb-3">{scopeBadge}</div>}

              {/* Filters. */}
              <div className={showFilters ? "pb-3" : "hidden"}>
                <AssetFilters
                  value={filter}
                  onChange={setFilter}
                  properties={[
                    "type",
                    "folders",
                    "tags",
                    "colors",
                    "shape",
                    "resolution",
                    "size",
                    "date",
                    "name",
                    "assetId",
                  ]}
                />
              </div>
            </header>

            {/* Results. */}
            <div ref={resultScrollRef} className="min-h-0 flex-1 overflow-y-auto p-4">
              {!searchMode ? null : loading && results.length === 0 ? (
                <div className="flex h-40 items-center justify-center text-muted-foreground">
                  <Loader2 className="mr-2 h-5 w-5 animate-spin" />
                  {t("common.loading")}
                </div>
              ) : results.length === 0 ? (
                <div className="flex h-full flex-col items-center justify-center gap-2 text-center">
                  <div className="text-4xl">🔍</div>
                  <div className="font-medium">{t("search.noResults")}</div>
                  <div className="text-sm text-muted-foreground">{t("search.noResultsDesc")}</div>
                </div>
              ) : (
                <MasonryGrid
                  items={results}
                  getKey={(hit) => hit.frameId}
                  onLoadMore={() => {
                    void (hasMore && !loading && runSearch(page + 1, true));
                  }}
                  hasMore={hasMore}
                  isLoadingMore={loading}
                  renderItem={(hit) => (
                    <SearchResultCard
                      hit={hit}
                      selected={selected.has(hit.frameId)}
                      selectMode={selectMode}
                      onOpen={(a) => setDetailId(a.id)}
                      onPreview={() => {
                        setPreviewTarget({ asset: hit.asset, startMs: hit.timestampMs });
                      }}
                      actions={{
                        onSimilar: (asset) => {
                          const thumbnail = hit.framePath ?? asset.thumbPath;
                          if (thumbnail) findSimilar(thumbnail);
                          else toast.error(t("search.thumbnailUnavailable"));
                        },
                        onAddTags: (asset) => {
                          setTagTargets([asset]);
                          setTagsOpen(true);
                        },
                        onMove: (asset) => {
                          setMoveTargets([asset]);
                          setMoveOpen(true);
                        },
                        onBatch: () => {
                          setSelected(new Set([hit.frameId]));
                          setSelectMode(true);
                        },
                        onDetails: (asset) => setDetailId(asset.id),
                        onTrash: (asset) => void trashAssets([asset]),
                      }}
                      onSelect={() => {
                        setSelectMode(true);
                        setSelected((current) => {
                          const next = new Set(current);
                          if (next.has(hit.frameId)) next.delete(hit.frameId);
                          else next.add(hit.frameId);
                          return next;
                        });
                      }}
                    />
                  )}
                />
              )}
              {loading && results.length > 0 && (
                <div className="flex items-center justify-center gap-2 py-4 text-sm text-muted-foreground">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  {t("common.loading")}
                </div>
              )}
            </div>
          </>
        )}

        {selectMode && (
          <div className="flex items-center gap-2 border-t bg-card px-6 py-2.5">
            <span className="text-sm font-medium">
              {t("assets.selection", { count: selected.size })}
            </span>
            <div className="flex-1" />
            <Button
              variant="outline"
              size="sm"
              disabled={!selected.size}
              onClick={() => {
                setTagTargets(selectedAssets);
                setTagsOpen(true);
              }}
            >
              {t("assets.addTags")}
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={!selected.size}
              onClick={() => {
                setMoveTargets(selectedAssets);
                setMoveOpen(true);
              }}
            >
              {t("assets.moveToFolder")}
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={!selected.size}
              onClick={() => setMoveLibOpen(true)}
            >
              {t("assets.moveToLibrary")}
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={!selected.size}
              className="text-destructive"
              onClick={() => void trashAssets(selectedAssets)}
            >
              {t("assets.trash")}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                setSelected(new Set());
                setSelectMode(false);
              }}
            >
              {t("common.cancel")}
            </Button>
          </div>
        )}
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
      </div>
      <AssetDetailPanel
        assetId={detailId}
        previewAssets={previewAssets}
        open={detailId != null}
        onOpenChange={(v) => !v && setDetailId(null)}
      />
      <Dialog open={needsModels} onOpenChange={setNeedsModels}>
        <DialogContent className="max-w-xl">
          <DialogHeader>
            <DialogTitle>{t("indexManagement.visual")}</DialogTitle>
          </DialogHeader>
          <ModelDownloadGate
            embedded
            onReady={() => {
              setNeedsModels(false);
              void runSearch(1, false);
            }}
          />
        </DialogContent>
      </Dialog>
      <AssetSourcePreview
        asset={previewTarget?.asset ?? null}
        assets={previewAssets}
        initialTimeMs={previewTarget?.startMs}
        open={previewTarget != null}
        onOpenChange={(open) => !open && setPreviewTarget(null)}
      />
    </div>
  );
}
export default function SearchPage() {
  return <SearchModePage VisualSearch={VisualSearchPage} />;
}
