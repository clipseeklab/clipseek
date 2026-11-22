import { useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Volume2, VolumeX } from "lucide-react";
import { streamingPlaybackUrl } from "@/lib/assetPreview";
import { api } from "@/lib/api";
// React owns the card hosts, but not this media element. Keep it across cards.
let shared: ReturnType<typeof createPlayer> | undefined;
function createPlayer() {
  const video = document.createElement("video");
  video.dataset.hoverPlayer = "shared";
  video.className = "h-full w-full object-cover pointer-events-none";
  video.muted = video.defaultMuted = true;
  video.setAttribute("muted", "");
  video.playsInline = true;
  video.preload = "auto";
  const parking = document.createElement("div");
  parking.hidden = true;
  document.body.append(parking);
  parking.append(video);
  let owner: HTMLElement | null = null;
  let generation = 0;
  let target = 0;
  let positioned = false;
  let pending = false;
  let loggedGeneration = -1;
  let hostRequested = false;
  let requestToken = "";
  function play() {
    if (!owner || !positioned || video.seeking || pending || !video.paused) return;
    const request = generation;
    pending = true;
    void video
      .play()
      .catch((error: DOMException) => {
        if (request !== generation || !owner || error.name === "AbortError") return;
        video.dataset.playbackError = error.name;
        if (error.name === "NotAllowedError" && !hostRequested) {
          hostRequested = true;
          void api.hoverVideoPlay(requestToken).catch((hostError) => {
            if (request === generation && owner) {
              void api.frontendError("hover-video", String(hostError)).catch(() => {});
            }
          });
          return;
        }
        if (loggedGeneration !== request) {
          loggedGeneration = request;
          void api
            .frontendError(
              "hover-video",
              error.name + ": readyState=" + video.readyState + ", muted=" + video.muted,
            )
            .catch(() => {});
        }
      })
      .finally(() => {
        if (request === generation) pending = false;
      });
  }
  function position() {
    if (!owner || video.readyState < 1 || positioned) return;
    positioned = true;
    const seconds = Number.isFinite(video.duration)
      ? Math.min(target, Math.max(0, video.duration - 0.05))
      : target;
    if (Math.abs(video.currentTime - seconds) > 0.05) video.currentTime = seconds;
    play();
  }
  video.addEventListener("loadedmetadata", position);
  video.addEventListener("canplay", () => {
    position();
    play();
  });
  video.addEventListener("seeked", play);
  video.addEventListener("host-hover-play", (event) => {
    if ((event as CustomEvent<string>).detail !== requestToken || !owner) return;
    pending = false;
    play();
  });
  video.addEventListener("playing", () => {
    delete video.dataset.playbackError;
    video.style.opacity = "1";
  });
  return {
    attach(host: HTMLElement, path: string, startMs: number, poster?: string) {
      generation += 1;
      requestToken = crypto.randomUUID();
      hostRequested = false;
      pending = false;
      video.pause();
      video.muted = true;
      owner = host;
      target = Math.max(0, startMs / 1000);
      positioned = false;
      delete video.dataset.playbackError;
      video.poster = poster ?? "";
      video.style.opacity = "0";
      host.append(video);
      const fileSrc = streamingPlaybackUrl(path);
      // A compatibility repair replaces the file in place. Force WebKit to discard
      // any byte ranges cached for the old container at the same path.
      const src = `${fileSrc}${fileSrc.includes("?") ? "&" : "?"}clipseek-hover=${generation}`;
      if (video.getAttribute("src") !== src) {
        video.src = src;
        video.load();
      } else position();
    },
    setMuted(host: HTMLElement, muted: boolean) {
      if (owner !== host) return;
      video.muted = muted;
      // The click supplies user activation; keep the current playback position.
      hostRequested = false;
      play();
    },
    detach(host: HTMLElement) {
      if (owner !== host) return;
      generation += 1;
      pending = false;
      owner = null;
      video.pause();
      video.muted = true;
      video.removeAttribute("src");
      video.load();
      parking.append(video);
    },
    dispose() {
      video.pause();
      video.removeAttribute("src");
      video.load();
      video.remove();
      parking.remove();
    },
  };
}
if (import.meta.hot)
  import.meta.hot.dispose(() => {
    shared?.dispose();
    shared = undefined;
  });
interface Props {
  active: boolean;
  assetId: number;
  videoPath: string;
  startMs?: number;
  poster?: string;
  className?: string;
}
export default function HoverVideoPreview({
  active,
  assetId,
  videoPath,
  startMs = 0,
  poster,
  className,
}: Props) {
  const { t } = useTranslation();
  const [muted, setMuted] = useState(true);
  const host = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    setMuted(true);
    if (!active || !host.current) return;
    const element = host.current;
    const player = (shared ??= createPlayer());
    let cancelled = false;
    api
      .assetPlaybackSource(assetId)
      .then((path) => {
        if (!cancelled) player.attach(element, path, startMs, poster);
      })
      // Keep the poster on failure; never fall back to the RAW original.
      .catch((error) => {
        if (!cancelled)
          void api.frontendError("hover-video-source", String(error)).catch(() => undefined);
      });
    return () => {
      cancelled = true;
      player.detach(element);
    };
  }, [active, assetId, videoPath, startMs, poster]);
  return (
    <div className={className} style={{ pointerEvents: "none" }}>
      <div ref={host} className="absolute inset-0" />
      {active && (
        <button
          type="button"
          className="pointer-events-auto absolute bottom-1.5 left-1.5 z-10 flex h-7 w-7 items-center justify-center rounded bg-black/65 text-white hover:bg-black/85 focus-visible:outline focus-visible:outline-2 focus-visible:outline-white"
          title={t(muted ? "assets.unmute" : "assets.mute")}
          aria-label={t(muted ? "assets.unmute" : "assets.mute")}
          aria-pressed={!muted}
          onPointerDown={(event) => event.stopPropagation()}
          onDoubleClick={(event) => event.stopPropagation()}
          onClick={(event) => {
            event.stopPropagation();
            if (!host.current) return;
            const next = !muted;
            shared?.setMuted(host.current, next);
            setMuted(next);
          }}
        >
          {muted ? <VolumeX className="h-4 w-4" /> : <Volume2 className="h-4 w-4" />}
        </button>
      )}
    </div>
  );
}
