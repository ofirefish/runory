import { FolderClosed, LayoutDashboard, ListChecks, PanelRightOpen, Rocket, ServerCog, Sparkles, SquareTerminal } from "lucide-react";
import { lazy, Suspense, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { AiTerminalView } from "../../features/ai/AiTerminalView";
import { DashboardView } from "../../features/dashboard/DashboardView";
import { DeploymentView } from "../../features/deployment/DeploymentView";
import { FilesView } from "../../features/files/FilesView";
import { OperationsView } from "../../features/operations/OperationsView";
import type { ServerProfile } from "../../types/domain";
import { useContextPanelStore } from "../context-panel/context-panel-store";

const ChangeSetWorkspace = lazy(() => import("../../features/agentic/ChangeSetWorkspace"));

/** Central workspace views. The AI Agent is NOT a view here — it lives in
 *  the right Context Panel as [Inspector] [Agent]. "changes" is the
 *  ChangeSet Review workspace opened by the Agent's Review Plan action. */
export type WorkspaceView = "terminal" | "files" | "assistant" | "changes" | "dashboard" | "operations" | "deployment";

export function ContextDock({ view, terminalContent, sessionId, profileId, profile, active, onViewChange, onInsertCommand }: {
  view: WorkspaceView;
  terminalContent: ReactNode;
  sessionId: string | null;
  profileId: string;
  profile: ServerProfile | null;
  active: boolean;
  onViewChange: (view: WorkspaceView) => void;
  onInsertCommand: (command: string) => void;
}) {
  const { t } = useTranslation();
  const panelVisible = useContextPanelStore((store) => store.visible);
  /** Restore the right Context Panel after it was hidden. Always visible at
   *  the far right of the dock tab strip so the collapsed panel stays reachable. */
  const showContextPanel = () => useContextPanelStore.getState().setVisible(true);
  const tabs: { id: WorkspaceView; icon: typeof SquareTerminal; label: string }[] = [
    { id: "terminal", icon: SquareTerminal, label: t("terminal.title") },
    { id: "files", icon: FolderClosed, label: t("files.title") },
    { id: "assistant", icon: Sparkles, label: t("ai.title") },
    { id: "changes", icon: ListChecks, label: t("agentic.tab.changes") },
    { id: "dashboard", icon: LayoutDashboard, label: t("dashboard.title") },
    { id: "operations", icon: ServerCog, label: t("operations.title") },
    { id: "deployment", icon: Rocket, label: t("deployment.title") },
  ];
  return <section className="context-dock context-dock-top" aria-label={t("dock.title")}>
    <header className="dock-tabs">{tabs.map((tab) => <button key={tab.id} type="button" className={view === tab.id ? "active" : ""} onClick={() => onViewChange(tab.id)} aria-current={view === tab.id ? "page" : undefined}><tab.icon size={14} />{tab.label}</button>)}
      {!panelVisible && <button type="button" className="dock-panel-restore" aria-label={t("contextPanel.show")} title={t("contextPanel.show")} onClick={showContextPanel}><PanelRightOpen size={16} /></button>}
    </header>
    <div className="dock-content">
      <div className={view === "terminal" ? "h-full" : "hidden"} aria-hidden={view !== "terminal"}>{terminalContent}</div>
      {view === "files" && <FilesView sessionId={sessionId} profileId={profileId} active={active && view === "files"} />}
      {view === "assistant" && <AiTerminalView sessionId={sessionId} onInsertCommand={onInsertCommand} />}
      {view === "changes" && <Suspense fallback={<p className="p-4 text-sm text-[hsl(var(--muted))]">{t("common.loading")}</p>}>
        <ChangeSetWorkspace sessions={sessionId ? [{ sessionId, profileId, label: profile?.name ?? sessionId }] : []} activeSessionId={sessionId} agentRunId={null} />
      </Suspense>}
      {view === "dashboard" && <DashboardView sessionId={sessionId} active={active && view === "dashboard"} />}
      {view === "operations" && <OperationsView sessionId={sessionId} active={active && view === "operations"} />}
      {view === "deployment" && <DeploymentView sessionId={sessionId} profileId={profileId} />}
    </div>
  </section>;
}
