import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { applyTheme } from "@/lib/theme";
import { useAppStore } from "@/stores/app";
export type SettingsPatch = Parameters<typeof api.settingsSet>[0];
/** Persist before applying UI settings; callers display failures for their interaction. */
export function useSettingsUpdate() {
  const { i18n } = useTranslation();
  const setSettings = useAppStore((state) => state.setSettings);
  return async (patch: SettingsPatch) => {
    const next = await api.settingsSet(patch);
    setSettings(next);
    if (patch.language) {
      const language =
        next.language === "auto"
          ? navigator.language.toLowerCase().startsWith("zh")
            ? "zh"
            : "en"
          : next.language;
      await i18n.changeLanguage(language);
    }
    if (patch.theme) applyTheme(next.theme);
  };
}
