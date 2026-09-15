import React from "react";
import ReactDOM from "react-dom/client";
import "@xterm/xterm/css/xterm.css";
import "./styles.css";
import "./i18n";
import { App } from "./app/App";
import { applyLanguage } from "./hooks/use-language";
import { applyTheme } from "./hooks/use-theme";
import { useSettingsStore } from "./stores/settings-store";
import { applyTerminalSurface, resolveTerminalPalette } from "./features/terminal/terminal-themes";
import { cloudConfigured } from "./lib/supabase/client";
import { isTauri } from "@tauri-apps/api/core";
import { initializeCloudOAuthDeepLinks } from "./lib/supabase/oauth-deep-link";
import { initializeCloudSessionPersistence } from "./lib/supabase/session-persistence";

/** Reveal the main window after first paint; safe to call more than once. */
async function revealMainWindow() {
  if (!isTauri()) return;
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const current = getCurrentWindow();
    await current.show();
    await current.setFocus().catch(() => undefined);
  } catch {
    // Window may already be visible in browser/dev fallbacks.
  }
}

/** Schedule show after paint, with a fail-safe so the window never stays hidden. */
function scheduleWindowReveal() {
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      void revealMainWindow();
    });
  });
  window.setTimeout(() => {
    void revealMainWindow();
  }, 2500);
}

/** Hydrate settings and optional cloud session after the shell is on screen. */
async function hydrateAfterFirstPaint() {
  await useSettingsStore.getState().hydrate();
  const settings = useSettingsStore.getState();
  applyTheme(settings.theme);
  applyTerminalSurface(resolveTerminalPalette(settings.terminalTheme, document.documentElement.classList.contains("dark")).background);
  await applyLanguage(settings.language);
  if (cloudConfigured && isTauri()) {
    await initializeCloudSessionPersistence().catch(() => undefined);
    await initializeCloudOAuthDeepLinks().catch(() => undefined);
  }
}

async function bootstrap() {
  applyTheme("system");

  const authMode = window.location.pathname === "/auth/confirm"
    ? "confirm"
    : window.location.pathname === "/auth/reset" ? "reset" : null;

  if (authMode) {
    const { AuthWebPage } = await import("./features/auth/AuthWebPage");
    ReactDOM.createRoot(document.getElementById("root")!).render(
      <React.StrictMode><AuthWebPage mode={authMode} /></React.StrictMode>,
    );
    scheduleWindowReveal();
    return;
  }

  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode><App /></React.StrictMode>,
  );
  scheduleWindowReveal();
  void hydrateAfterFirstPaint();
}

void bootstrap();
