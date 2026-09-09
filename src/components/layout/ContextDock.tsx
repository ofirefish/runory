import { FolderClosed, LayoutDashboard, PanelRightOpen, Rocket, ServerCog, SquareTerminal } from "lucide-react";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { DashboardView } from "../../features/dashboard/DashboardView";
import { DeploymentView } from "../../features/deployment/DeploymentView";
import { FilesView } from "../../features/files/FilesView";
import { OperationsView } from "../../features/operations/OperationsView";
import { useContextPanelStore } from "../context-panel/context-panel-store";
import type { WorkspaceView } from "./workspace-view";

export function ContextDock({ view, terminalContent, sessionId, profileId, active, onViewChange }: {
  view: WorkspaceView;
  terminalContent: (toolbarHost: HTMLDivElement | null) => ReactNode;
  sessionId: string | null;
  profileId: string;
  active: boolean;
  onViewChange: (view: WorkspaceView) => void;
}) {
  const { t } = useTranslation();
  const [terminalToolbarHost, setTerminalToolbarHost] = useState<HTMLDivElement | null>(null);
  const panelVisible = useContextPanelStore((store) => store.visible);
  /** Keep the collapsed Context Panel reachable in the fixed tab-strip actions. */
  const showContextPanel = () => useContextPanelStore.getState().setVisible(true);
  const tabs: { id: WorkspaceView; icon: typeof SquareTerminal; label: string }[] = [
    { id: "terminal", icon: SquareTerminal, label: t("terminal.title") },
    { id: "files", icon: FolderClosed, label: t("files.title") },
    { id: "dashboard", icon: LayoutDashboard, label: t("dashboard.title") },
    { id: "operations", icon: ServerCog, label: t("operations.title") },
    { id: "deployment", icon: Rocket, label: t("deployment.title") },
  ];
  return <section className="context-dock context-dock-top" aria-label={t("dock.title")}>
    <header className="dock-tabs"><div className="dock-view-tabs">{tabs.map((tab) => <button key={tab.id} type="button" className={view === tab.id ? "active" : ""} onClick={() => onViewChange(tab.id)} aria-current={view === tab.id ? "page" : undefined}><tab.icon size={14} />{tab.label}</button>)}</div>
      <div className="dock-actions">
        {!panelVisible && <button type="button" className="dock-panel-restore" aria-label={t("contextPanel.show")} title={t("contextPanel.show")} onClick={showContextPanel}><PanelRightOpen size={16} /></button>}
        <div ref={setTerminalToolbarHost} hidden={!active || view !== "terminal"} />
      </div>
    </header>
    <div className="dock-content">
      <div className={view === "terminal" ? "h-full" : "hidden"} aria-hidden={view !== "terminal"}>{terminalContent(terminalToolbarHost)}</div>
      {view === "files" && <FilesView sessionId={sessionId} profileId={profileId} active={active && view === "files"} />}
      {view === "dashboard" && <DashboardView sessionId={sessionId} active={active && view === "dashboard"} />}
      {view === "operations" && <OperationsView sessionId={sessionId} active={active && view === "operations"} />}
      {view === "deployment" && <DeploymentView sessionId={sessionId} profileId={profileId} />}
    </div>
  </section>;
}
