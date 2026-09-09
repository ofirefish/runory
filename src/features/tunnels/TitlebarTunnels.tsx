import { ArrowDown, ArrowUp, ArrowUpRight, Cable, Network } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Popover, PopoverContent, PopoverTrigger } from "../../components/ui/popover";
import { useCatalogStore } from "../../stores/catalog-store";
import { localEndpoint, targetEndpoint } from "../../types/tunnels";
import { useTunnels } from "./use-tunnels";
import { useTunnelSummaryTraffic } from "./use-tunnel-summary-traffic";
import "./titlebar-tunnels.css";

export function TitlebarTunnels({ onOpenTunnels }: { onOpenTunnels: () => void }) {
  const { items, loading, error } = useTunnels();
  const { t, i18n } = useTranslation();
  const profiles = useCatalogStore((state) => state.profiles);
  const [open, setOpen] = useState(false);
  const unavailable = loading || !!error;
  const traffic = useTunnelSummaryTraffic(items, unavailable);
  const running = items.filter(({ status }) => status.state === "running");
  const visible = !unavailable && running.length > 0;
  useEffect(() => { if (!visible) setOpen(false); }, [visible]);
  if (unavailable || running.length === 0) return null;
  const connections = running.reduce((sum, { status }) => sum + status.activeConnections, 0);
  const formatRate = (bytes: number) => {
    const unit = bytes >= 1048576 ? "mib" : bytes >= 1024 ? "kib" : "byte";
    const value = bytes / (unit === "mib" ? 1048576 : unit === "kib" ? 1024 : 1);
    return t(`tunnels.summary.rate.${unit}`, { value: value.toLocaleString(i18n.language, { maximumFractionDigits: unit === "byte" ? 0 : 1 }) });
  };
  const active = traffic.sent > 0 || traffic.received > 0;
  return <div className="titlebar-tunnels" data-no-drag>
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button type="button" className="tunnel-summary-trigger" aria-label={t("tunnels.summary.open", { count: running.length })} title={t("tunnels.summary.open", { count: running.length })} data-sending={traffic.sent > 0} data-receiving={traffic.received > 0}>
          <span className="tunnel-summary-symbol" aria-hidden="true"><Cable size={15} /><i /></span>
          <span className="tunnel-summary-count">{running.length.toLocaleString(i18n.language)}</span>
          <svg className="tunnel-summary-border" viewBox="0 0 48 30" preserveAspectRatio="none" aria-hidden="true" focusable="false">
            <rect className="tunnel-summary-border-send" x="0.5" y="0.5" width="47" height="29" rx="6.5" pathLength="100" vectorEffect="non-scaling-stroke" />
            <rect className="tunnel-summary-border-receive" x="0.5" y="0.5" width="47" height="29" rx="6.5" pathLength="100" vectorEffect="non-scaling-stroke" />
          </svg>
        </button>
      </PopoverTrigger>
      <PopoverContent className="tunnel-summary-popover" align="end" sideOffset={9} aria-label={t("tunnels.summary.title")}>
        <header className="tunnel-summary-header"><span><Cable size={16} aria-hidden="true" />{t("tunnels.summary.title")}</span><span className="tunnel-summary-status" data-active={active}>{t(active ? "tunnels.summary.transferring" : "tunnels.summary.idle")}</span></header>
        <dl className="tunnel-summary-metrics">
          <div><dt>{t("tunnels.listeners")}</dt><dd>{running.length.toLocaleString(i18n.language)}</dd></div>
          <div><dt>{t("tunnels.connections")}</dt><dd>{connections.toLocaleString(i18n.language)}</dd></div>
          <div><dt><ArrowUp size={12} aria-hidden="true" />{t("tunnels.summary.upload")}</dt><dd>{formatRate(traffic.sent)}</dd></div>
          <div><dt><ArrowDown size={12} aria-hidden="true" />{t("tunnels.summary.download")}</dt><dd>{formatRate(traffic.received)}</dd></div>
        </dl>
        <ul className="tunnel-summary-list">{running.map(({ rule, status }) => <li key={rule.id}>
          <div className="tunnel-summary-rule"><strong title={rule.name}>{rule.name}</strong><span title={t("tunnels.connections")}><Network size={12} aria-hidden="true" /><span className="sr-only">{t("tunnels.connections")}: </span>{status.activeConnections.toLocaleString(i18n.language)}</span></div>
          <p className="tunnel-summary-host">{profiles.find((profile) => profile.id === rule.profileId)?.name ?? t("tunnels.missingProfile")}</p>
          <div className="tunnel-summary-route"><code title={localEndpoint(rule)}>{localEndpoint(rule)}</code><ArrowUpRight size={13} aria-hidden="true" /><code title={targetEndpoint(rule)}>{targetEndpoint(rule)}</code></div>
          {status.health === "unreachable" && <p className="tunnel-summary-warning">{t("tunnels.health.unreachable")}</p>}
        </li>)}</ul>
        <footer className="tunnel-summary-footer"><span>{t("tunnels.summary.sampled")}</span><button type="button" onClick={() => { setOpen(false); onOpenTunnels(); }}>{t("tunnels.summary.manage")}<ArrowUpRight size={13} aria-hidden="true" /></button></footer>
      </PopoverContent>
    </Popover>
  </div>;
}
