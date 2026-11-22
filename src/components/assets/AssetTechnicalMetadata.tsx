import { Fragment } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { explainError } from "@/lib/errors";
import { queryKeys } from "@/lib/queryKeys";
import { formatBitRate, type MetadataTag, type MediaStreamMetadata } from "@/lib/mediaMetadata";
import { useAppStore } from "@/stores/app";
import type { AssetDetail } from "@/types";
import { Button } from "@/components/ui/button";
function Rows({ values }: { values: [string, string | null | undefined][] }) {
  return (
    <dl className="grid min-w-0 grid-cols-[minmax(0,0.42fr)_minmax(0,0.58fr)] gap-x-3 gap-y-1 text-xs">
      {values
        .filter(([, value]) => value != null && value !== "")
        .map(([label, value], index) => (
          <Fragment key={`${label}:${index}`}>
            <dt className="min-w-0 break-words text-muted-foreground [overflow-wrap:anywhere]">
              {label}
            </dt>
            <dd className="min-w-0 whitespace-pre-wrap break-words [overflow-wrap:anywhere]">
              {value}
            </dd>
          </Fragment>
        ))}
    </dl>
  );
}
function Tags({ tags, label }: { tags: MetadataTag[]; label?: string }) {
  const { t } = useTranslation();
  if (!tags.length) return null;
  return (
    <details className="min-w-0">
      <summary className="cursor-pointer text-xs text-muted-foreground">
        {label ?? t("assetMetadata.tags", { count: tags.length })}
      </summary>
      <div className="mt-2 max-h-64 overflow-auto pr-1">
        <Rows values={tags.map((tag) => [tag.name, tag.value])} />
      </div>
    </details>
  );
}
const commonExifFields = [
  "Make",
  "Model",
  "LensMake",
  "LensModel",
  "LensSpecification",
  "DateTimeOriginal",
  "OffsetTimeOriginal",
  "ExposureTime",
  "FNumber",
  "PhotographicSensitivity",
  "FocalLength",
  "FocalLengthIn35mmFilm",
  "ExposureBiasValue",
  "Flash",
  "WhiteBalance",
  "Orientation",
  "GPSLatitude",
  "GPSLongitude",
  "GPSAltitude",
] as const;
function Exif({ tags }: { tags: MetadataTag[] }) {
  const { t } = useTranslation();
  if (!tags.length) return null;
  return (
    <section className="min-w-0 space-y-2 border-t pt-2">
      <h5 className="text-xs font-medium">{t("assetMetadata.exif.title")}</h5>
      <Rows
        values={commonExifFields.map((name) => [
          t(`assetMetadata.exif.fields.${name}`),
          tags.find((tag) => tag.name === name)?.value,
        ])}
      />
      <Tags tags={tags} label={t("assetMetadata.exif.allFields", { count: tags.length })} />
    </section>
  );
}
function Stream({ stream, image }: { stream: MediaStreamMetadata; image: boolean }) {
  const { t } = useTranslation();
  return (
    <section className="min-w-0 space-y-2 border-t pt-2">
      <h5 className="text-xs font-medium">
        {image
          ? t("assetMetadata.image")
          : t(`assetMetadata.streams.${stream.kind}`, { index: stream.index })}
      </h5>
      <Rows
        values={[
          [t("assetMetadata.codec"), stream.codec],
          [t("assetMetadata.profile"), stream.profile],
          [t("assetMetadata.language"), stream.language],
          [
            t("assets.resolution"),
            !image && stream.width && stream.height ? `${stream.width} × ${stream.height}` : null,
          ],
          [
            t("assetMetadata.frameRate"),
            !image && stream.frameRate ? `${stream.frameRate} fps` : null,
          ],
          [t("assetMetadata.bitRate"), stream.bitRate ? formatBitRate(stream.bitRate) : null],
          [t("assetMetadata.pixelFormat"), stream.pixelFormat],
          [
            t("assetMetadata.sampleRate"),
            stream.sampleRate ? `${stream.sampleRate / 1000} kHz` : null,
          ],
          [t("assetMetadata.channels"), stream.channelLayout],
          [t("assetMetadata.sampleFormat"), stream.sampleFormat],
        ]}
      />
      <Tags tags={stream.tags} />
      <details className="min-w-0 text-xs text-muted-foreground">
        <summary className="cursor-pointer">{t("assetMetadata.streamDetails")}</summary>
        <p className="mt-1 whitespace-pre-wrap break-words [overflow-wrap:anywhere]">
          {stream.description}
        </p>
      </details>
    </section>
  );
}
export default function AssetTechnicalMetadata({ detail }: { detail: AssetDetail }) {
  const { t, i18n } = useTranslation();
  const libraryId = useAppStore((state) => state.current?.id);
  const ready = detail.importStage === "done" || detail.importStage.startsWith("error");
  const enabled = !!libraryId && !detail.sourceMissing && ready;
  const query = useQuery({
    queryKey: queryKeys.assetMetadata.asset(libraryId, detail),
    queryFn: () => api.assetMetadata(detail.id),
    enabled,
    staleTime: 60000,
    retry: false,
  });
  if (!enabled) return null;
  const date = (seconds: number | null) =>
    seconds == null
      ? null
      : new Date(seconds * 1000).toLocaleString(i18n.resolvedLanguage === "zh" ? "zh-CN" : "en-US");
  const data = query.data;
  return (
    <section aria-label={t("assetMetadata.title")} className="min-w-0 space-y-2 border-t pt-3">
      <h4 className="text-xs font-medium">{t("assetMetadata.title")}</h4>
      {query.isPending && (
        <p role="status" className="text-xs text-muted-foreground">
          {t("common.loading")}
        </p>
      )}
      {query.isError && (
        <div role="alert" className="space-y-1 text-xs">
          <p className="break-words text-muted-foreground">{explainError(query.error)}</p>
          <Button size="sm" variant="outline" onClick={() => void query.refetch()}>
            {t("common.retry")}
          </Button>
        </div>
      )}
      {data && (
        <>
          <Rows
            values={[
              [t("assetMetadata.format"), data.format],
              [t("assetMetadata.bitRate"), data.bitRate ? formatBitRate(data.bitRate) : null],
              [t("assetMetadata.createdAt"), date(data.createdAt)],
              [t("assetMetadata.modifiedAt"), date(data.modifiedAt)],
            ]}
          />
          <Exif tags={data.exif} />
          <Tags tags={data.tags} />
          {data.streams.map((stream) => (
            <Stream
              key={stream.index}
              stream={stream}
              image={detail.kind === "image" && stream.kind === "video"}
            />
          ))}
        </>
      )}
    </section>
  );
}
