import { Bot, PanelRightClose, PanelRightOpen } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ServerProfile } from "../../types/domain";
import type { SessionState } from "../../types/session";
import { AgentPanel } from "./agent/AgentPanel";
import { ContextPanelTabs, type ContextTab } from "./ContextPanelTabs";
import { InspectorPanel } from "./inspector/InspectorPanel";
import { useContextPanelStore } from "./context-panel-store";

const MIN_WIDTH = 360;
const MAX_WIDTH = 600;
const DEFAULT_WIDTH = 420;
const WIDTH_KEY = "runory.contextPanelWidth";

function loadWidth(): number {
  try {
    const stored = Number(localStorage.getItem(WIDTH_KEY));
    if (Number.isFinite(stored) && stored > 0) return Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, stored));
  } catch { /* ignore */ }
  return DEFAULT_WIDTH;
}

/**
 * Right Context Panel: [Inspector] [Agent].
 *
 * Switching tabs never touches the server workspace: no terminal change,
 * no session change, no reconnection. Panel visibility / width / active tab
 * persist to localStorage. UI preferences only — never LLM context or secrets.
 */
export function ContextPanel({ profile, jumpProfile, sessionId, state, connected, onNewTerminal, onDisconnect, onEdit, onResize, onSelectServer }: {
  profile: ServerProfile | null;
  jumpProfile: ServerProfile | null;
  sessionId: string | null;
  state: SessionState;
  connected: boolean;
  onNewTerminal: () => void;
  onDisconnect: () => void;
  onEdit: () => void;
  /** Fired continuously during width drag so Main Workspace (xterm) can fit. */
  onResize: () => void;
  onSelectServer: () => void;
}) {
  const { t } = useTranslation();
  const open = useContextPanelStore((store) => store.visible);
  const tab = useContextPanelStore((store) => store.activeTab);
  const setOpen = useContextPanelStore((store) => store.setVisible);
  const setTab = useContextPanelStore((store) => store.setActiveTab);
  const [width, setWidth] = useState(loadWidth);
  const [agentRunning, setAgentRunning] = useState(false);

  /**
   * Agent running dot on the [Agent] tab. The AgentPanel signals active
   * runs through a tiny custom event (UI preference only — the panel
   * must not reach into the Agent orchestration layer).
   */
  useEffect(() => {
    const onRun = () => setAgentRunning(true);
    const onDone = () => setAgentRunning(false);
    window.addEventListener("runory:agent-run", onRun);
    window.addEventListener("runory:agent-done", onDone);
    return () => {
      window.removeEventListener("runory:agent-run", onRun);
      window.removeEventListener("runory:agent-done", onDone);
    };
  }, []);

  const saveWidth = (value: number) => {
    try { localStorage.setItem(WIDTH_KEY, String(value)); } catch { /* ignore */ }
  };

  const toggleOpen = () => {
    setOpen(!open);
    // Width / collapse changes cascade to xterm fit.
    window.setTimeout(() => onResize(), 0);
  };
  const openAgent = () => {
    setTab("agent");
    if (!open) setOpen(true);
  };
  const tabChanged = (next: ContextTab) => {
    setTab(next);
    window.setTimeout(() => onResize(), 0);
  };
  const beginResize = (event: React.PointerEvent<HTMLDivElement>) => {
    const startX = event.clientX;
    const startWidth = width;
    let finalWidth = startWidth;
    const move = (moveEvent: PointerEvent) => {
      finalWidth = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, startWidth + startX - moveEvent.clientX));
      setWidth(finalWidth);
      onResize();
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      saveWidth(finalWidth);
      onResize();
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop, { once: true });
  };

  return open ? (
    <aside className="context-panel" style={{ width, flexBasis: width }} aria-label={t("contextPanel.title")}>
      <div className="inspector-resize-handle" role="separator" aria-label={t("shell.resizeInspector")} aria-orientation="vertical" onPointerDown={beginResize} />
      <div className="context-panel-header-wrap">
        <ContextPanelTabs active={tab} onChange={tabChanged} running={agentRunning} />
        <button type="button" className="context-panel-toggle" aria-label={t("contextPanel.hide")} title={t("contextPanel.hide")} onClick={toggleOpen}><PanelRightClose size={16} /></button>
      </div>
      <div className="context-panel-content" id="context-panel-content">
        {tab === "inspector"
          ? <InspectorPanel profile={profile} jumpProfile={jumpProfile} state={state} connected={connected} onNewTerminal={onNewTerminal} onDisconnect={onDisconnect} onEdit={onEdit} />
          : <AgentPanel profile={profile} sessionId={sessionId} state={state} connected={connected} onNewTerminal={onNewTerminal} onSelectServer={onSelectServer} />}
      </div>
    </aside>
  ) : (
    <aside className="context-panel-collapsed" aria-label={t("contextPanel.title")}>
      <button type="button" className="collapsed-tab" aria-label={t("agent.title")} title={t("agent.title")} onClick={openAgent}><Bot size={16} /></button>
      <button type="button" className="collapsed-tab" aria-label={t("inspector.show")} title={t("inspector.show")} onClick={toggleOpen}><PanelRightOpen size={16} /></button>
    </aside>
  );
}
