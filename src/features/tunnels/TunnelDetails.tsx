import { Cable, Copy, Laptop, Network, Server, SquareTerminal, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { localEndpoint, targetEndpoint, type TunnelView } from "../../types/tunnels";
import { useTunnelTraffic } from "./use-tunnel-traffic";

export function TunnelDetails({ tunnel, profileName, busy, onClose, onCopy, onCheck, onSession }: {
  tunnel: TunnelView;
  profileName: string;
  busy: boolean;
  onClose: () => void;
  onCopy: () => void;
  onCheck: () => void;
  onSession: () => void;
}) {
  const { t, i18n } = useTranslation();
  const { rule, status } = tunnel;
  const traffic = useTunnelTraffic(rule.id, status);
  const time = (at: number | null) => at === null ? t("tunnels.notYet") : new Date(at * 1000).toLocaleString(i18n.language);
  const rows = [
    [t("tunnels.state"), t(`tunnels.state.${status.state}`)],
    [t("tunnels.health"), t(`tunnels.health.${status.health}`)],
    [t("tunnels.checkedAt"), time(status.checkedAt)],
    [t("tunnels.startedAt"), time(status.startedAt)],
    [t("tunnels.connections"), status.activeConnections.toLocaleString(i18n.language)],
    [t("tunnels.sent"), t("tunnels.bytes", { count: status.bytesSent })],
    [t("tunnels.received"), t("tunnels.bytes", { count: status.bytesReceived })],
  ];
  return <aside className="tunnel-details" aria-label={t("tunnels.details")}>
    <div className="tunnel-details-header"><span>{t("tunnels.details")}</span><Button variant="ghost" size="icon" onClick={onClose} aria-label={t("a11y.close")} title={t("a11y.close")}><X size={16} /></Button></div>
    <h2><Cable size={18} aria-hidden="true" />{rule.name}</h2>
    <ol className="tunnel-route" aria-label={t("tunnels.preview")} data-state={status.state} data-sending={traffic.sending} data-receiving={traffic.receiving}>
      <li><span className="tunnel-route-icon"><Laptop size={16} aria-hidden="true" /></span><div><span>{t("tunnels.local")}</span><code>{localEndpoint(rule)}</code></div><TunnelFlow /></li>
      <li><span className="tunnel-route-icon"><Server size={16} aria-hidden="true" /></span><div><span>{t("tunnels.via")}</span><strong>{profileName}</strong></div><TunnelFlow /></li>
      <li><span className="tunnel-route-icon"><Network size={16} aria-hidden="true" /></span><div><span>{t("tunnels.target")}</span><code>{targetEndpoint(rule)}</code></div></li>
    </ol>
    <div className="tunnel-details-actions"><Button size="sm" variant="secondary" onClick={onCopy}><Copy size={14} />{t("tunnels.copy")}</Button><Button size="sm" variant="secondary" disabled={busy || status.state !== "running"} onClick={onCheck}>{t("tunnels.check")}</Button><Button size="sm" variant="ghost" onClick={onSession}><SquareTerminal size={14} />{t("tunnels.viewSession")}</Button></div>
    <dl>{rows.map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>
    <p className="tunnel-hint">{t("tunnels.healthHint")}</p>
    {status.errorCode && <p role="alert" className="tunnel-error">{t(`errors.${status.errorCode}`, { defaultValue: t("errors.UNKNOWN") })}</p>}
    <h3>{t("tunnels.events")}</h3>
    {status.events.length === 0 ? <p className="tunnel-hint">{t("tunnels.noEvents")}</p> : <ol className="tunnel-events">{status.events.slice().reverse().map((event, index) => <li key={`${event.at}-${index}`}><time>{time(event.at)}</time><span>{t(event.code === event.code.toUpperCase() ? `errors.${event.code}` : `tunnels.event.${event.code}`, { defaultValue: t("errors.UNKNOWN") })}</span></li>)}</ol>}
    <p className="tunnel-hint">{t("tunnels.lifecycleHint")}</p>
  </aside>;
}

function TunnelFlow() {
  return <span className="tunnel-flow" aria-hidden="true"><i className="tunnel-flow-send" /><i className="tunnel-flow-receive" /></span>;
}
