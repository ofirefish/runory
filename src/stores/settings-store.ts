import { create } from "zustand";
import {
  getSettings,
  updateSettings,
  type AppLanguage,
  type AppTheme,
  type TerminalThemeId,
} from "../lib/tauri/settings";

export type ThemeMode = AppTheme;
export type Language = AppLanguage;
export type { TerminalThemeId };

type SettingsState = {
  theme: ThemeMode;
  language: Language;
  terminalTheme: TerminalThemeId;
  boundaryCliPath: string;
  teleportCliPath: string;
  hydrated: boolean;
  saving: boolean;
  persistenceError: boolean;
  /** Load persisted settings from the Rust backend once at startup. */
  hydrate: () => Promise<void>;
  setTheme: (theme: ThemeMode) => Promise<void>;
  setLanguage: (language: Language) => Promise<void>;
  setTerminalTheme: (terminalTheme: TerminalThemeId) => Promise<void>;
  setBoundaryCliPath: (path: string) => Promise<void>;
  setTeleportCliPath: (path: string) => Promise<void>;
};

const DEFAULT_TERMINAL_THEME: TerminalThemeId = "runory";

function normalizeTerminalTheme(value: unknown): TerminalThemeId {
  const allowed: TerminalThemeId[] = ["runory", "oneDark", "tokyoNight", "catppuccin", "solarized"];
  return allowed.includes(value as TerminalThemeId) ? (value as TerminalThemeId) : DEFAULT_TERMINAL_THEME;
}

export const useSettingsStore = create<SettingsState>((set) => {
  let writeQueue: Promise<void> = Promise.resolve();
  let pendingWrites = 0;

  const applySaved = (saved: Awaited<ReturnType<typeof updateSettings>>) => ({
    theme: saved.theme,
    language: saved.language,
    terminalTheme: normalizeTerminalTheme(saved.terminalTheme),
    boundaryCliPath: saved.boundaryCliPath?.trim() || "",
    teleportCliPath: saved.teleportCliPath?.trim() || "",
  });

  const persist = (patch: Partial<{
    theme: ThemeMode;
    language: Language;
    terminalTheme: TerminalThemeId;
    boundaryCliPath: string;
    teleportCliPath: string;
  }>) => {
    pendingWrites += 1;
    set({ saving: true, persistenceError: false });
    const operation = writeQueue.then(async () => {
      try {
        const saved = await updateSettings(patch);
        pendingWrites -= 1;
        if (pendingWrites === 0) set({ ...applySaved(saved), saving: false });
      } catch {
        pendingWrites -= 1;
        try {
          const persisted = await getSettings();
          set({ ...applySaved(persisted), saving: pendingWrites > 0, persistenceError: true });
        } catch {
          set({ saving: pendingWrites > 0, persistenceError: true });
        }
      }
    });
    writeQueue = operation.catch(() => undefined);
    return operation;
  };

  return {
    theme: "system",
    language: "en-US",
    terminalTheme: DEFAULT_TERMINAL_THEME,
    boundaryCliPath: "",
    teleportCliPath: "",
    hydrated: false,
    saving: false,
    persistenceError: false,
    hydrate: async () => {
      try {
        const settings = await getSettings();
        set({ ...applySaved(settings), hydrated: true, persistenceError: false });
      } catch {
        set({ hydrated: true, persistenceError: true });
      }
    },
    setTheme: (theme) => {
      set({ theme });
      return persist({ theme });
    },
    setLanguage: (language) => {
      set({ language });
      return persist({ language });
    },
    setTerminalTheme: (terminalTheme) => {
      set({ terminalTheme });
      return persist({ terminalTheme });
    },
    setBoundaryCliPath: (path) => {
      const boundaryCliPath = path.trim();
      set({ boundaryCliPath });
      return persist({ boundaryCliPath });
    },
    setTeleportCliPath: (path) => {
      const teleportCliPath = path.trim();
      set({ teleportCliPath });
      return persist({ teleportCliPath });
    },
  };
});
