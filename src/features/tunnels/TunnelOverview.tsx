import { Activity, Cable, CircleAlert, Radio } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { TunnelView } from "../../types/tunnels";

export function TunnelOverview({ items, unavailable }: { items: TunnelView[]; unavailable: boolean }) {
  const { t, i18n } = useTranslation();
  const metrics = [
    { key: "rules", icon: Cable, value: items.length, tone: "primary" },
    { key: "listeners", icon: Radio, value: items.filter(({ status }) => status.state === "running").length, tone: "cyan" },
    { key: "connections", icon: Activity, value: items.reduce((sum, { status }) => sum + status.activeConnections, 0), tone: "teal" },
    { key: "attention", icon: CircleAlert, value: items.filter(({ status }) => status.state === "error" || status.state === "interrupted" || status.health === "unreachable").length, tone: "warning" },
  ];
  return <dl className="tunnel-overview" aria-label={t("tunnels.overview")}>
    {metrics.map(({ key, icon: Icon, value, tone }) => <div className={`tunnel-metric tunnel-metric-${tone}`} key={key}>
      <dt><Icon size={16} aria-hidden="true" />{t(`tunnels.${key}`)}</dt>
      <dd>{unavailable ? t("tunnels.notYet") : value.toLocaleString(i18n.language)}</dd>
    </div>)}
  </dl>;
}
