import { Server, Settings, SquareTerminal } from "lucide-react";
import { useTranslation } from "react-i18next";

export function PrimaryNavigationRail({ active, onSelect }: { active: "servers" | "sessions" | "settings"; onSelect: (item: "servers" | "sessions" | "settings") => void }) {
  const { t } = useTranslation();
  const items = [
    { id: "servers" as const, icon: Server, label: t("sidebar.servers") },
    { id: "sessions" as const, icon: SquareTerminal, label: t("shell.sessions") },
  ];
  return <nav className="primary-rail" aria-label={t("shell.primaryNavigation")}>
    <div className="rail-items">{items.map((item) => <button key={item.id} type="button" className={active === item.id ? "active" : ""} onClick={() => onSelect(item.id)} aria-current={active === item.id ? "page" : undefined} title={item.label}><item.icon size={20} /><span>{item.label}</span></button>)}</div>
    <div className="rail-footer"><button type="button" className={active === "settings" ? "active" : ""} onClick={() => onSelect("settings")} aria-current={active === "settings" ? "page" : undefined} title={t("sidebar.settings")}><Settings size={20} /><span>{t("sidebar.settings")}</span></button></div>
  </nav>;
}
