import { CirclePlus, Menu, SquareTerminal, X } from "lucide-react";
import { type MouseEvent, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ConnectionDialog } from "../../features/sessions/ConnectionDialog";
import { TerminalView, type TerminalHandle } from "../../features/terminal/TerminalView";
import { connectSsh, disconnectSsh, reconnectSsh, testSsh } from "../../lib/tauri/ssh";
import { useCatalogStore } from "../../stores/catalog-store";
import { useSessionStore } from "../../stores/session-store";
import type { ConnectRequest, SessionState } from "../../types/session";
import { Button } from "../ui/button";
import { ContextDock, type WorkspaceView } from "./ContextDock";
import { ContextPanel } from "../context-panel/ContextPanel";
import { useContextPanelStore } from "../context-panel/context-panel-store";
import { ProfileDialog } from "../../features/profiles/ProfileDialog";
import { OsLogo } from "../../features/profiles/OsLogo";
import { RenameWorkspaceTabDialog } from "./RenameWorkspaceTabDialog";
import { WorkspaceTabMenu, type WorkspaceTabMenuState } from "./WorkspaceTabMenu";

type DialogState = { mode: "connect" | "reconnect"; profileId: string; tabId?: string; title?: string };
const statusColor = (state: SessionState) => state === "connected" ? "bg-emerald-500" : state === "error" ? "bg-red-500" : ["connecting", "verifying-host", "authenticating", "opening-shell"].includes(state) ? "bg-blue-500" : "bg-slate-500";
const workspaceViews: WorkspaceView[] = ["terminal", "files", "assistant", "changes", "dashboard", "operations", "deployment"];

export function Workspace({ onOpenNavigation, connectProfileRequest }: { onOpenNavigation?: () => void; connectProfileRequest?: { profileId: string; requestId: number } | null }) {
  const { t } = useTranslation();
  const [dialog, setDialog] = useState<DialogState | null>(null);
  const [editProfile, setEditProfile] = useState(false);
  const [tabMenu, setTabMenu] = useState<WorkspaceTabMenuState | null>(null);
  const [renamingTabId, setRenamingTabId] = useState<string | null>(null);
  const [reviewRunId, setReviewRunId] = useState<string | null>(null);
  const toggleContextPanel = useContextPanelStore((store) => store.setVisible);
  const [workspaceView, setWorkspaceView] = useState<WorkspaceView>(() => { const saved = localStorage.getItem("runory.workspaceView") as WorkspaceView | null; return saved && workspaceViews.includes(saved) ? saved : "terminal"; });
  const terminalRefs = useRef(new Map<string, TerminalHandle>());
  const { profiles, selectedProfileId, load } = useCatalogStore();
  const { tabs, activeTabId, addTab, beginReconnect, attachSession, setState, markClosed, markError, setActive, renameTab, removeTab } = useSessionStore();
  const selectedProfile = profiles.find((profile) => profile.id === selectedProfileId) ?? null;
  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? null;
  const activeProfile = profiles.find((profile) => profile.id === activeTab?.profileId) ?? null;
  const dialogProfile = profiles.find((profile) => profile.id === dialog?.profileId) ?? null;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.shiftKey && event.key.toLowerCase() === "b") { event.preventDefault(); toggleContextPanel(!useContextPanelStore.getState().visible); }
    };
    window.addEventListener("keydown", onKeyDown); return () => window.removeEventListener("keydown", onKeyDown);
  }, [toggleContextPanel]);
  useEffect(() => { localStorage.setItem("runory.workspaceView", workspaceView); }, [workspaceView]);
  useEffect(() => {
    if (connectProfileRequest) setDialog({ mode: "connect", profileId: connectProfileRequest.profileId });
  }, [connectProfileRequest]);

  const openSession = async (values: Omit<ConnectRequest, "cols" | "rows">, tabId: string, reconnecting: boolean, title?: string) => {
    const connectionAttemptId = crypto.randomUUID();
    const tabExists = useSessionStore.getState().tabs.some((tab) => tab.id === tabId);
    if (tabExists) beginReconnect(tabId, connectionAttemptId);
    else addTab({ id: tabId, profileId: values.profileId, title, sessionId: null, connectionAttemptId, state: "connecting", view: "terminal" });
    const size = terminalRefs.current.get(tabId)?.dimensions() ?? { cols: 120, rows: 34 };
    try {
      const open = reconnecting ? reconnectSsh : connectSsh;
      const response = await open({ ...values, ...size }, (event) => {
        if (event.event === "output") terminalRefs.current.get(tabId)?.write(event.data.bytes);
        else if (event.event === "state") setState(tabId, connectionAttemptId, event.data.state);
        else if (event.data.reason !== "terminal-closed") markClosed(tabId, connectionAttemptId);
      });
      attachSession(tabId, connectionAttemptId, response.sessionId);
      void load();
      return response.credentialSaved;
    } catch (error) { setState(tabId, connectionAttemptId, "error"); throw error; }
  };

  const disconnectTab = async (tabId: string) => {
    const tab = useSessionStore.getState().tabs.find((candidate) => candidate.id === tabId);
    if (!tab?.sessionId) return;
    try { await disconnectSsh(tab.sessionId); } finally { markClosed(tab.id, tab.connectionAttemptId); }
  };
  const closeTab = async (tabId: string) => {
    const tab = useSessionStore.getState().tabs.find((candidate) => candidate.id === tabId);
    if (tab?.sessionId) { try { await disconnectSsh(tab.sessionId); } catch { /* Rust may already have removed a remotely closed session. */ } }
    terminalRefs.current.delete(tabId);
    removeTab(tabId);
  };
  const openConnectDialog = () => { if (selectedProfile) setDialog({ mode: "connect", profileId: selectedProfile.id }); };
  const openReconnectDialog = (tabId: string) => { const tab = tabs.find((candidate) => candidate.id === tabId); if (tab) setDialog({ mode: "reconnect", profileId: tab.profileId, tabId }); };
  const openTabMenu = (event: MouseEvent, tabId: string) => {
    event.preventDefault();
    setActive(tabId);
    const width = 176;
    const height = 112;
    setTabMenu({ tabId, x: Math.max(8, Math.min(event.clientX, window.innerWidth - width - 8)), y: Math.max(8, Math.min(event.clientY, window.innerHeight - height - 8)) });
  };
  const test = async (values: Omit<ConnectRequest, "cols" | "rows">) => (await testSsh(values)).credentialSaved;
  /** Review Plan (方案 A): open the ChangeSet Review in the central Main
   *  Workspace while the right Agent panel stays open. */
  const openChangeSetReview = (runId: string) => {
    setReviewRunId(runId);
    setWorkspaceView("changes");
    window.setTimeout(fitTerminals, 0);
  };
  /** Right Panel width changes trigger xterm fit so the terminal never
   *  stays mis-sized when the Inspector / Agent panel resizes or collapses. */
  const fitTerminals = () => {
    window.setTimeout(() => { window.dispatchEvent(new Event("resize")); }, 0);
  };
  const openExplorerAndSelect = () => {
    // "Select Server": surface the primary navigation so the user can pick
    // a context. No guessing-target behavior.
    document.querySelector<HTMLButtonElement>('[data-nav="servers"]')?.click();
  };

  return <main className="workspace-shell flex min-w-0 flex-1 flex-col bg-[hsl(var(--background))]">
    {tabs.length > 0 && <div className="workspace-tabs">
      <Button variant="ghost" size="icon" className="shrink-0 md:hidden" aria-label={t("mobile.openNavigation")} onClick={onOpenNavigation}><Menu size={19} /></Button>
      <div className="workspace-tab-strip">
        {tabs.map((tab) => { const profile = profiles.find((candidate) => candidate.id === tab.profileId); const active = tab.id === activeTabId; const title = tab.title ?? profile?.name ?? t("terminal.unknownProfile"); return <div key={tab.id} className={`workspace-tab ${active ? "active" : ""}`} onContextMenu={(event) => openTabMenu(event, tab.id)}>
          <button type="button" className="workspace-tab-select" onClick={() => setActive(tab.id)}>{profile?.osDistribution ? <OsLogo plain distribution={profile.osDistribution} state={tab.state} statusLabel={t(`status.${tab.state}`)} /> : <span className={`workspace-tab-status ${statusColor(tab.state)}`} aria-label={t(`status.${tab.state}`)} />}<span>{title}</span></button>
          <button type="button" className="workspace-tab-close" aria-label={t("terminal.closeTab", { name: title })} onClick={() => void closeTab(tab.id)}><X size={13} /></button>
        </div>; })}
        <Button variant="ghost" size="icon" className="workspace-new-tab" aria-label={t("terminal.newTab")} disabled={!selectedProfile} title={!selectedProfile ? t("terminal.selectHost") : t("terminal.newTab")} onClick={openConnectDialog}><CirclePlus size={16} /></Button>
      </div>
    </div>}
    <div className="workspace-stage">
      <div className="workspace-main-column">
        {tabs.length === 0 && <div className="empty-workspace"><span className="empty-terminal-icon"><SquareTerminal size={24} /></span><strong>{t("terminal.readyTitle")}</strong><span>{t("terminal.ready")}</span><Button size="sm" disabled={!selectedProfile} onClick={openConnectDialog}><CirclePlus size={15} />{t("terminal.connect")}</Button></div>}
        {tabs.map((tab) => { const profile = profiles.find((candidate) => candidate.id === tab.profileId) ?? null; const active = tab.id === activeTabId; return <section key={tab.id} className={`session-workspace ${active ? "active" : ""}`} aria-hidden={!active}>
          <ContextDock view={workspaceView} sessionId={tab.sessionId} profileId={tab.profileId} profile={profile} active={active} reviewRunId={reviewRunId} onViewChange={setWorkspaceView} onInsertCommand={(command) => { terminalRefs.current.get(tab.id)?.insert(command); setWorkspaceView("terminal"); }} terminalContent={<div className="terminal-panel"><div className="terminal-panel-content"><TerminalView ref={(handle) => { if (handle) terminalRefs.current.set(tab.id, handle); else terminalRefs.current.delete(tab.id); }} sessionId={tab.sessionId} active={active && workspaceView === "terminal"} onTransportError={() => markError(tab.id, tab.connectionAttemptId)} /></div></div>} />
        </section>; })}
      </div>
      {/* ContextPanel owns its open/collapsed rail states; it must stay mounted
        so the collapsed rail remains clickable after hide. */}
      {tabs.length > 0 && <ContextPanel profile={activeProfile ?? selectedProfile} sessionId={activeTab?.sessionId ?? null} state={activeTab?.state ?? "idle"} connected={Boolean(activeTab?.sessionId)} onNewTerminal={() => activeTab && !activeTab.sessionId ? openReconnectDialog(activeTab.id) : openConnectDialog()} onDisconnect={() => { if (activeTab) void disconnectTab(activeTab.id); }} onEdit={() => setEditProfile(true)} onResize={fitTerminals} onSelectServer={openExplorerAndSelect} onReviewPlan={openChangeSetReview} />}
    </div>
    <footer className="terminal-status-bar" aria-label={t("a11y.connectionStatus")}><span><i className={statusColor(activeTab?.state ?? "idle")} />{t(`status.${activeTab?.state ?? "idle"}`)}</span><span>SSH</span><span>UTF-8</span><span className="status-spacer" /><span>{activeProfile?.name ?? t("terminal.noActiveSession")}</span><span className="font-mono">xterm-256color</span></footer>
    {tabMenu && <WorkspaceTabMenu menu={tabMenu} onClose={() => setTabMenu(null)} onCopy={() => { const tab = tabs.find((candidate) => candidate.id === tabMenu.tabId); setTabMenu(null); if (tab) setDialog({ mode: "connect", profileId: tab.profileId, title: tab.title }); }} onRename={() => { setRenamingTabId(tabMenu.tabId); setTabMenu(null); }} onCloseTab={() => { const tabId = tabMenu.tabId; setTabMenu(null); void closeTab(tabId); }} />}
    {renamingTabId && (() => { const tab = tabs.find((candidate) => candidate.id === renamingTabId); if (!tab) return null; const profile = profiles.find((candidate) => candidate.id === tab.profileId); return <RenameWorkspaceTabDialog initialTitle={tab.title ?? profile?.name ?? t("terminal.unknownProfile")} onClose={() => setRenamingTabId(null)} onSave={(title) => { renameTab(tab.id, title); setRenamingTabId(null); }} />; })()}
    {dialog && dialogProfile && <ConnectionDialog profile={dialogProfile} mode={dialog.mode} onClose={() => setDialog(null)} onConnect={(values) => { const tabId = dialog.tabId ?? crypto.randomUUID(); if (!dialog.tabId) setDialog({ ...dialog, tabId }); return openSession(values, tabId, dialog.mode === "reconnect", dialog.title); }} onTest={test} />}
    {editProfile && (activeProfile ?? selectedProfile) && <ProfileDialog profile={activeProfile ?? selectedProfile ?? undefined} onClose={() => setEditProfile(false)} />}
  </main>;
}
