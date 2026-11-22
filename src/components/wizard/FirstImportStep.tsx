import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import { Images, FolderOpen, ShieldCheck } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useImportStore } from "@/stores/import";
import { toast } from "@/components/Toaster";
import { explainError } from "@/lib/errors";
export default function FirstImportStep({ onDone }: { onDone: (query?: string) => Promise<void> }) {
  const { t, i18n } = useTranslation();
  const [busy, setBusy] = useState(false);
  const zh = i18n.language.startsWith("zh");
  const choose = async (folder: boolean) => {
    setBusy(true);
    try {
      const result = await open({ multiple: true, directory: folder });
      const paths = typeof result === "string" ? [result] : result;
      if (paths?.length) {
        useImportStore.getState().enqueue(paths, null, folder ? "folders" : "paths");
        await onDone();
      }
    } catch (e) {
      toast.error(explainError(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="mx-auto w-full max-w-2xl space-y-6 rounded-xl border bg-card p-8">
      <div className="flex items-center gap-3">
        <Images className="size-6 text-primary" />
        <h2 className="text-xl font-semibold">
          {zh ? "添加你的第一份素材" : "Add your first media"}
        </h2>
      </div>
      <p className="text-sm text-muted-foreground">
        {zh
          ? "选择照片或 32 秒以内的视频，仅引用原文件，不拷贝入库。"
          : "Reference photos or videos up to 32 seconds. Originals stay in place."}
      </p>
      <div className="grid grid-cols-2 gap-3">
        <Button disabled={busy} onClick={() => void choose(false)}>
          <Images className="size-4" />
          {t("assets.import")}
        </Button>
        <Button variant="outline" disabled={busy} onClick={() => void choose(true)}>
          <FolderOpen className="size-4" />
          {t("assets.importFolder")}
        </Button>
      </div>
      <p className="flex items-center gap-2 text-xs text-muted-foreground">
        <ShieldCheck className="size-4" />
        {zh ? "索引和搜索完全在本机运行" : "Indexing and search run on your device"}
      </p>
      <Button variant="ghost" disabled={busy} onClick={() => void onDone()}>
        {zh ? "稍后添加，进入素材库" : "Add later"}
      </Button>
    </section>
  );
}
