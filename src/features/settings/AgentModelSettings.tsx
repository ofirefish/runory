import { Bot, Cloud, ExternalLink, KeyRound, LoaderCircle, Plus, RefreshCw, Zap } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { ConfirmDialog } from "../../components/ui/confirm-dialog";
import { appErrorCode } from "../../lib/app-error";
import { cloudConfigured, cloudEndpoint } from "../../lib/supabase/client";
import { cloudSession, listOrganizations } from "../../lib/supabase/cloud";
import { activateAgentModelProfile, cancelAgentModelOauth, getAgentModelProfiles, removeAgentModelProfile, saveAgentModelProfile, startAgentModelOauth, testAgentModel } from "../../lib/tauri/agentic";
import type { ModelConfigureRequest, ModelProfile, OauthProvider } from "../../types/agentic";
import type { Organization } from "../../types/cloud";
import { ModelProfileForm } from "./ModelProfileForm";
import { ModelProfileList } from "./ModelProfileList";
import { QuickProviderLogo } from "./QuickProviderLogo";
import { SettingsEditorSheet } from "./SettingsEditorSheet";
import { providerLabelKeys } from "./model-provider-presets";
import "./model-settings.css";

type Page = { type: "list" } | { type: "quick" } | { type: "form"; profile: ModelProfile | null };

export function AgentModelSettings() {
  const { t } = useTranslation();
  const [profiles, setProfiles] = useState<ModelProfile[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [page, setPage] = useState<Page>({ type: "list" });
  const [busy, setBusy] = useState(false);
  const [oauthProvider, setOauthProvider] = useState<OauthProvider | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tested, setTested] = useState(false);
  const [removing, setRemoving] = useState<ModelProfile | null>(null);
  const [managedOrganizations, setManagedOrganizations] = useState<Organization[]>([]);
  const [managedOrganizationId, setManagedOrganizationId] = useState("");
  const refresh = async () => {
    setError(null);
    try { setProfiles(await getAgentModelProfiles()); setLoaded(true); }
    catch (error) { setError(appErrorCode(error)); }
  };
  useEffect(() => { void refresh(); }, []);
  useEffect(() => {
    if (page.type !== "quick" || !cloudConfigured) return;
    void (async () => {
      try {
        if (!await cloudSession()) return;
        const organizations = await listOrganizations();
        setManagedOrganizations(organizations);
        setManagedOrganizationId((current) => current || organizations.find((item) => item.kind === "personal")?.id || organizations[0]?.id || "");
      } catch {
        setManagedOrganizations([]);
        setManagedOrganizationId("");
      }
    })();
  }, [page.type]);
  const run = async (action: () => Promise<void>) => {
    setBusy(true); setError(null); setTested(false);
    try { await action(); } catch (error) { const code = appErrorCode(error); if (code !== "MODEL_OAUTH_CANCELLED") setError(code); }
    finally { setBusy(false); }
  };
  const save = (request: ModelConfigureRequest) => run(async () => {
    setProfiles(await saveAgentModelProfile(page.type === "form" ? page.profile?.id ?? null : null, request));
    setPage({ type: "list" });
  });
  const connect = (provider: OauthProvider) => run(async () => {
    setOauthProvider(provider);
    try {
      await startAgentModelOauth(provider);
      setProfiles(await getAgentModelProfiles());
      setPage({ type: "list" });
    } finally { setOauthProvider(null); }
  });
  const changePage = (page: Page) => { setPage(page); setError(null); setTested(false); };
  const active = profiles.find((profile) => profile.active);
  return <div className="provider-settings" aria-busy={busy}>
    <>
      <div className="model-list-toolbar">
        <div className="model-list-count"><h4>{t("settings.models.saved")}</h4><span>{profiles.length}</span></div>
        <div className="model-list-add"><Button size="sm" variant="secondary" disabled={busy || !loaded} onClick={() => changePage({ type: "quick" })}><Zap size={14} />{t("settings.models.quickAdd")}</Button><Button size="sm" disabled={busy || !loaded} onClick={() => changePage({ type: "form", profile: null })}><Plus size={14} />{t("settings.models.addApiKey")}</Button></div>
      </div>
      {!loaded ? <div className="model-list-empty"><LoaderCircle size={24} className={error ? undefined : "animate-spin"} /><p>{t(error ? "settings.modelError" : "common.loading")}</p>{error && <Button size="sm" variant="secondary" onClick={() => void refresh()}>{t("settings.models.retry")}</Button>}</div>
        : profiles.length === 0 ? <div className="model-list-empty"><span className="model-empty-icon"><Bot size={28} /></span><h4>{t("settings.models.empty")}</h4><p>{t("settings.models.emptyHint")}</p><Button size="sm" onClick={() => changePage({ type: "quick" })}><Plus size={14} />{t("settings.models.addFirst")}</Button></div>
          : <ModelProfileList profiles={profiles} busy={busy} onActivate={(profile) => void run(async () => { setProfiles(await activateAgentModelProfile(profile.id)); })} onEdit={(profile) => changePage({ type: "form", profile })} onRemove={setRemoving} />}
      {loaded && profiles.length > 0 && <div className="model-list-footer"><p>{t("settings.models.singleActive")}</p>{active && <Button size="sm" variant="ghost" disabled={busy || !active.apiKeyConfigured} onClick={() => void run(async () => { await testAgentModel(); setTested(true); })}><RefreshCw size={13} className={busy ? "animate-spin" : undefined} />{t("settings.testModel")}</Button>}</div>}
    </>
    {page.type !== "list" && <SettingsEditorSheet title={t(page.type === "quick" ? "settings.models.quickAdd" : page.profile ? "settings.models.edit" : "settings.models.addApiKey")} description={t(page.type === "quick" ? "settings.models.quickHint" : "settings.models.formHint")} closeDisabled={busy} onClose={() => changePage({ type: "list" })}>
      {page.type === "form" ? <ModelProfileForm profile={page.profile} busy={busy} onSave={save} onCancel={() => changePage({ type: "list" })} /> : <div className="model-quick-options">
        <div className="model-managed-quick">
          <Button variant="ghost" className="model-quick-option h-auto" type="button" disabled={busy || !cloudEndpoint || !managedOrganizationId} onClick={() => void save({ kind: "runory-managed", name: "", baseUrl: cloudEndpoint ?? "", model: "runory-agent-fast", maxContextTokens: 128000, organizationId: managedOrganizationId || null, apiKey: null })}><span className="model-profile-icon" data-provider="runory-managed"><Cloud size={22} /></span><span><strong>{t("settings.modelProvider.runoryManaged")}</strong><small>{t("settings.provider.runoryManagedAccount")}</small></span><Plus size={17} /></Button>
          {managedOrganizations.length > 0 ? <label className="model-managed-workspace"><span>{t("settings.provider.billingWorkspace")}</span><select value={managedOrganizationId} onChange={(event) => setManagedOrganizationId(event.target.value)}>{managedOrganizations.map((organization) => <option key={organization.id} value={organization.id}>{organization.kind === "personal" ? t("cloud.personalWorkspace") : organization.name}</option>)}</select></label> : <small>{t(cloudConfigured ? "settings.provider.runoryManagedSignIn" : "settings.provider.runoryManagedUnavailable")}</small>}
        </div>
        {(["chat-gpt", "open-router"] as const).map((provider) => <Button variant="ghost" key={provider} className="model-quick-option h-auto" type="button" disabled={busy} onClick={() => void connect(provider)}><span className="model-profile-icon" data-provider={provider}><QuickProviderLogo provider={provider} /></span><span><strong>{t(providerLabelKeys[provider])}</strong><small>{t(provider === "chat-gpt" ? "settings.provider.chatgptAccount" : "settings.provider.openrouterAccount")}</small></span>{oauthProvider === provider ? <LoaderCircle size={18} className="animate-spin" /> : <ExternalLink size={17} />}</Button>)}
        {oauthProvider && <div className="model-oauth-progress" role="status"><p>{t("settings.oauthInProgress")}</p><Button size="sm" variant="secondary" onClick={() => void cancelAgentModelOauth().catch((error: unknown) => setError(appErrorCode(error)))}>{t("common.cancel")}</Button></div>}
        <Button type="button" variant="ghost" className="model-quick-manual h-auto" disabled={busy} onClick={() => changePage({ type: "form", profile: null })}><KeyRound size={14} />{t("settings.models.useApiKey")}</Button>
      </div>}
      {error && <p role="alert" className="mt-3 text-xs text-red-500">{t(error === "VAULT_LOCKED" ? "settings.models.vaultLocked" : error === "MODEL_AUTH_FAILED" ? "settings.models.authFailed" : "settings.modelError")}</p>}
    </SettingsEditorSheet>}
    {error && loaded && page.type === "list" && <p role="alert" className="text-xs text-red-500">{t(error === "VAULT_LOCKED" ? "settings.models.vaultLocked" : error === "MODEL_AUTH_FAILED" ? "settings.models.authFailed" : "settings.modelError")}</p>}
    {tested && <p role="status" className="text-xs text-emerald-600">{t("settings.modelTestSucceeded")}</p>}
    {removing && <ConfirmDialog title={t("settings.models.removeTitle")} description={t("settings.models.removeHint", { name: removing.name || t(providerLabelKeys[removing.kind]) })} onClose={() => setRemoving(null)} onConfirm={async () => { setProfiles(await removeAgentModelProfile(removing.id)); setTested(false); }} />}
  </div>;
}
