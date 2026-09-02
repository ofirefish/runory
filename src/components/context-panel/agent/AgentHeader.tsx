import { History, Pause, Play, SquarePen } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AgentRunStateV2 } from "../../../types/agent-v2";
import type { ServerProfile } from "../../../types/domain";
import type { SessionState } from "../../../types/session";

const CONNECTING: SessionState[] = ["connecting", "verifying-host", "authenticating", "opening-shell"];

function ConnectionStatus({ connected, state }: { connected: boolean; state: SessionState }) {
  const { t } = useTranslation();
  if (connected) return <span className="agent-header-status connected"><i />{t("status.connected")}</span>;
  if (CONNECTING.includes(state)) return <span className="agent-header-status connecting"><i />{t("status.connecting")}</span>;
  return <span className="agent-header-status disconnected"><i />{t("status.disconnected")}</span>;
}

function RunStatusBadge({ runState, running }: { runState: AgentRunStateV2 | null; running: boolean }) {
  const { t } = useTranslation();
  if (!running && !runState) return null;
  const key = running
    ? "contextPanel.timeline.running"
    : runState === "awaiting_approval"
      ? "contextPanel.timeline.awaitingApproval"
      : runState === "awaiting_user"
        ? "contextPanel.timeline.awaitingUser"
        : runState === "paused"
          ? "contextPanel.timeline.paused"
          : null;
  if (!key) return null;
  return <span className="agent-header-run-status">{t(key)}</span>;
}

/**
 * Compact Agent context header: always bound to the current server.
 * Secondary line shows server name · connection state. No multi-server selector.
 */
export function AgentHeader({ profile, connected, state, runState, running, onNewConversation, onOpenHistory, onCancel, onPause, onResume }: {
  profile: ServerProfile | null;
  connected: boolean;
  state: SessionState;
  runState?: AgentRunStateV2 | null;
  running?: boolean;
  onNewConversation: () => void;
  onOpenHistory: () => void;
  onCancel?: () => void;
  onPause?: () => void;
  onResume?: () => void;
}) {
  const { t } = useTranslation();
  return <header className="agent-header">
    <div className="agent-header-main">
      <div className="agent-header-title">
        <strong>{t("contextPanel.agentName")}</strong>
        <ConnectionStatus connected={connected} state={state} />
        <RunStatusBadge runState={runState ?? null} running={Boolean(running)} />
      </div>
      <p className="agent-header-context">{profile ? `${profile.name} · ${profile.username}@${profile.host}` : t("contextPanel.noServer")}</p>
    </div>
    <div className="agent-header-actions">
      {runState === "paused" && onResume && <button type="button" className="agent-icon-button" aria-label={t("contextPanel.resume")} title={t("contextPanel.resume")} onClick={onResume}><Play size={15} /></button>}
      {running && onPause && <button type="button" className="agent-icon-button" aria-label={t("contextPanel.pause")} title={t("contextPanel.pause")} onClick={onPause}><Pause size={15} /></button>}
      {running && onCancel && <button type="button" className="agent-icon-button" aria-label={t("contextPanel.stop")} title={t("contextPanel.stop")} onClick={onCancel}>{t("contextPanel.stop")}</button>}
      <button type="button" className="agent-icon-button" aria-label={t("contextPanel.history")} title={t("contextPanel.history")} onClick={onOpenHistory}><History size={15} /></button>
      <button type="button" className="agent-icon-button" aria-label={t("contextPanel.newConversation")} title={t("contextPanel.newConversation")} onClick={onNewConversation}><SquarePen size={15} /></button>
    </div>
  </header>;
}