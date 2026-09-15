import { CirclePlus, SquareTerminal } from "lucide-react";
import { lazy, Suspense, type MouseEvent, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import type { TerminalHandle } from "../../features/terminal/TerminalView";
import { TerminalView } from "../../features/terminal/TerminalView";
import { bastionConnectFlow } from "../../lib/tauri/bastion";
import { connectSsh, disconnectSsh, reconnectSsh, testSsh } from "../../lib/tauri/ssh";
import { useCatalogStore } from "../../stores/catalog-store";
import { useSessionStore } from "../../stores/session-store";
import type { ConnectRequest, SessionState } from "../../types/session";
import { Button } from "../ui/button";
import { ContextDock } from "./ContextDock";
import { restoreWorkspaceView } from "./workspace-view";
import { ContextPanel } from "../context-panel/ContextPanel";
import { useContextPanelStore } from "../context-panel/context-panel-store";
import { WorkspaceTabs } from "./WorkspaceTabs";
import { RenameWorkspaceTabDialog } from "./RenameWorkspaceTabDialog";
import { WorkspaceTabMenu, type WorkspaceTabMenuState } from "./WorkspaceTabMenu";
import { useTunnelDisconnect } from "../../features/tunnels/use-tunnel-disconnect";

const BastionConnectionDialog = lazy(() => import("../../features/sessions/BastionConnectionDialog").then((module) => ({ default: module.BastionConnectionDialog })));
const ConnectionDialog = lazy(() => import("../../features/sessions/ConnectionDialog").then((module) => ({ default: module.ConnectionDialog })));
const JumpConnectionDialog = lazy(() => import("../../features/sessions/JumpConnectionDialog").then((module) => ({ default: module.JumpConnectionDialog })));
const ProfileDialog = lazy(() => import("../../features/profiles/ProfileDialog").then((module) => ({ default: module.ProfileDialog })));

type DialogState = { mode: "connect" | "reconnect"; profileId: string; tabId?: string; title?: string };
const statusColor = (state: SessionState) => state === "connected" ? "bg-emerald-500" : state === "error" ? "bg-red-500" : ["connecting", "verifying-host", "authenticating", "opening-shell"].includes(state) ? "bg-blue-500" : "bg-slate-500";

export function Workspace({ visible = true, onSelectServer, titlebarTabsHost, onShowSessions, connectProfileRequest }: { visible?: boolean; onSelectServer: () => void; titlebarTabsHost: HTMLDivElement | null; onShowSessions: () => void; connectProfileRequest?: { profileId: string; requestId: number } | null }) {
  const { t } = useTranslation();
  const [dialog, setDialog] = useState<DialogState | null>(null);
  const [editProfile, setEditProfile] = useState(false);
  const [tabMenu, setTabMenu] = useState<WorkspaceTabMenuState | null>(null);
  const [renamingTabId, setRenamingTabId] = useState<string | null>(null);
  const tunnelDisconnect = useTunnelDisconnect();
  const toggleContextPanel = useContextPanelStore((store) => store.setVisible);
  const terminalRefs = useRef(new Map<string, TerminalHandle>());
  const { profiles, selectedProfileId, load } = useCatalogStore();
  const { tabs, activeTabId, addTab, beginReconnect, attachSession, setState, markClosed, markError, setActive, setView, renameTab, removeTab } = useSessionStore();
  const selectedProfile = profiles.find((profile) => profile.id === selectedProfileId) ?? null;
  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? null;
  const activeProfile = profiles.find((profile) => profile.id === activeTab?.profileId) ?? null;
  const activeConnectionRoute = activeProfile?.connectionRoute;
  const activeJumpProfile = activeConnectionRoute?.type === "jumpHost"
    ? profiles.find((profile) => profile.id === activeConnectionRoute.profileId) ?? null
    : null;
  const dialogProfile = profiles.find((profile) => profile.id === dialog?.profileId) ?? null;
  const dialogRoute = dialogProfile?.connectionRoute;
  const dialogJumpProfile = dialogRoute?.type === "jumpHost"
    ? profiles.find((profile) => profile.id === dialogRoute.profileId) ?? null
    : null;

  useEffect(() => {
    if (!visible) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.shiftKey && event.key.toLowerCase() === "b") { event.preventDefault(); toggleContextPanel(!useContextPanelStore.getState().visible); }
    };
    window.addEventListener("keydown", onKeyDown); return () => window.removeEventListener("keydown", onKeyDown);
  }, [toggleContextPanel, visible]);
  useEffect(() => {
    if (!connectProfileRequest) return;
    const existing = useSessionStore.getState().tabs.find((tab) => tab.profileId === connectProfileRequest.profileId && tab.state === "connected" && tab.sessionId);
    if (existing) useSessionStore.getState().setActive(existing.id);
    else setDialog({ mode: "connect", profileId: connectProfileRequest.profileId });
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

  const disconnectTab = async (tabId: string) => tunnelDisconnect.request(tabId, async () => {
    const tab = useSessionStore.getState().tabs.find((candidate) => candidate.id === tabId);
    if (!tab?.sessionId) return;
    try { await disconnectSsh(tab.sessionId); } finally { markClosed(tab.id, tab.connectionAttemptId); }
  });
  const closeTab = async (tabId: string) => tunnelDisconnect.request(tabId, async () => {
    const tab = useSessionStore.getState().tabs.find((candidate) => candidate.id === tabId);
    if (tab?.sessionId) { try { await disconnectSsh(tab.sessionId); } catch { /* Rust may already have removed a remotely closed session. */ } }
    terminalRefs.current.delete(tabId);
    removeTab(tabId);
  });
  const openConnectDialog = () => { const profile = activeProfile ?? selectedProfile; if (profile) setDialog({ mode: "connect", profileId: profile.id }); };
  const openReconnectDialog = (tabId: string) => { const tab = tabs.find((candidate) => candidate.id === tabId); if (tab) setDialog({ mode: "reconnect", profileId: tab.profileId, tabId }); };
  const openTabMenu = (event: MouseEvent, tabId: string) => {
    event.preventDefault();
    setActive(tabId);
    onShowSessions();
    const width = 176;
    const height = 112;
    setTabMenu({ tabId, x: Math.max(8, Math.min(event.clientX, window.innerWidth - width - 8)), y: Math.max(8, Math.min(event.clientY, window.innerHeight - height - 8)) });
  };
  const test = async (values: Omit<ConnectRequest, "cols" | "rows">) => (await testSsh(values)).credentialSaved;
  /** Right Panel width changes trigger xterm fit so the terminal never
   *  stays mis-sized when the Inspector / Agent panel resizes or collapses. */
  const fitTerminals = () => {
    window.setTimeout(() => { window.dispatchEvent(new Event("resize")); }, 0);
  };

  return <main style={visible ? undefined : { display: "none" }} aria-hidden={!visible} aria-label={t("shell.sessions")} className="workspace-shell flex min-w-0 flex-1 flex-col bg-[hsl(var(--background))]">
    {tunnelDisconnect.dialog}
    {titlebarTabsHost && createPortal(<WorkspaceTabs tabs={tabs} profiles={profiles} activeTabId={visible ? activeTabId : null} onSelect={(tabId) => { setActive(tabId); onShowSessions(); }} onClose={(tabId) => void closeTab(tabId)} onContextMenu={openTabMenu} />, titlebarTabsHost)}
    <div className="workspace-stage">
      <div className="workspace-main-column">
        {tabs.length === 0 && <div className="empty-workspace"><span className="empty-terminal-icon"><SquareTerminal size={24} /></span><strong>{t("terminal.readyTitle")}</strong><span>{t("terminal.ready")}</span><Button size="sm" onClick={onSelectServer}><CirclePlus size={15} />{t("contextPanel.selectServer")}</Button></div>}
        {tabs.map((tab) => { const active = tab.id === activeTabId; const view = restoreWorkspaceView(tab.view); return <section key={tab.id} className={`session-workspace ${active ? "active" : ""}`} aria-hidden={!active}>
          <ContextDock view={view} sessionId={tab.sessionId} profileId={tab.profileId} active={visible && active} onViewChange={(nextView) => setView(tab.id, nextView)} terminalContent={(toolbarHost) => <div className="terminal-panel"><div className="terminal-panel-content"><TerminalView ref={(handle) => { if (handle) terminalRefs.current.set(tab.id, handle); else terminalRefs.current.delete(tab.id); }} toolbarHost={toolbarHost} sessionId={tab.sessionId} active={visible && active && view === "terminal"} onTransportError={() => markError(tab.id, tab.connectionAttemptId)} /></div></div>} />
        </section>; })}
      </div>
      {/* ContextPanel owns its open/collapsed rail states; it must stay mounted
        so the collapsed rail remains clickable after hide. */}
      {tabs.length > 0 && <ContextPanel profile={activeProfile ?? selectedProfile} jumpProfile={activeJumpProfile} sessionId={activeTab?.sessionId ?? null} state={activeTab?.state ?? "idle"} connected={Boolean(activeTab?.sessionId)} onNewTerminal={() => activeTab && !activeTab.sessionId ? openReconnectDialog(activeTab.id) : openConnectDialog()} onDisconnect={() => { if (activeTab) void disconnectTab(activeTab.id); }} onEdit={() => setEditProfile(true)} onResize={fitTerminals} onSelectServer={onSelectServer} />}
    </div>
    <footer className="terminal-status-bar" aria-label={t("a11y.connectionStatus")}><span><i className={statusColor(activeTab?.state ?? "idle")} />{t(`status.${activeTab?.state ?? "idle"}`)}</span><span>SSH</span><span>UTF-8</span><span className="status-spacer" /><span>{activeProfile?.name ?? t("terminal.noActiveSession")}</span><span className="font-mono">xterm-256color</span></footer>
    {tabMenu && <WorkspaceTabMenu menu={tabMenu} onClose={() => setTabMenu(null)} onCopy={() => { const tab = tabs.find((candidate) => candidate.id === tabMenu.tabId); setTabMenu(null); if (tab) setDialog({ mode: "connect", profileId: tab.profileId, title: tab.title }); }} onRename={() => { setRenamingTabId(tabMenu.tabId); setTabMenu(null); }} onCloseTab={() => { const tabId = tabMenu.tabId; setTabMenu(null); void closeTab(tabId); }} />}
    {renamingTabId && (() => { const tab = tabs.find((candidate) => candidate.id === renamingTabId); if (!tab) return null; const profile = profiles.find((candidate) => candidate.id === tab.profileId); return <RenameWorkspaceTabDialog initialTitle={tab.title ?? profile?.name ?? t("terminal.unknownProfile")} onClose={() => setRenamingTabId(null)} onSave={(title) => { renameTab(tab.id, title); setRenamingTabId(null); }} />; })()}
    {dialog && dialogProfile && <Suspense fallback={null}>{dialogProfile.connectionRoute.type === "bastion"
      ? <BastionConnectionDialog
          key={dialogProfile.id}
          profile={dialogProfile}
          mode={dialog.mode}
          onClose={() => setDialog(null)}
          onOpenSession={async (flowId, _provider, verificationAttemptId, sshPasswordCredential) => {
            const tabId = dialog.tabId ?? crypto.randomUUID();
            if (!dialog.tabId) setDialog({ ...dialog, tabId });
            const connectionAttemptId = crypto.randomUUID();
            const tabExists = useSessionStore.getState().tabs.some((tab) => tab.id === tabId);
            if (tabExists) beginReconnect(tabId, connectionAttemptId);
            else addTab({ id: tabId, profileId: dialogProfile.id, title: dialog.title, sessionId: null, connectionAttemptId, state: "connecting", view: "terminal" });
            const size = terminalRefs.current.get(tabId)?.dimensions() ?? { cols: 120, rows: 34 };
            try {
              const response = await bastionConnectFlow(
                flowId,
                dialogProfile.id,
                size.cols,
                size.rows,
                (event) => {
                  if (event.event === "output") terminalRefs.current.get(tabId)?.write(event.data.bytes);
                  else if (event.event === "state") setState(tabId, connectionAttemptId, event.data.state);
                  else if (event.data.reason !== "terminal-closed") markClosed(tabId, connectionAttemptId);
                },
                verificationAttemptId,
                sshPasswordCredential,
              );
              attachSession(tabId, connectionAttemptId, response.sessionId);
              void load();
              return true;
            } catch (error) {
              setState(tabId, connectionAttemptId, "error");
              throw error;
            }
          }}
        />
      : dialogProfile.connectionRoute.type === "jumpHost" && dialogJumpProfile
      ? <JumpConnectionDialog key={`${dialogProfile.id}:${dialogJumpProfile.id}`} profile={dialogProfile} jumpProfile={dialogJumpProfile} mode={dialog.mode} onClose={() => setDialog(null)} onConnect={(values) => { const tabId = dialog.tabId ?? crypto.randomUUID(); if (!dialog.tabId) setDialog({ ...dialog, tabId }); return openSession(values, tabId, dialog.mode === "reconnect", dialog.title); }} onTest={test} />
      : <ConnectionDialog key={dialogProfile.id} profile={dialogProfile} mode={dialog.mode} onClose={() => setDialog(null)} onConnect={(values) => { const tabId = dialog.tabId ?? crypto.randomUUID(); if (!dialog.tabId) setDialog({ ...dialog, tabId }); return openSession(values, tabId, dialog.mode === "reconnect", dialog.title); }} onTest={test} />}</Suspense>}
    {editProfile && (activeProfile ?? selectedProfile) && <Suspense fallback={null}><ProfileDialog profile={activeProfile ?? selectedProfile ?? undefined} onClose={() => setEditProfile(false)} /></Suspense>}
  </main>;
}
