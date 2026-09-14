import { lazy, Suspense, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ServerManagement } from "../components/layout/ServerManagement";
import { Workspace } from "../components/layout/Workspace";
import { GlobalTopBar } from "../components/layout/GlobalTopBar";
import { PrimaryNavigationRail } from "../components/layout/PrimaryNavigationRail";
import { SettingsPanel, type SettingsSection } from "../features/settings/SettingsPanel";
import { DesktopUpdateController } from "../features/settings/DesktopUpdateController";
import { MobilePrivacyGuard } from "../features/mobile/MobilePrivacyGuard";
import { useLanguage } from "../hooks/use-language";
import { useTheme } from "../hooks/use-theme";
import { useCatalogStore } from "../stores/catalog-store";
import { useSessionStore } from "../stores/session-store";
import { TunnelsPage } from "../features/tunnels/TunnelsPage";
import { Toaster } from "../components/ui/sonner";

const AuthDialog = lazy(() => import("../features/auth/AuthDialog").then((module) => ({ default: module.AuthDialog })));
const PricingDialog = lazy(() => import("../features/settings/PricingDialog").then((module) => ({ default: module.PricingDialog })));

export function App() {
  const { t } = useTranslation();
  useTheme();
  useLanguage();
  const [mobileNavigationOpen, setMobileNavigationOpen] = useState(false);
  const [activeNavigation, setActiveNavigation] = useState<"servers" | "sessions" | "tunnels">("servers");
  const [tunnelProfileId, setTunnelProfileId] = useState<string | undefined>();
  const [query, setQuery] = useState("");
  const [titlebarTabsHost, setTitlebarTabsHost] = useState<HTMLDivElement | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [authOpen, setAuthOpen] = useState(false);
  const [pricingOpen, setPricingOpen] = useState(false);
  const [settingsSection, setSettingsSection] = useState<SettingsSection>("general");
  const [connectProfileRequest, setConnectProfileRequest] = useState<{ profileId: string; requestId: number } | null>(null);
  const load = useCatalogStore((state) => state.load);
  useEffect(() => { void load(); }, [load]);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMobileNavigationOpen(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
  useEffect(() => {
    const onOpenSettings = (event: Event) => {
      const detail = (event as CustomEvent<{ section?: SettingsSection }>).detail;
      setSettingsSection(detail?.section ?? "general");
      setSettingsOpen(true);
    };
    window.addEventListener("runory:open-settings", onOpenSettings);
    return () => window.removeEventListener("runory:open-settings", onOpenSettings);
  }, []);
  const selectNavigation = (item: "servers" | "sessions" | "tunnels" | "settings") => {
    if (item === "tunnels") setTunnelProfileId(undefined);
    setMobileNavigationOpen(false);
    if (item === "settings") { setSettingsSection("general"); setSettingsOpen(true); }
    else setActiveNavigation(item);
  };
  const connectProfile = (profileId: string) => {
    setActiveNavigation("sessions");
    setConnectProfileRequest((previous) => ({ profileId, requestId: (previous?.requestId ?? 0) + 1 }));
  };
  return <><div className="app-shell">
    <GlobalTopBar onOpenProfile={connectProfile} tabsHostRef={setTitlebarTabsHost} onOpenNavigation={() => setMobileNavigationOpen((open) => !open)} onOpenTunnels={() => selectNavigation("tunnels")} onOpenPricing={() => setPricingOpen(true)} />
    <div className="desktop-shell-body">
      {mobileNavigationOpen && <button type="button" className="mobile-navigation-backdrop" aria-label={t("mobile.closeNavigation")} onClick={() => setMobileNavigationOpen(false)} />}
      <PrimaryNavigationRail active={activeNavigation} onSelect={selectNavigation} onOpenAccount={() => { setMobileNavigationOpen(false); setSettingsSection("account"); setSettingsOpen(true); }} onOpenAuth={() => { setMobileNavigationOpen(false); setAuthOpen(true); }} onOpenPricing={() => { setMobileNavigationOpen(false); setPricingOpen(true); }} mobileOpen={mobileNavigationOpen} />
      {activeNavigation === "servers" && <ServerManagement query={query} onQueryChange={setQuery} onConnectProfile={connectProfile} onOpenSync={() => { setSettingsSection("cloud"); setSettingsOpen(true); }} onOpenAuth={() => setAuthOpen(true)} onCreateTunnel={(profileId) => { setTunnelProfileId(profileId); setActiveNavigation("tunnels"); }} />}
      {activeNavigation === "tunnels" && <TunnelsPage initialProfileId={tunnelProfileId} onConnectProfile={connectProfile} onShowSession={(tabId) => { useSessionStore.getState().setActive(tabId); setActiveNavigation("sessions"); }} />}
      {/* Keep terminal instances and channel bindings alive across page changes. */}
      <Workspace visible={activeNavigation === "sessions"} onSelectServer={() => selectNavigation("servers")} titlebarTabsHost={titlebarTabsHost} onShowSessions={() => selectNavigation("sessions")} connectProfileRequest={connectProfileRequest} />
    </div>
    {settingsOpen && <SettingsPanel initialSection={settingsSection} onClose={() => setSettingsOpen(false)} />}
    {pricingOpen && <Suspense fallback={null}><PricingDialog onClose={() => setPricingOpen(false)} onOpenAuth={() => { setPricingOpen(false); setAuthOpen(true); }} /></Suspense>}
    {authOpen && <Suspense fallback={null}><AuthDialog onClose={() => setAuthOpen(false)} /></Suspense>}
    <DesktopUpdateController onOpenSettings={() => { setSettingsSection("general"); setSettingsOpen(true); }} />
    <MobilePrivacyGuard />
  </div><Toaster /></>;
}
