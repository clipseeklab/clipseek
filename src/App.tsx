import appI18n from "@/i18n";
import { queryKeys } from "@/lib/queryKeys";
import LibraryUpgradeDialog from "@/components/library/LibraryUpgradeDialog";
/** Load libraries, current session, and preferences at startup; show onboarding when empty,
 * otherwise the main layout, and synchronize library-switch events.
 */
import { useEffect, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { withLibraryAuthorization } from "@/lib/libraryAccess";
import { errorCode, explainError } from "@/lib/errors";
import LibraryRecovery from "@/components/library/LibraryRecovery";
import { applyTheme } from "@/lib/theme";
import { detectSystemLanguage } from "@/i18n";
import { flushRecentTags, useAppStore } from "@/stores/app";
import type { LibraryMeta, OnboardingState } from "@/types";
import { prepareFirstUseTips } from "@/lib/usageTips";
import AppLayout from "@/components/layout/AppLayout";
import FirstRunWizard from "@/components/wizard/FirstRunWizard";
import { Button } from "@/components/ui/button";
import { isWindows } from "@/lib/platform";
import WindowsTitleBar from "@/components/layout/WindowsTitleBar";
export default function App() {
  const [modelsReady, setModelsReady] = useState<boolean | null>(null);
  const [onboarding, setOnboarding] = useState<OnboardingState | null>(null);
  const [startupError, setStartupError] = useState<string | null>(null);
  const [libraryFailure, setLibraryFailure] = useState<{
    library: LibraryMeta;
    reason: string;
  } | null>(null);
  const [startupAttempt, setStartupAttempt] = useState(0);
  const { loaded, current, setLibraries, setCurrent, setSettings, setLoaded } = useAppStore();
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const inWindow = (content: ReactNode, mainActions = false) => (
    <>
      <LibraryUpgradeDialog />
      {isWindows ? (
        <div className="windows-app flex h-full min-h-0 flex-col bg-background">
          <WindowsTitleBar mainActions={mainActions} />
          <div className="min-h-0 flex-1">{content}</div>
        </div>
      ) : (
        content
      )}
    </>
  );
  useEffect(() => {
    api
      .modelStatus()
      .then((status) => setModelsReady(status.ready))
      .catch(() => setModelsReady(false));
  }, []);
  useEffect(() => {
    window.addEventListener("beforeunload", flushRecentTags);
    return () => {
      window.removeEventListener("beforeunload", flushRecentTags);
      flushRecentTags();
    };
  }, []);
  // Retry startup after a library-upgrade resolution event.
  useEffect(() => {
    const retry = () => setStartupAttempt((value) => value + 1);
    window.addEventListener("library-upgrade-resolved", retry);
    return () => window.removeEventListener("library-upgrade-resolved", retry);
  }, []);
  // Load initial configuration.
  useEffect(() => {
    let disposed = false;
    setLoaded(false);
    setStartupError(null);
    setLibraryFailure(null);
    void (async () => {
      try {
        const [libs, registeredCurrent, settings, onboardingState] = await Promise.all([
          api.libraryList(),
          api.libraryCurrent(),
          api.settingsGet(),
          api.onboardingGet(),
        ]);
        if (disposed) return;
        setLibraries(libs);
        setSettings(settings);
        await i18n.changeLanguage(
          settings.language === "auto" ? detectSystemLanguage() : settings.language,
        );
        applyTheme(settings.theme);
        // A configured current library may have failed to open during native startup.
        // Confirm the session and recover directory access before requesting assets.
        let cur: LibraryMeta | null = null;
        try {
          cur = registeredCurrent
            ? await withLibraryAuthorization(
                () => api.librarySwitch(registeredCurrent.id),
                [registeredCurrent],
                (library) => appI18n.t("library.reauthorizeTitle", { name: library.name }),
              )
            : null;
          if (registeredCurrent && !cur)
            throw new Error(appI18n.t("libraryRecovery.accessCancelled"));
        } catch (error) {
          if (disposed) return;
          setCurrent(null);
          setOnboarding(onboardingState);
          if (registeredCurrent)
            setLibraryFailure({
              library: registeredCurrent,
              reason:
                errorCode(error) === "library_dir_invalid"
                  ? appI18n.t("libraryRecovery.missing")
                  : explainError(error),
            });
          else throw error;
          return;
        }
        let effectiveOnboarding = onboardingState;
        if (effectiveOnboarding.completed && !cur) {
          effectiveOnboarding = await api.onboardingSet(2, false);
        } else if (!effectiveOnboarding.completed && effectiveOnboarding.currentStep === 2 && cur) {
          // Resume at import if creation committed before onboarding progress, avoiding
          // duplicate libraries.
          effectiveOnboarding = await api.onboardingSet(3, false);
        }
        if (disposed) return;
        setLibraries(libs);
        setCurrent(cur);
        setSettings(settings);
        setOnboarding(effectiveOnboarding);
        if (effectiveOnboarding.completed) prepareFirstUseTips({ existingOnly: true });
        else prepareFirstUseTips({ restart: true });
      } catch (error) {
        if (!disposed) setStartupError(explainError(error));
      } finally {
        if (!disposed) setLoaded(true);
      }
    })();
    return () => {
      disposed = true;
    };
  }, [i18n, setLibraries, setCurrent, setSettings, setLoaded, startupAttempt]);
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const refresh = ({ payload }: { payload: string }) => {
      if (payload !== useAppStore.getState().current?.id) return;
      if (timer) return;
      timer = setTimeout(() => {
        timer = undefined;
        if (payload !== useAppStore.getState().current?.id) return;
        for (const queryKey of [
          queryKeys.processing.library(payload),
          queryKeys.assets.library(payload),
          queryKeys.assetDetail.library(payload),
          queryKeys.indexTasks.library(payload),
          queryKeys.indexSummary.library(payload),
        ])
          void queryClient.invalidateQueries({ queryKey });
      }, 250);
    };
    const stop = listen<string>("index://changed", refresh);
    const stopProcessing = listen<string>("processing://changed", refresh);
    return () => {
      clearTimeout(timer);
      void stop.then((fn) => fn());
      void stopProcessing.then((fn) => fn());
    };
  }, [queryClient]);
  // Invalidate before switching so old observers cannot fetch using a new library context.
  useEffect(() => {
    const un = listen<LibraryMeta>("library://switched", (e) => {
      setCurrent(e.payload);
      api
        .libraryList()
        .then(setLibraries)
        .catch(() => {});
      void queryClient.invalidateQueries({ refetchType: "none" });
      void queryClient.invalidateQueries({
        predicate: (query) => query.queryKey[1] === e.payload.id,
      });
    });
    return () => {
      void un.then((f) => f());
    };
  }, [setCurrent, setLibraries, queryClient]);
  const completeRecovery = async (replacement: LibraryMeta) => {
    if (!libraryFailure) return;
    // Opening the replacement has succeeded. Only remove the old registration.
    const libraries = await api.libraryList();
    const removeOld = replacement.id !== libraryFailure.library.id;
    if (removeOld && libraries.some((item) => item.id === libraryFailure.library.id)) {
      try {
        await api.libraryDelete(libraryFailure.library.id, false);
      } catch (error) {
        throw Object.assign(
          new Error(t("libraryRecovery.cleanupFailed", { message: explainError(error) })),
          { cause: error },
        );
      }
    }
    setLibraries(libraries.filter((item) => !removeOld || item.id !== libraryFailure.library.id));
    setCurrent(replacement);
    void queryClient.invalidateQueries({ refetchType: "none" });
    void queryClient.invalidateQueries({
      predicate: (query) => query.queryKey[1] === replacement.id,
    });
    setStartupAttempt((attempt) => attempt + 1);
  };
  if (!loaded || modelsReady === null) {
    return inWindow(
      <div className="flex h-full items-center justify-center text-muted-foreground">
        ClipSeek Open…
      </div>,
    );
  }
  if (libraryFailure)
    return inWindow(
      <LibraryRecovery
        library={libraryFailure.library}
        reason={libraryFailure.reason}
        onRecovered={completeRecovery}
        onRetry={() => setStartupAttempt((attempt) => attempt + 1)}
      />,
    );
  if (startupError !== null) {
    return inWindow(
      <div className="flex h-full items-center justify-center p-8">
        <div role="alert" className="flex max-w-sm flex-col items-center gap-4 text-center">
          <p className="text-sm text-muted-foreground">{t("common.startupFailed")}</p>
          {startupError && startupError !== t("common.startupFailed") && (
            <p className="break-all text-sm text-destructive">{startupError}</p>
          )}
          <Button onClick={() => setStartupAttempt((attempt) => attempt + 1)}>
            {t("common.retry")}
          </Button>
        </div>
      </div>,
    );
  }
  if (onboarding === null) {
    return inWindow(
      <div className="flex h-full items-center justify-center text-muted-foreground">
        ClipSeek Open…
      </div>,
    );
  }
  if (!onboarding.completed) {
    return inWindow(
      <FirstRunWizard
        initialState={onboarding}
        modelsReady={modelsReady}
        onModelsReady={() => setModelsReady(true)}
        onStateChange={setOnboarding}
        onComplete={() => setOnboarding((state) => (state ? { ...state, completed: true } : state))}
      />,
    );
  }
  if (!current) return inWindow(null);
  return inWindow(<AppLayout />, true);
}
