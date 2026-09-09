import { Menu } from "lucide-react";
import type { Ref } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../ui/button";
import { WindowControls } from "./WindowControls";
import { GlobalSearch } from "./GlobalSearch";
import { TitlebarTunnels } from "../../features/tunnels/TitlebarTunnels";
import runoryLogo from "../../../src-tauri/icons/64x64.png";

export function GlobalTopBar({ onOpenProfile, tabsHostRef, onOpenNavigation, onOpenTunnels, onOpenPricing }: {
  onOpenProfile: (profileId: string) => void;
  tabsHostRef: Ref<HTMLDivElement>;
  onOpenNavigation: () => void;
  onOpenTunnels: () => void;
  onOpenPricing: () => void;
}) {
  const { t } = useTranslation();
  return <header className="global-topbar" data-tauri-drag-region>
    <Button variant="ghost" size="icon" className="mobile-navigation-toggle" aria-label={t("mobile.openNavigation")} onClick={onOpenNavigation}><Menu size={18} /></Button>
    <div className="brand-lockup" aria-label="Runory" data-tauri-drag-region><img className="brand-mark" src={runoryLogo} alt="" data-tauri-drag-region /></div>
    <GlobalSearch onOpenProfile={onOpenProfile} />
    <div ref={tabsHostRef} className="titlebar-tabs-host" data-no-drag />
    <div className="titlebar-drag-spacer" data-tauri-drag-region />
    <TitlebarTunnels onOpenTunnels={onOpenTunnels} />
    <WindowControls onOpenPricing={onOpenPricing} />
  </header>;
}
