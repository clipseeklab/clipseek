import { isWindows } from "@/lib/platform";
/** Resumable three-step onboarding: model, library, and first import. */
import { startTransition, useEffect, useEffectEvent, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Check, Languages } from "lucide-react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { detectSystemLanguage } from "@/i18n";
import type { OnboardingState } from "@/types";
import { toast } from "@/components/Toaster";
import ModelDownloadGate from "@/components/ModelDownloadGate";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/stores/app";
import { explainError } from "@/lib/errors";
import { useNavigate } from "react-router-dom";
import { prepareFirstUseTips } from "@/lib/usageTips";
import LibrarySetupStep from "./LibrarySetupStep";
import FirstImportStep from "./FirstImportStep";
export default function FirstRunWizard({
  initialState,
  modelsReady,
  onModelsReady,
  onStateChange,
  onComplete,
}: {
  initialState: OnboardingState;
  modelsReady: boolean;
  onModelsReady: () => void;
  onStateChange: (state: OnboardingState) => void;
  onComplete: () => void;
}) {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const [transitionError, setTransitionError] = useState("");
  const { settings, setSettings } = useAppStore();
  const [step, setStep] = useState<OnboardingState["currentStep"]>(initialState.currentStep);
  const visibleStep = modelsReady ? step : 1;
  const changeLanguage = async (language: string) => {
    try {
      const next = await api.settingsSet({ language });
      setSettings(next);
      await i18n.changeLanguage(language === "auto" ? detectSystemLanguage() : language);
    } catch (cause) {
      toast.error(explainError(cause));
    }
  };
  const moveTo = async (next: OnboardingState["currentStep"]) => {
    const state = await api.onboardingSet(next, false);
    setStep(state.currentStep);
    onStateChange(state);
  };
  const finish = async (query?: string) => {
    const state = await api.onboardingSet(3, true);
    prepareFirstUseTips({ restart: true });
    // Router updates run in a React transition. Finish the wizard in that same
    // transition so the main layout never mounts at the old root route and
    // redirects to /assets before the requested search location is committed.
    startTransition(() => {
      void Promise.resolve(
        navigate(query ? `/search?q=${encodeURIComponent(query)}` : "/assets", { replace: true }),
      ).catch((cause) => toast.error(explainError(cause)));
      onStateChange(state);
      onComplete();
    });
  };
  const advanceAfterModels = useEffectEvent(() => moveTo(2));
  useEffect(() => {
    if (step === 1 && modelsReady)
      void advanceAfterModels().catch((cause) => setTransitionError(explainError(cause)));
  }, [modelsReady, step]);
  return (
    <div className="flex h-full flex-col bg-background">
      <header
        data-tauri-drag-region={isWindows ? undefined : true}
        className="flex flex-wrap items-center gap-4 border-b px-8 py-5"
        onMouseDown={(event) => {
          if (
            !isWindows &&
            event.button === 0 &&
            !(event.target as Element).closest("[data-no-drag]")
          ) {
            void getCurrentWindow().startDragging();
          }
        }}
      >
        <div
          data-tauri-drag-region={isWindows ? undefined : true}
          className="mx-auto flex w-full max-w-3xl flex-1 items-center justify-between gap-3"
        >
          {[1, 2, 3].map((number) => (
            <div
              data-tauri-drag-region={isWindows ? undefined : true}
              key={number}
              className="flex min-w-0 flex-1 items-center gap-2 last:flex-none"
            >
              <div
                data-tauri-drag-region={isWindows ? undefined : true}
                className={cn(
                  "flex size-7 shrink-0 items-center justify-center rounded-full border text-xs font-semibold",
                  number < visibleStep && "border-primary bg-primary text-primary-foreground",
                  number === visibleStep && "border-primary text-primary",
                  number > visibleStep && "text-muted-foreground",
                )}
              >
                {number < visibleStep ? <Check className="size-4" /> : number}
              </div>
              <span
                data-tauri-drag-region={isWindows ? undefined : true}
                className={cn(
                  "hidden text-xs sm:block",
                  number === visibleStep ? "font-medium" : "text-muted-foreground",
                )}
              >
                {t(`wizard.steps.${number}`)}
              </span>
              {number < 3 && (
                <div
                  data-tauri-drag-region={isWindows ? undefined : true}
                  className="h-px min-w-4 flex-1 bg-border"
                />
              )}
            </div>
          ))}
        </div>
        <div data-no-drag className="ml-auto flex items-center gap-2">
          <Languages className="size-4 text-muted-foreground" />
          <Select value={settings.language} onValueChange={(value) => void changeLanguage(value)}>
            <SelectTrigger className="w-36" aria-label={t("settings.language")}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="auto">{t("settings.autoLang")}</SelectItem>
              <SelectItem value="zh">中文</SelectItem>
              <SelectItem value="en">English</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </header>
      <main className="min-h-0 flex-1 overflow-y-auto p-5 sm:p-8">
        <div className="flex min-h-full flex-col items-center justify-center gap-4">
          {transitionError && (
            <div
              role="alert"
              className="rounded-lg border border-destructive/30 p-3 text-sm text-destructive"
            >
              <p>{transitionError}</p>
              <Button
                variant="outline"
                size="sm"
                className="mt-2"
                onClick={() => {
                  setTransitionError("");
                  void moveTo(2).catch((cause) => setTransitionError(explainError(cause)));
                }}
              >
                {t("common.retry")}
              </Button>
            </div>
          )}
          {(!modelsReady || step === 1) && (
            <ModelDownloadGate embedded showcase onReady={onModelsReady} />
          )}
          {modelsReady && step === 2 && (
            <LibrarySetupStep onCreated={() => moveTo(3)} onImported={finish} />
          )}
          {modelsReady && step === 3 && <FirstImportStep onDone={finish} />}
        </div>
      </main>
    </div>
  );
}
