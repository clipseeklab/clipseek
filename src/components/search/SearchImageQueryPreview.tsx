import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Image as ImageIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
export default function SearchImageQueryPreview({ path }: { path: string }) {
  const { t } = useTranslation();
  const [failedPath, setFailedPath] = useState<string | null>(null);
  const [preview, setPreview] = useState<{
    path: string;
    url: string;
  } | null>(null);
  const [failedUrl, setFailedUrl] = useState<string | null>(null);
  const rasterize = /\.hei[cf]$/i.test(path) || failedPath === path;
  useEffect(() => {
    if (!rasterize) return;
    let disposed = false;
    let url: string | null = null;
    void api
      .searchImagePreview(path)
      .then((bytes) => {
        if (disposed) return;
        url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: "image/png" }));
        setPreview({ path, url });
      })
      .catch(() => {
        // A missing preview must not interrupt the independent image search.
        if (!disposed) setPreview(null);
      });
    return () => {
      disposed = true;
      if (url) URL.revokeObjectURL(url);
    };
  }, [path, rasterize]);
  const src = rasterize ? (preview?.path === path ? preview.url : null) : convertFileSrc(path);
  if (!src || src === failedUrl)
    return <ImageIcon aria-hidden="true" className="h-6 w-6 text-muted-foreground" />;
  return (
    <img
      src={src}
      alt={t("search.imageMode")}
      className="h-6 w-6 rounded object-cover"
      onError={() => (rasterize ? setFailedUrl(src) : setFailedPath(path))}
    />
  );
}
