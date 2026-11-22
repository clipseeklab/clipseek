import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { useAppStore } from "@/stores/app";
import { useImportStore } from "@/stores/import";
import { api } from "@/lib/api";
import { queryKeys } from "@/lib/queryKeys";
import { AssetBrowserPage } from "@/pages/AssetsPage";
/** Single-folder asset view with folder-targeted imports. */
export default function SingleFolderPage({ folderId }: { folderId: number }) {
  const { t } = useTranslation();
  const setDropFolderId = useImportStore((s) => s.setDropFolderId);
  const libraryId = useAppStore((state) => state.current?.id);
  const {
    data: folder,
    isPending,
    isError,
  } = useQuery({
    queryKey: queryKeys.folders.detail(libraryId, folderId),
    queryFn: () => api.folderGet(folderId),
    retry: false,
  });
  useEffect(() => {
    setDropFolderId(folderId);
    return () => setDropFolderId(null);
  }, [folderId, setDropFolderId]);
  if (!isPending && (!folder || isError)) {
    return (
      <div className="flex h-full items-center justify-center text-muted-foreground">
        {t("common.error")}
      </div>
    );
  }
  return (
    <AssetBrowserPage
      filter={{ folderId }}
      emptyHint="folders"
      importFolder={folderId}
      backTo="/folders"
      scopeName={folder?.name}
    />
  );
}
