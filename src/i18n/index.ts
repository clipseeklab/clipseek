/** Initialize English by default while allowing persisted language preferences. */
import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { zh } from "./zh";
import { en } from "./en";
/** Resolve explicit system-following mode: zh prefixes select Chinese, otherwise English. */
export function detectSystemLanguage(): "zh" | "en" {
  const lang = (navigator.language || "en").toLowerCase();
  return lang.startsWith("zh") ? "zh" : "en";
}
i18n.on("languageChanged", (language: string) => {
  document.documentElement.lang = language.startsWith("zh") ? "zh-CN" : "en";
});
i18n
  .use(initReactI18next)
  .init({
    resources: {
      zh: { translation: zh },
      en: { translation: en },
    },
    lng: "en",
    fallbackLng: "en",
    interpolation: { escapeValue: false },
  })
  .catch((error: unknown) => {
    // Translation setup precedes the UI; report failures without relying on translated toasts.
    console.error("Language initialization failed", error);
  });
export default i18n;
