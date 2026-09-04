import { Bot, Check, KeyRound, Pencil, Play, ShieldCheck, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import type { ModelProfile } from "../../types/agentic";
import { providerLabelKeys } from "./model-provider-presets";

export function ModelProfileList({ profiles, busy, onActivate, onEdit, onRemove }: {
  profiles: ModelProfile[];
  busy: boolean;
  onActivate: (profile: ModelProfile) => void;
  onEdit: (profile: ModelProfile) => void;
  onRemove: (profile: ModelProfile) => void;
}) {
  const { t } = useTranslation();
  return <ul className="model-profile-list" aria-label={t("settings.models.saved")}>
    {profiles.map((profile) => {
      const name = profile.name || t(providerLabelKeys[profile.kind]);
      const oauth = profile.authMode === "oauth";
      return <li key={profile.id} className="model-profile" data-active={profile.active}>
        <span className="model-profile-icon" data-provider={profile.kind} aria-hidden="true"><Bot size={21} /></span>
        <div className="model-profile-info">
          <div className="model-profile-title"><h4 title={name}>{name}</h4><span className="model-profile-auth">{oauth ? <ShieldCheck size={11} /> : <KeyRound size={11} />}{t(oauth ? "settings.models.account" : "settings.modelApiKey")}</span></div>
          <div className="model-profile-model" title={profile.model}>{profile.model}</div>
          <div className="model-profile-endpoint" title={profile.baseUrl}>{profile.baseUrl}</div>
        </div>
        <div className="model-profile-actions">
          {profile.active ? <span className="model-profile-enabled"><Check size={13} />{t("settings.models.enabled")}</span>
            : <Button size="sm" variant="secondary" disabled={busy || !profile.apiKeyConfigured} aria-label={t("settings.models.enableNamed", { name })} onClick={() => onActivate(profile)}><Play size={12} />{t("settings.models.enable")}</Button>}
          <Button className="model-profile-icon-action" size="icon" variant="ghost" disabled={busy} title={t("settings.models.editNamed", { name })} aria-label={t("settings.models.editNamed", { name })} onClick={() => onEdit(profile)}><Pencil size={15} /></Button>
          <Button className="model-profile-icon-action" size="icon" variant="ghost" disabled={busy || profile.active} title={t(profile.active ? "settings.models.switchBeforeRemove" : "settings.models.removeNamed", { name })} aria-label={t("settings.models.removeNamed", { name })} onClick={() => onRemove(profile)}><Trash2 size={15} /></Button>
        </div>
      </li>;
    })}
  </ul>;
}
