import { useEffect } from "react";
import { useSettingsStore, type ThemeMode } from "../stores/settings-store";

export function applyTheme(theme: ThemeMode) {
  const systemDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  document.documentElement.classList.toggle("dark", theme === "dark" || (theme === "system" && systemDark));
}

export function useTheme() {
  const theme = useSettingsStore((state) => state.theme);
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => applyTheme(theme);
    apply(); media.addEventListener("change", apply); return () => media.removeEventListener("change", apply);
  }, [theme]);
}
