import { Eye, EyeOff, LoaderCircle, Save } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { Label } from "../../components/ui/label";
import { SettingsSelectField } from "./SettingsSelectField";
import { ModelNameInput } from "./ModelNameInput";
import type { AdvancedProviderKind, ModelConfigureRequest, ModelProfile } from "../../types/agentic";
import { advancedPresets, chatgptModels, isAdvancedKind, managedModels, providerLabelKeys } from "./model-provider-presets";

export function ModelProfileForm({ profile, busy, onSave, onCancel }: {
  profile: ModelProfile | null;
  busy: boolean;
  onSave: (request: ModelConfigureRequest) => Promise<void>;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const formId = useId();
  const [kind, setKind] = useState(profile?.kind ?? "open-ai");
  const [name, setName] = useState(profile?.name ?? "");
  const [baseUrl, setBaseUrl] = useState(profile?.baseUrl ?? advancedPresets["open-ai"].baseUrl);
  const [model, setModel] = useState(profile?.model ?? advancedPresets["open-ai"].model);
  const [maxContextTokens, setMaxContextTokens] = useState(profile?.maxContextTokens ?? advancedPresets["open-ai"].maxContextTokens);
  const [apiKey, setApiKey] = useState("");
  const [visible, setVisible] = useState(false);
  const oauth = profile?.authMode === "oauth";
  const managed = kind === "runory-managed";
  const reuseKey = profile?.apiKeyConfigured && kind === profile.kind && baseUrl.trim() === profile.baseUrl;
  const models = managed ? managedModels : kind === "chat-gpt" ? chatgptModels : isAdvancedKind(kind) ? advancedPresets[kind].models : [];
  const changeProvider = (value: AdvancedProviderKind) => {
    setKind(value);
    setBaseUrl(advancedPresets[value].baseUrl);
    setModel(advancedPresets[value].model);
    setMaxContextTokens(advancedPresets[value].maxContextTokens);
    setApiKey("");
    setVisible(false);
  };
  return <form className="model-profile-form" onSubmit={(event) => {
    event.preventDefault();
    if (busy) return;
    void onSave({ kind, name, baseUrl, model, maxContextTokens, organizationId: profile?.organizationId ?? null, apiKey: apiKey.trim() || null });
  }}>
    <fieldset disabled={busy}>
      <div className="provider-field"><Label htmlFor={`${formId}-name`}>{t("settings.modelConfigName")}</Label><Input id={`${formId}-name`} autoFocus value={name} maxLength={50} placeholder={t("settings.modelConfigNamePlaceholder")} onChange={(event) => setName(event.target.value)} /></div>
      <SettingsSelectField className="provider-field" controlClassName="provider-field-select" label={t("settings.modelProvider")} value={kind} disabled={busy || !!profile} onValueChange={(value) => { if (isAdvancedKind(value)) changeProvider(value); }} options={[
        ...(Object.keys(advancedPresets) as AdvancedProviderKind[]).map((value) => ({ value, label: t(providerLabelKeys[value]) })),
        ...(!isAdvancedKind(kind) ? [{ value: kind, label: t(providerLabelKeys[kind]) }] : []),
      ]} />
      {!oauth && !managed && <div className="provider-field"><Label htmlFor={`${formId}-url`}>{t("settings.modelBaseUrl")}</Label><Input id={`${formId}-url`} required type="url" readOnly={kind === "open-ai" || kind === "anthropic" || kind === "google"} value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} placeholder={t("settings.models.endpointPlaceholder")} /></div>}
      {!oauth && !managed && <div className="provider-field"><Label htmlFor={`${formId}-key`}>{t("settings.modelApiKey")}</Label>
        <div className="provider-api-key"><Input id={`${formId}-key`} aria-describedby={`${formId}-key-hint`} aria-label={t("settings.modelApiKey")} type={visible ? "text" : "password"} autoComplete="off" spellCheck={false} required={!reuseKey} value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder={t(reuseKey ? "settings.modelApiKeyConfigured" : "settings.modelApiKeyPaste")} />
          <Button type="button" variant="ghost" size="icon" className="provider-api-key-toggle h-7 w-7" aria-label={t(visible ? "settings.hideApiKey" : "settings.showApiKey")} aria-pressed={visible} onClick={() => setVisible(!visible)}>{visible ? <EyeOff size={16} /> : <Eye size={16} />}</Button>
        </div><span id={`${formId}-key-hint`} className="provider-field-hint">{t("settings.models.keyHint")}</span>
      </div>}
      <div className="provider-field"><Label htmlFor={`${formId}-model`}>{t("settings.modelLabel")}</Label><ModelNameInput id={`${formId}-model`} value={model} onChange={setModel} models={models} disabled={busy} /></div>
    </fieldset>
    <div className="model-form-footer"><Button variant="ghost" type="button" disabled={busy} onClick={onCancel}>{t("common.cancel")}</Button><Button type="submit" disabled={busy || !model.trim() || (!managed && !reuseKey && !apiKey.trim())}>{busy ? <LoaderCircle size={14} className="animate-spin" /> : <Save size={14} />}{t(profile ? "common.save" : "settings.models.save")}</Button></div>
  </form>;
}
