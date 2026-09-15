import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en-US.json";

void i18n.use(initReactI18next).init({
  resources: { "en-US": { translation: en } },
  lng: "en-US",
  fallbackLng: "en-US",
  interpolation: { escapeValue: false },
});

/** Load a language pack on demand so cold start only ships the default locale. */
export async function ensureLanguageResources(language: string) {
  if (i18n.hasResourceBundle(language, "translation")) return;
  if (language === "zh-CN") {
    const zh = await import("./locales/zh-CN.json");
    i18n.addResourceBundle("zh-CN", "translation", zh.default ?? zh, true, true);
  }
}

export default i18n;
