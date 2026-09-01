import { invoke } from "@tauri-apps/api/core";

export type AppTheme = "system" | "light" | "dark";
export type AppLanguage = "en-US" | "zh-CN";

export type AppSettings = {
  theme: AppTheme;
  language: AppLanguage;
};

export async function getSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("settings_get");
}

export async function updateSettings(patch: Partial<AppSettings>): Promise<AppSettings> {
  return invoke<AppSettings>("settings_update", { request: patch });
}