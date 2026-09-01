import { Menu, PanelLeftClose, PanelLeftOpen, Plus, Search } from "lucide-react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "react-i18next";
import { Button } from "../ui/button";
import { WindowControls } from "./WindowControls";
import runoryLogo from "../../../src-tauri/icons/64x64.png";

export function GlobalTopBar({ query, onQueryChange, explorerOpen, onToggleExplorer, onNewConnection, onOpenNavigation }: {
  query: string;
  onQueryChange: (query: string) => void;
  explorerOpen: boolean;
  onToggleExplorer: () => void;
  onNewConnection: () => void;
  onOpenNavigation: () => void;
}) {
  const { t } = useTranslation();
  const searchShortcut = navigator.userAgent.includes("Macintosh") ? "⌘ K" : "Ctrl K";
  const toggleMaximize = () => {
    if (isTauri()) void getCurrentWindow().toggleMaximize().catch(() => undefined);
  };
  return <header className="global-topbar" data-tauri-drag-region onDoubleClick={(event) => { if (!(event.target as HTMLElement).closest("button,input,label,[data-no-drag]")) toggleMaximize(); }}>
    <Button variant="ghost" size="icon" className="md:hidden" aria-label={t("mobile.openNavigation")} onClick={onOpenNavigation}><Menu size={18} /></Button>
    <div className="brand-lockup" aria-label="Runory" data-tauri-drag-region><img className="brand-mark" src={runoryLogo} alt="" data-tauri-drag-region /><span data-tauri-drag-region>Runory</span></div>
    <Button type="button" variant="ghost" size="icon" className="titlebar-explorer-toggle hidden md:inline-flex" data-no-drag aria-label={t(explorerOpen ? "shell.hideExplorer" : "shell.showExplorer")} title={t(explorerOpen ? "shell.hideExplorer" : "shell.showExplorer")} onClick={onToggleExplorer}>{explorerOpen ? <PanelLeftClose size={17} /> : <PanelLeftOpen size={17} />}</Button>
    <div className="environment-switcher" aria-label={t("shell.environment")} data-no-drag><span>{t("shell.environment")}</span><strong>{t("shell.local")}</strong></div>
    <label className="global-search" data-no-drag>
      <Search size={16} aria-hidden="true" />
      <span className="sr-only">{t("shell.globalSearch")}</span>
      <input value={query} onChange={(event) => onQueryChange(event.target.value)} placeholder={t("shell.globalSearch")} />
      <kbd>{searchShortcut}</kbd>
    </label>
    <Button size="sm" className="new-connection" onClick={onNewConnection}><Plus size={16} />{t("profile.createTitle")}</Button>
    <div className="titlebar-drag-spacer" data-tauri-drag-region />
    <WindowControls />
  </header>;
}
