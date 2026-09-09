import React from "react";
import ReactDOM from "react-dom/client";
import "@xterm/xterm/css/xterm.css";
import "./styles.css";
import "./i18n";
import { App } from "./app/App";
import { applyTheme } from "./hooks/use-theme";
import { useSettingsStore } from "./stores/settings-store";
import { cloudConfigured } from "./lib/supabase/client";
import { isTauri } from "@tauri-apps/api/core";
import { initializeCloudOAuthDeepLinks } from "./lib/supabase/oauth-deep-link";
import { initializeCloudSessionPersistence } from "./lib/supabase/session-persistence";

async function bootstrap() {
  const authMode = window.location.pathname === "/auth/confirm"
    ? "confirm"
    : window.location.pathname === "/auth/reset" ? "reset" : null;
  if (authMode) {
    const { AuthWebPage } = await import("./features/auth/AuthWebPage");
    ReactDOM.createRoot(document.getElementById("root")!).render(<React.StrictMode><AuthWebPage mode={authMode} /></React.StrictMode>);
    return;
  }
  await useSettingsStore.getState().hydrate();
  if (cloudConfigured && isTauri()) {
    await initializeCloudSessionPersistence();
    await initializeCloudOAuthDeepLinks().catch(() => undefined);
  }
  applyTheme(useSettingsStore.getState().theme);
  ReactDOM.createRoot(document.getElementById("root")!).render(<React.StrictMode><App /></React.StrictMode>);
}

void bootstrap();
