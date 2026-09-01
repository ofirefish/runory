import { History, SquarePen } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ServerProfile } from "../../../types/domain";
import type { SessionState } from "../../../types/session";

const CONNECTING: SessionState[] = ["connecting", "verifying-host", "authenticating", "opening-shell"];

function ConnectionStatus({ connected, state }: { connected: boolean; state: SessionState }) {
  const { t } = useTranslation();
  if (connected) return <span className="agent-header-status connected"><i />{t("status.connected")}</span>;
  if (CONNECTING.includes(state)) return <span className="agent-header-status connecting"><i />{t("status.connecting")}</span>;
  return <span className="agent-header-status disconnected"><i />{t("status.disconnected")}</span>;
}

/**
 * Compact Agent context header: always bound to the current server.
 * Secondary line shows server name · connection state. No multi-server selector.
 */
export function AgentHeader({ profile, connected, state, onNewConversation, onOpenHistory }: {
  profile: ServerProfile | null;
  connected: boolean;
  state: SessionState;
  onNewConversation: () => void;
  onOpenHistory: () => void;
}) {
  const { t } = useTranslation();
  return <header className="agent-header">
    <div className="agent-header-main">
      <div className="agent-header-title">
        <strong>{t("contextPanel.agentName")}</strong>
        <ConnectionStatus connected={connected} state={state} />
      </div>
      <p className="agent-header-context">{profile ? `${profile.name} · ${profile.username}@${profile.host}` : t("contextPanel.noServer")}</p>
    </div>
    <div className="agent-header-actions">
      <button type="button" className="agent-icon-button" aria-label={t("contextPanel.history")} title={t("contextPanel.history")} onClick={onOpenHistory}><History size={15} /></button>
      <button type="button" className="agent-icon-button" aria-label={t("contextPanel.newConversation")} title={t("contextPanel.newConversation")} onClick={onNewConversation}><SquarePen size={15} /></button>
    </div>
  </header>;
}