import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLocation, useNavigate } from "react-router-dom";
import {
  ChevronLeft,
  ChevronRight,
  FolderInput,
  HardDrive,
  Image,
  Lightbulb,
  Search,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import SettingsDialog from "@/components/settings/SettingsDialog";
import { loadTipsState, saveTipsState, usageTips } from "@/lib/usageTips";
import { toast } from "@/components/Toaster";
import { explainError } from "@/lib/errors";
const icons = {
  visual: Search,
  organize: FolderInput,
  reference: HardDrive,
  image: Image,
};
export default function UsageTips() {
  const { t } = useTranslation();
  const location = useLocation();
  const navigate = useNavigate();
  const [state, setState] = useState(loadTipsState);
  const [index, setIndex] = useState(() =>
    Math.max(
      0,
      usageTips.findIndex((tip) => tip.id === state.lastId),
    ),
  );
  const [open, setOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const searchAfterClose = useRef(false);
  const tip = usageTips[index];
  const Icon = icons[tip.id];
  const unread = usageTips.some((item) => state.announced[item.id] !== item.revision);
  const focusSearch = () => {
    const input = Array.from(
      document.querySelectorAll<HTMLInputElement>("[data-usage-search] input"),
    ).find((item) => !item.closest("[hidden], [inert]"));
    input?.focus();
    return !!input;
  };
  useEffect(() => {
    if (!searchAfterClose.current) return;
    const frame = requestAnimationFrame(() => {
      if (focusSearch()) searchAfterClose.current = false;
    });
    return () => cancelAnimationFrame(frame);
  }, [location.pathname, location.search]);
  // Wait for recovery/update dialogs to close before showing the first tip.
  useEffect(() => {
    if (!state.pending || state.presented) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const check = () => {
      if (document.querySelector('[role="dialog"][data-state="open"], [role="alertdialog"]')) {
        clearTimeout(timer);
        timer = undefined;
        return;
      }
      // Thumbnail/index updates must not indefinitely postpone the first tip.
      if (timer !== undefined) return;
      timer = setTimeout(() => {
        timer = undefined;
        if (!document.querySelector('[role="dialog"][data-state="open"], [role="alertdialog"]'))
          setOpen(true);
      }, 500);
    };
    const observer = new MutationObserver(check);
    observer.observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["data-state"],
    });
    check();
    return () => {
      clearTimeout(timer);
      observer.disconnect();
    };
  }, [state.pending, state.presented]);
  useEffect(() => {
    if (!open) return;
    setState((previous) => {
      const next = {
        ...previous,
        announced: Object.fromEntries(usageTips.map((item) => [item.id, item.revision])),
        pending: false,
        presented: true,
        lastId: tip.id,
        seen: { ...previous.seen, [tip.id]: tip.revision },
      };
      saveTipsState(next);
      return next;
    });
  }, [open, tip.id, tip.revision]);
  const openManually = (value: boolean) => {
    if (value) {
      const firstUnread = usageTips.findIndex((item) => state.seen[item.id] !== item.revision);
      if (firstUnread >= 0) setIndex(firstUnread);
    }
    setOpen(value);
  };
  return (
    <>
      <div className="shrink-0" data-no-drag>
        <Popover open={open} onOpenChange={openManually}>
          <PopoverTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="relative size-9 text-muted-foreground hover:text-foreground"
              aria-label={t("usageTips.entry")}
              title={t("usageTips.entry")}
            >
              <Lightbulb className="size-4" aria-hidden="true" />
              {unread && (
                <span
                  className="absolute right-1 top-1 size-1.5 rounded-full bg-primary"
                  role="img"
                  aria-label={t("usageTips.unread")}
                />
              )}
            </Button>
          </PopoverTrigger>
          <PopoverContent
            side="top"
            align="start"
            sideOffset={10}
            className="w-[360px] max-w-[calc(100vw-32px)] rounded-xl p-0 shadow-lg"
            aria-label={t("usageTips.entry")}
            onOpenAutoFocus={(event) => {
              event.preventDefault();
            }}
            onCloseAutoFocus={(event) => {
              if (searchAfterClose.current) {
                event.preventDefault();
                if (focusSearch()) searchAfterClose.current = false;
                else
                  void Promise.resolve(navigate("/search")).catch((error) => {
                    searchAfterClose.current = false;
                    toast.error(explainError(error));
                  });
              }
            }}
          >
            <div className="flex items-center justify-between border-b px-5 py-3">
              <span className="flex items-center gap-2 text-xs text-muted-foreground">
                <Lightbulb className="size-3.5" />
                {t("usageTips.entry")}
              </span>
              <Button
                variant="ghost"
                size="icon"
                className="size-7"
                aria-label={t("usageTips.close")}
                onClick={() => setOpen(false)}
              >
                <X className="size-4" />
              </Button>
            </div>
            <div className="min-h-60 px-5 py-5" aria-live="polite" aria-atomic="true">
              <div className="mb-4 flex size-10 items-center justify-center rounded-xl bg-primary/10 text-primary">
                <Icon className="size-5" />
              </div>
              <h2 className="mb-2 text-base font-medium">{t(`usageTips.${tip.id}Title`)}</h2>
              <p className="text-sm leading-6 text-muted-foreground">
                {t(`usageTips.${tip.id}Body`)}
              </p>
              {"action" in tip && (
                <Button
                  variant="link"
                  className="mt-2 h-auto p-0 text-sm"
                  onClick={() => {
                    if (tip.action === "settings") setSettingsOpen(true);
                    else searchAfterClose.current = true;
                    setOpen(false);
                  }}
                >
                  {t(tip.action === "settings" ? "usageTips.settings" : "usageTips.trySearch")}
                </Button>
              )}
            </div>
            <div className="flex items-center justify-between border-t px-4 py-3">
              <Button
                variant="ghost"
                size="icon"
                className="size-8"
                aria-label={t("usageTips.previous")}
                disabled={index === 0}
                onClick={() => setIndex(index - 1)}
              >
                <ChevronLeft className="size-4" />
              </Button>
              <span
                className="text-xs tabular-nums text-muted-foreground"
                aria-label={t("usageTips.position", {
                  current: index + 1,
                  total: usageTips.length,
                })}
              >
                {index + 1} / {usageTips.length}
              </span>
              <Button
                variant="ghost"
                size="icon"
                className="size-8"
                aria-label={t("usageTips.next")}
                disabled={index === usageTips.length - 1}
                onClick={() => setIndex(index + 1)}
              >
                <ChevronRight className="size-4" />
              </Button>
            </div>
          </PopoverContent>
        </Popover>
      </div>
      {settingsOpen && (
        <SettingsDialog open onOpenChange={setSettingsOpen} initialSection="settings-storage" />
      )}
    </>
  );
}
