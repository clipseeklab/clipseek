import { useTranslation } from "react-i18next";
import { forwardRef, useEffect, useState } from "react";
import { streamingPlaybackUrl } from "@/lib/assetPreview";
import { useAppStore } from "@/stores/app";
import { api } from "@/lib/api";
import { toast } from "@/components/Toaster";
import { explainError } from "@/lib/errors";
interface Props extends Omit<React.VideoHTMLAttributes<HTMLVideoElement>, "src"> {
  assetId: number;
  sourcePath: string;
}
export const PlaybackVideo = forwardRef<HTMLVideoElement, Props>(function PlaybackVideo(
  { assetId, sourcePath, onError, ...props },
  ref,
) {
  const { t } = useTranslation();
  const libraryId = useAppStore((s) => s.current?.id);
  const [url, setUrl] = useState<string>();
  useEffect(() => {
    let active = true;
    setUrl(undefined);
    void api
      .assetPlaybackSource(assetId)
      .then((path) => {
        if (active) setUrl(streamingPlaybackUrl(path));
      })
      .catch((error) => {
        if (active) toast.error(explainError(error));
      });
    return () => {
      active = false;
    };
  }, [assetId, sourcePath, libraryId]);
  return (
    <video
      {...props}
      ref={ref}
      src={url}
      onError={(event) => {
        onError?.(event);
        toast.error(t("playback.unsupportedCodec"));
      }}
    />
  );
});
export default PlaybackVideo;
