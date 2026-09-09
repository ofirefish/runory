import { Cable, Server, Settings } from "lucide-react";
import { useTranslation } from "react-i18next";
import { UserMenu } from "./UserMenu";

export function PrimaryNavigationRail({ active, onSelect, onOpenAccount, onOpenAuth, onOpenPricing, mobileOpen = false }: { mobileOpen?: boolean; active: "servers" | "sessions" | "tunnels"; onSelect: (item: "servers" | "tunnels" | "settings") => void; onOpenAccount: () => void; onOpenAuth: () => void; onOpenPricing: () => void }) {
  const { t } = useTranslation();
  const items = [
    { id: "servers" as const, icon: Server, label: t("sidebar.servers") },
    { id: "tunnels" as const, icon: Cable, label: t("tunnels.nav") },
  ];
  return <nav className={`primary-rail ${mobileOpen ? "mobile-open" : ""}`} aria-label={t("shell.primaryNavigation")}>
    <div className="rail-items">{items.map((item) => <button key={item.id} type="button" className={active === item.id ? "active" : ""} onClick={() => onSelect(item.id)} aria-current={active === item.id ? "page" : undefined} title={item.label}><item.icon size={20} /><span>{item.label}</span></button>)}</div>
    <div className="rail-footer">
      <button type="button" onClick={() => onSelect("settings")} aria-haspopup="dialog" title={t("sidebar.settings")}><Settings size={20} /><span>{t("sidebar.settings")}</span></button>
      <UserMenu onOpenAccount={onOpenAccount} onOpenAuth={onOpenAuth} onOpenSettings={() => onSelect("settings")} onOpenPricing={onOpenPricing} />
    </div>
  </nav>;
}
