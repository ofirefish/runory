import { Cable, Server, SquarePen, SquareTerminal, Trash2, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import type { ServerProfile } from "../../types/domain";

export function ServerProfileDetails({ profile, groupName, onConnect, onEdit, onDelete, onClose, onCreateTunnel }: {
  profile: ServerProfile | null;
  groupName: string;
  onConnect: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onClose?: () => void;
  onCreateTunnel?: () => void;
}) {
  const { t } = useTranslation();
  if (!profile) return <section className="server-profile-details server-profile-empty"><Server size={28} aria-hidden="true" /><h2>{t("serverManagement.selectTitle")}</h2><p>{t("serverManagement.selectDescription")}</p></section>;
  const rows = [
    [t("profile.host"), profile.host],
    [t("profile.port"), String(profile.port)],
    [t("profile.username"), profile.username],
    [t("profile.group"), groupName],
    [t("profile.authMethod"), t(profile.authMethod === "password" ? "profile.password" : "profile.privateKey")],
  ];
  rows.push([
    t("profile.connectionRoute"),
    profile.connectionRoute.type === "jumpHost" ? t("profile.routeJumpHost") : t("profile.routeDirect"),
  ]);
  if (profile.keySource) rows.push([t("profile.keyStorage"), t(profile.keySource.type === "file" ? "profile.keyFile" : "profile.keyVault")]);
  return <section className="server-profile-details" aria-label={t("serverManagement.details")}>
    {onClose && <div className="server-details-toolbar"><span>{t("serverManagement.details")}</span><Button variant="ghost" size="icon" aria-label={t("serverManagement.closeDetails")} title={t("serverManagement.closeDetails")} onClick={onClose}><X size={16} /></Button></div>}
    <header><span className="server-profile-icon"><Server size={22} aria-hidden="true" /></span><div><h2>{profile.name}</h2><p className="font-mono">{profile.username}@{profile.host}:{profile.port}</p></div></header>
    <div className="server-profile-actions">
      <Button size="sm" onClick={onConnect}><SquareTerminal size={15} />{t("serverManagement.openSession")}</Button>
      <Button size="sm" variant="secondary" onClick={onEdit}><SquarePen size={15} />{t("common.edit")}</Button>
      {onCreateTunnel && <Button size="sm" variant="secondary" onClick={onCreateTunnel}><Cable size={15} />{t("tunnels.create")}</Button>}
      <Button size="sm" variant="ghost" onClick={onDelete}><Trash2 size={15} />{t("common.delete")}</Button>
    </div>
    <h3>{t("serverManagement.details")}</h3>
    <dl>{rows.map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>
  </section>;
}
