import { create } from "zustand";
import { getSettings, updateSettings, type AppLanguage, type AppTheme } from "../lib/tauri/settings";

export type ThemeMode = AppTheme;
export type Language = AppLanguage;

type SettingsState = {
  theme: ThemeMode;
  language: Language;
  hydrated: boolean;
  /** Load persisted settings from the Rust backend once at startup. */
  hydrate: () => Promise<void>;
  setTheme: (theme: ThemeMode) => void;
  setLanguage: (language: Language) => void;
};

export const useSettingsStore = create<SettingsState>((set) => ({
  theme: "system",
  language: "en-US",
  hydrated: false,
  hydrate: async () => {
    try {
      const settings = await getSettings();
      set({ theme: settings.theme, language: settings.language, hydrated: true });
    } catch {
      // Backend unavailable (e.g. running outside Tauri during dev); keep defaults.
      set({ hydrated: true });
    }
  },
  setTheme: (theme) => {
    set({ theme });
    void updateSettings({ theme });
  },
  setLanguage: (language) => {
    set({ language });
    void updateSettings({ language });
  },
}));