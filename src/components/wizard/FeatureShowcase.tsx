import { useEffect, useState } from "react";
import { FolderOpen, Pause, Play, Search, ShieldCheck, Sparkles, Tags } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import "./onboarding.css";
const FEATURES = ["visual", "organize"] as const;
/** Lightweight, local UI demonstrations: no media downloads compete with the models. */
export default function FeatureShowcase() {
  const { t } = useTranslation();
  const [slide, setSlide] = useState(0);
  const [paused, setPaused] = useState(false);
  const [interacting, setInteracting] = useState(false);
  const [reducedMotion, setReducedMotion] = useState(
    () => window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );
  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setReducedMotion(media.matches);
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  useEffect(() => {
    if (paused || interacting || reducedMotion) return;
    const timer = window.setInterval(
      () => setSlide((value) => (value + 1) % FEATURES.length),
      7000,
    );
    return () => window.clearInterval(timer);
  }, [paused, interacting, reducedMotion, slide]);
  const feature = FEATURES[slide];
  return (
    <section
      className="onboarding-showcase min-w-0"
      aria-roledescription={t("modelDownload.showcase.carousel")}
      aria-label={t("modelDownload.showcase.label")}
      onMouseEnter={() => setInteracting(true)}
      onMouseLeave={() => setInteracting(false)}
      onFocusCapture={() => setInteracting(true)}
      onBlurCapture={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) setInteracting(false);
      }}
      data-paused={paused || interacting || reducedMotion}
    >
      <div className="mb-6 flex items-center gap-2 text-sm font-semibold">
        <Sparkles className="size-4" />
        ClipSeek Open
        <span className="ml-auto text-xs font-normal text-muted-foreground">
          {t("modelDownload.showcase.preview")}
        </span>
      </div>
      <div
        key={feature}
        className="onboarding-slide"
        role="group"
        aria-roledescription={t("modelDownload.showcase.slide")}
        aria-label={`${slide + 1} / ${FEATURES.length}`}
      >
        <div
          className="onboarding-demo rounded-xl border bg-background p-4 shadow-sm"
          aria-hidden="true"
        >
          {feature === "visual" && (
            <>
              <div className="mb-4 flex items-center gap-2 rounded-lg border bg-muted/40 px-3 py-2.5 text-sm">
                <Search className="size-4 shrink-0 text-muted-foreground" />
                <span className="onboarding-query">{t("modelDownload.showcase.visualQuery")}</span>
                <span className="ml-auto text-xs text-muted-foreground">↵</span>
              </div>
              <div className="grid grid-cols-3 gap-2">
                {[0, 1, 2, 3, 4, 5].map((item) => (
                  <div
                    key={item}
                    className={cn(
                      "onboarding-result overflow-hidden rounded-lg",
                      item === 0 && "ring-2 ring-orange-400 ring-offset-2 ring-offset-background",
                    )}
                    style={{ animationDelay: `${item * 110 + 400}ms` }}
                  >
                    <Landscape variant={item} />
                  </div>
                ))}
              </div>
              <div className="mt-3 flex items-center gap-1.5 text-xs text-muted-foreground">
                <Sparkles className="size-3" />
                {t("modelDownload.showcase.visualResult")}
              </div>
            </>
          )}
          {feature === "organize" && (
            <>
              <div className="mb-4 flex items-center gap-2 border-b pb-3 text-sm">
                <FolderOpen className="size-4" />
                {t("modelDownload.showcase.library")}
              </div>
              <div className="grid grid-cols-3 gap-2">
                {[3, 1, 4].map((item) => (
                  <div key={item} className="onboarding-result overflow-hidden rounded-lg">
                    <Landscape variant={item} />
                  </div>
                ))}
              </div>
              <div className="mt-5 flex flex-wrap gap-2">
                {["travel", "inspiration", "favorite"].map((tag, index) => (
                  <span
                    key={tag}
                    className="onboarding-result flex items-center gap-1.5 rounded-md border bg-muted/50 px-2.5 py-1.5 text-xs"
                    style={{ animationDelay: `${index * 250 + 400}ms` }}
                  >
                    <Tags className="size-3" />
                    {t(`modelDownload.showcase.${tag}`)}
                  </span>
                ))}
              </div>
              <div className="mt-4 flex items-center gap-2 text-xs text-muted-foreground">
                <ShieldCheck className="size-4" />
                {t("modelDownload.showcase.local")}
              </div>
            </>
          )}
        </div>
        <h2 className="mt-6 text-2xl font-semibold tracking-tight">
          {t(`modelDownload.showcase.${feature}Title`)}
        </h2>
        <p className="mt-2 min-h-10 text-sm leading-relaxed text-muted-foreground">
          {t(`modelDownload.showcase.${feature}Description`)}
        </p>
      </div>
      <div className="mt-5 flex items-center gap-2">
        {FEATURES.map((item, index) => (
          <button
            key={item}
            type="button"
            aria-label={t(`modelDownload.showcase.${item}Title`)}
            aria-current={index === slide ? "true" : undefined}
            onClick={() => {
              setSlide(index);
              setPaused(true);
            }}
            className="group flex h-8 min-w-8 items-center justify-center rounded-md focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
          >
            <span
              className={cn(
                "h-1.5 rounded-full transition-all motion-reduce:transition-none",
                index === slide ? "w-7 bg-primary" : "w-2 bg-primary/20 group-hover:bg-primary/50",
              )}
            />
          </button>
        ))}
        {!reducedMotion && (
          <button
            type="button"
            className="ml-auto flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-muted focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
            aria-label={t(
              paused ? "modelDownload.showcase.resume" : "modelDownload.showcase.pause",
            )}
            onClick={() => setPaused((value) => !value)}
          >
            {paused ? <Play className="size-3.5" /> : <Pause className="size-3.5" />}
          </button>
        )}
      </div>
    </section>
  );
}
function Landscape({ variant }: { variant: number }) {
  const palettes = [
    ["#f8c594", "#dc896c", "#536c80"],
    ["#edb7a1", "#bb7e86", "#425777"],
    ["#b9d6dd", "#859ea2", "#466c72"],
    ["#e7cdb0", "#b79d88", "#6e7973"],
    ["#cfcee7", "#9b96b3", "#5d6b82"],
    ["#f2cfb0", "#c49c8a", "#68818a"],
  ];
  const [sky, hill, sea] = palettes[variant];
  return (
    <svg viewBox="0 0 160 106" className="block w-full" focusable="false">
      <rect width="160" height="106" fill={sky} />
      <circle cx={105 - variant * 8} cy="30" r="13" fill="#fff4d1" />
      <path d={`M0 72 L${25 + variant * 5} 39 L77 77 L120 52 L160 70 V106 H0Z`} fill={hill} />
      <path d="M0 76 Q40 68 80 78 T160 75 V106 H0Z" fill={sea} />
      <path
        d="M50 90 Q80 85 110 90 M65 96 H95"
        fill="none"
        stroke="#ffffff"
        strokeOpacity=".35"
        strokeWidth="2"
      />
    </svg>
  );
}
