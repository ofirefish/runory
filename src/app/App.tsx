import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Sidebar } from "../components/layout/Sidebar";
import { Workspace } from "../components/layout/Workspace";
import { GlobalTopBar } from "../components/layout/GlobalTopBar";
import { PrimaryNavigationRail } from "../components/layout/PrimaryNavigationRail";
import { MobilePrivacyGuard } from "../features/mobile/MobilePrivacyGuard";
import { useTheme } from "../hooks/use-theme";
import { useCatalogStore } from "../stores/catalog-store";

export function App() {
  const { t } = useTranslation();
  useTheme();
  const [mobileNavigationOpen, setMobileNavigationOpen] = useState(false);
  const [activeNavigation, setActiveNavigation] = useState<"servers" | "sessions" | "settings">("servers");
  const [explorerOpen, setExplorerOpen] = useState(true);
  const [query, setQuery] = useState("");
  const [newProfileRequest, setNewProfileRequest] = useState(0);
  const [settingsRequest, setSettingsRequest] = useState(0);
  const [connectProfileRequest, setConnectProfileRequest] = useState<{ profileId: string; requestId: number } | null>(null);
  const load = useCatalogStore((state) => state.load);
  useEffect(() => { void load(); }, [load]);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && !event.shiftKey && event.key.toLowerCase() === "b") { event.preventDefault(); setExplorerOpen((open) => !open); }
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") { event.preventDefault(); document.querySelector<HTMLInputElement>(".global-search input")?.focus(); }
    };
    window.addEventListener("keydown", onKeyDown); return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
  const selectNavigation = (item: "servers" | "sessions" | "settings") => {
    setActiveNavigation(item);
    if (item === "servers" || item === "settings") setExplorerOpen(true);
    if (item === "settings") setSettingsRequest((value) => value + 1);
  };
  return <div className="app-shell">
    <GlobalTopBar query={query} onQueryChange={setQuery} explorerOpen={explorerOpen} onToggleExplorer={() => setExplorerOpen((open) => !open)} onNewConnection={() => { setExplorerOpen(true); setNewProfileRequest((value) => value + 1); }} onOpenNavigation={() => { setExplorerOpen(true); setMobileNavigationOpen(true); }} />
    <div className="desktop-shell-body">
      <PrimaryNavigationRail active={activeNavigation} onSelect={selectNavigation} />
      {mobileNavigationOpen && <button type="button" className="fixed inset-0 z-40 bg-black/45 md:hidden" aria-label={t("mobile.closeNavigation")} onClick={() => setMobileNavigationOpen(false)} />}
      {explorerOpen && <Sidebar mobileOpen={mobileNavigationOpen} onMobileClose={() => setMobileNavigationOpen(false)} query={query} onQueryChange={setQuery} newProfileRequest={newProfileRequest} settingsRequest={settingsRequest} onConnectProfile={(profileId) => setConnectProfileRequest((previous) => ({ profileId, requestId: (previous?.requestId ?? 0) + 1 }))} />}
      <Workspace onOpenNavigation={() => setMobileNavigationOpen(true)} connectProfileRequest={connectProfileRequest} />
    </div>
    <MobilePrivacyGuard />
  </div>;
}
