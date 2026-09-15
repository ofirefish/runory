import { useEffect } from "react";
import { applyTerminalSurface, resolveTerminalPalette } from "../features/terminal/terminal-themes";
import { useSettingsStore, type ThemeMode } from "../stores/settings-store";

export function applyTheme(theme: ThemeMode) {
  const systemDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const dark = theme === "dark" || (theme === "system" && systemDark);
  document.documentElement.classList.toggle("dark", dark);
  const terminalTheme = useSettingsStore.getState().terminalTheme;
  applyTerminalSurface(resolveTerminalPalette(terminalTheme, dark).background);
}

export function useTheme() {
  const theme = useSettingsStore((state) => state.theme);
  const terminalTheme = useSettingsStore((state) => state.terminalTheme);
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => applyTheme(theme);
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme, terminalTheme]);
}
