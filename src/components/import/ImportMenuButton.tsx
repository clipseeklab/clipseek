import { queryKeys } from "@/lib/queryKeys";
/** Quick menu for creating libraries/folders and importing local media, directories, or
 * libraries.
 */
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FileInput, FolderInput, FolderPlus, LibraryBig, Plus } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/components/Toaster";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import CreateLibraryDialog from "@/components/library/CreateLibraryDialog";
import FolderNameDialog from "@/components/folders/FolderNameDialog";
import { useAppStore } from "@/stores/app";
import { useImportStore } from "@/stores/import";
import { explainError } from "@/lib/errors";
export default function ImportMenuButton({ className }: { className?: string }) {
  const { t } = useTranslation();
  const libraryId = useAppStore((state) => state.current?.id);
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { setCurrent, setLibraries } = useAppStore();
  const [createLibraryOpen, setCreateLibraryOpen] = useState(false);
  const [createFolderOpen, setCreateFolderOpen] = useState(false);
  const importAssets = async (folders: boolean) => {
    const picked = await openDialog({
      multiple: true,
      directory: folders,
      title: t(folders ? "assets.importFolder" : "assets.import"),
    });
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
    if (paths.length === 0) return;
    useImportStore.getState().enqueue(paths, null, folders ? "folders" : "paths");
    void navigate("/assets");
  };
  const createFolder = async (name: string) => {
    try {
      await api.folderCreate(name);
      await queryClient.invalidateQueries({ queryKey: queryKeys.folders.library(libraryId) });
    } catch (error) {
      toast.error(explainError(error));
      throw error;
    }
  };
  const importLibrary = async (configFile: boolean) => {
    const picked = await openDialog(
      configFile
        ? {
            title: t("library.importConfig"),
            filters: [{ name: "ClipSeek Library", extensions: ["json"] }],
          }
        : { directory: true, title: t("library.importDirectory") },
    );
    if (typeof picked !== "string") return;
    try {
      const library = await api.libraryImport(picked);
      setCurrent(library);
      setLibraries(await api.libraryList());
      void queryClient.invalidateQueries();
      toast.success(t("library.importSuccess"));
    } catch (error) {
      toast.error(explainError(error));
    }
  };
  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon" className={className} title={t("importMenu.title")}>
            <Plus className="h-5 w-5" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" sideOffset={6} className="w-52">
          <DropdownMenuItem onSelect={() => setCreateLibraryOpen(true)}>
            <Plus className="h-4 w-4" />
            {t("library.create")}
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => setCreateFolderOpen(true)}>
            <FolderPlus className="h-4 w-4" />
            {t("folders.create")}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem onSelect={() => void importAssets(false)}>
            <FileInput className="h-4 w-4" />
            {t("assets.import")}
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => void importAssets(true)}>
            <FolderInput className="h-4 w-4" />
            {t("assets.importFolder")}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem onSelect={() => void importLibrary(false)}>
            <LibraryBig className="h-4 w-4" />
            {t("importMenu.libraryRoot")}
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => void importLibrary(true)}>
            <FileInput className="h-4 w-4" />
            {t("importMenu.libraryConfig")}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <CreateLibraryDialog open={createLibraryOpen} onOpenChange={setCreateLibraryOpen} />
      <FolderNameDialog
        open={createFolderOpen}
        onOpenChange={setCreateFolderOpen}
        mode="create"
        onSubmit={createFolder}
      />
    </>
  );
}
