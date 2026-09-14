import { useEffect } from "react";
import i18n from "../i18n";
import { useSettingsStore, type Language } from "../stores/settings-store";

/** Apply persisted language to i18next and the document before/alongside first paint. */
export async function applyLanguage(language: Language) {
  document.documentElement.lang = language;
  await i18n.changeLanguage(language);
}

export function useLanguage() {
  const language = useSettingsStore((state) => state.language);
  useEffect(() => {
    void applyLanguage(language);
  }, [language]);
}
