import React from "react";
import ReactDOM from "react-dom/client";
import "@xterm/xterm/css/xterm.css";
import "./styles.css";
import "./i18n";
import { App } from "./app/App";
import { applyTheme } from "./hooks/use-theme";
import { useSettingsStore } from "./stores/settings-store";

async function bootstrap() {
  await useSettingsStore.getState().hydrate();
  applyTheme(useSettingsStore.getState().theme);
  ReactDOM.createRoot(document.getElementById("root")!).render(<React.StrictMode><App /></React.StrictMode>);
}

void bootstrap();
