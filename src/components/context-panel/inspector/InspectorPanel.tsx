import { ArrowRight, Edit3, Laptop, Plug, Power, Server, SquareTerminal } from "lucide-react";
import { useTranslation } from "react-i18next";
import { JumpHostIndicator } from "../../../features/profiles/JumpHostIndicator";
import type { ServerProfile } from "../../../types/domain";
import type { SessionState } from "../../../types/session";
import { Button } from "../../ui/button";

/**
 * Inspector tab content — the previous ContextInspector body, now living
 * inside the shared right Context Panel (context.inspector styles reused).
 */
export function InspectorPanel({ profile, jumpProfile, state, connected, onNewTerminal, onDisconnect, onEdit }: {
  profile: ServerProfile | null;
  jumpProfile: ServerProfile | null;
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
  const endpoint = (item: ServerProfile) => `${item.username}@${item.host}:${item.port}`;
  const jumpName = jumpProfile?.name ?? t("inspector.jumpHostUnavailable");
  const jumpIndicatorLabel = jumpProfile ? t("profile.jumpHostIndicatorNamed", { name: jumpProfile.name }) : t("profile.jumpHostIndicator");
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
    {profile.connectionRoute.type === "jumpHost" && <section className="inspector-jump-route"><h3>{t("profile.connectionRoute")}</h3>
      <div className="inspector-route-summary"><JumpHostIndicator className="inspector-route-mark" label={jumpIndicatorLabel} size={16} /><div><strong>{t("profile.routeJumpHost")}</strong><p>{t("inspector.jumpRouteDescription", { jumpHost: jumpName, target: profile.name })}</p></div></div>
      <div className="inspector-route-diagram" role="img" aria-label={t("inspector.jumpRouteDiagramLabel", { jumpHost: jumpName, target: profile.name })}>
        <div className="inspector-route-node"><span><Laptop size={16} aria-hidden="true" /></span><small>{t("inspector.routeOrigin")}</small><strong>{t("inspector.thisDevice")}</strong></div>
        <ArrowRight className="inspector-route-arrow" size={15} aria-hidden="true" />
        <div className="inspector-route-node inspector-route-node-jump"><JumpHostIndicator label={jumpIndicatorLabel} /><small>{t("profile.jumpHost")}</small><strong title={jumpName}>{jumpName}</strong><code title={jumpProfile ? endpoint(jumpProfile) : undefined}>{jumpProfile ? endpoint(jumpProfile) : "—"}</code></div>
        <ArrowRight className="inspector-route-arrow" size={15} aria-hidden="true" />
        <div className="inspector-route-node"><span><Server size={16} aria-hidden="true" /></span><small>{t("inspector.routeTarget")}</small><strong title={profile.name}>{profile.name}</strong><code title={endpoint(profile)}>{endpoint(profile)}</code></div>
      </div>
    </section>}
    <section><h3>{t("inspector.quickActions")}</h3><div className="quick-actions">
      <Button variant="secondary" onClick={onNewTerminal}>{connected ? <SquareTerminal size={15} /> : <Plug size={15} />}{connected ? t("terminal.newTab") : t("terminal.connect")}</Button>
      {connected && <Button variant="secondary" onClick={onDisconnect}><Power size={15} />{t("terminal.disconnect")}</Button>}
      <Button variant="secondary" onClick={onEdit}><Edit3 size={15} />{t("profile.editTitle")}</Button>
    </div></section>
  </div>;
}
