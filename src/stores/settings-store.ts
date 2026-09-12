import { create } from "zustand";
import { getSettings, updateSettings, type AppLanguage, type AppTheme } from "../lib/tauri/settings";

export type ThemeMode = AppTheme;
export type Language = AppLanguage;

type SettingsState = {
  theme: ThemeMode;
  language: Language;
  boundaryCliPath: string;
  teleportCliPath: string;
  hydrated: boolean;
  saving: boolean;
  persistenceError: boolean;
  /** Load persisted settings from the Rust backend once at startup. */
  hydrate: () => Promise<void>;
  setTheme: (theme: ThemeMode) => Promise<void>;
  setLanguage: (language: Language) => Promise<void>;
  setBoundaryCliPath: (path: string) => Promise<void>;
  setTeleportCliPath: (path: string) => Promise<void>;
};

export const useSettingsStore = create<SettingsState>((set) => {
  let writeQueue: Promise<void> = Promise.resolve();
  let pendingWrites = 0;

  const applySaved = (saved: Awaited<ReturnType<typeof updateSettings>>) => ({
    theme: saved.theme,
    language: saved.language,
    boundaryCliPath: saved.boundaryCliPath?.trim() || "",
    teleportCliPath: saved.teleportCliPath?.trim() || "",
  });

  const persist = (patch: Partial<{
    theme: ThemeMode;
    language: Language;
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
