import { Edit3, Plug, Power, SquareTerminal } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ServerProfile } from "../../../types/domain";
import type { SessionState } from "../../../types/session";
import { Button } from "../../ui/button";

/**
 * Inspector tab content — the previous ContextInspector body, now living
 * inside the shared right Context Panel (context.inspector styles reused).
 */
export function InspectorPanel({ profile, state, connected, onNewTerminal, onDisconnect, onEdit }: {
  profile: ServerProfile | null;
  state: SessionState;
  connected: boolean;
  onNewTerminal: () => void;
  onDisconnect: () => void;
  onEdit: () => void;
}) {
  const { t } = useTranslation();
  if (!profile) {
    return <div className="context-inspector-body"><div className="inspector-empty">{t("inspector.empty")}</div></div>;
  }
  return <div className="context-inspector-body">
    <header><div><span className="eyebrow">{t("inspector.title")}</span><h2>{profile.name}</h2></div></header>
    <section><h3>{t("inspector.connectionStatus")}</h3><div className="inspector-status"><span className={`status-dot status-${state}`} /><strong>{t(`status.${state}`)}</strong></div></section>
    <section><h3>{t("inspector.information")}</h3><dl>
      <div><dt>{t("connection.host")}</dt><dd>{profile.host}</dd></div>
      <div><dt>{t("connection.port")}</dt><dd>{profile.port}</dd></div>
      <div><dt>{t("connection.username")}</dt><dd>{profile.username}</dd></div>
      <div><dt>{t("profile.authMethod")}</dt><dd>{t(profile.authMethod === "password" ? "profile.password" : "profile.privateKey")}</dd></div>
      <div><dt>{t("inspector.lastConnected")}</dt><dd>{profile.lastConnectedAt ? new Date(profile.lastConnectedAt).toLocaleString() : "—"}</dd></div>
    </dl></section>
    <section><h3>{t("inspector.quickActions")}</h3><div className="quick-actions">
      <Button variant="secondary" onClick={onNewTerminal}>{connected ? <SquareTerminal size={15} /> : <Plug size={15} />}{connected ? t("terminal.newTab") : t("terminal.connect")}</Button>
      {connected && <Button variant="secondary" onClick={onDisconnect}><Power size={15} />{t("terminal.disconnect")}</Button>}
      <Button variant="secondary" onClick={onEdit}><Edit3 size={15} />{t("profile.editTitle")}</Button>
    </div></section>
  </div>;
}