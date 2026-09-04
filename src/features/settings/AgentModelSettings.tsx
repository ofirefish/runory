import { ArrowLeft, Bot, ExternalLink, KeyRound, LoaderCircle, Plus, RefreshCw, Zap } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { ConfirmDialog } from "../../components/ui/confirm-dialog";
import { appErrorCode } from "../../lib/app-error";
import { activateAgentModelProfile, cancelAgentModelOauth, getAgentModelProfiles, removeAgentModelProfile, saveAgentModelProfile, startAgentModelOauth, testAgentModel } from "../../lib/tauri/agentic";
import type { ModelConfigureRequest, ModelProfile, OauthProvider } from "../../types/agentic";
import { ModelProfileForm } from "./ModelProfileForm";
import { ModelProfileList } from "./ModelProfileList";
import { QuickProviderLogo } from "./QuickProviderLogo";
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
  const refresh = async () => {
    setError(null);
    try { setProfiles(await getAgentModelProfiles()); setLoaded(true); }
    catch (error) { setError(appErrorCode(error)); }
  };
  useEffect(() => { void refresh(); }, []);
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
    {page.type === "list" ? <>
      <div className="model-list-toolbar">
        <div className="model-list-count"><h4>{t("settings.models.saved")}</h4><span>{profiles.length}</span></div>
        <div className="model-list-add"><Button size="sm" variant="secondary" disabled={busy || !loaded} onClick={() => changePage({ type: "quick" })}><Zap size={14} />{t("settings.models.quickAdd")}</Button><Button size="sm" disabled={busy || !loaded} onClick={() => changePage({ type: "form", profile: null })}><Plus size={14} />{t("settings.models.addApiKey")}</Button></div>
      </div>
      {!loaded ? <div className="model-list-empty"><LoaderCircle size={24} className={error ? undefined : "animate-spin"} /><p>{t(error ? "settings.modelError" : "common.loading")}</p>{error && <Button size="sm" variant="secondary" onClick={() => void refresh()}>{t("settings.models.retry")}</Button>}</div>
        : profiles.length === 0 ? <div className="model-list-empty"><span className="model-empty-icon"><Bot size={28} /></span><h4>{t("settings.models.empty")}</h4><p>{t("settings.models.emptyHint")}</p><Button size="sm" onClick={() => changePage({ type: "quick" })}><Plus size={14} />{t("settings.models.addFirst")}</Button></div>
          : <ModelProfileList profiles={profiles} busy={busy} onActivate={(profile) => void run(async () => { setProfiles(await activateAgentModelProfile(profile.id)); })} onEdit={(profile) => changePage({ type: "form", profile })} onRemove={setRemoving} />}
      {loaded && profiles.length > 0 && <div className="model-list-footer"><p>{t("settings.models.singleActive")}</p>{active && <Button size="sm" variant="ghost" disabled={busy || !active.apiKeyConfigured} onClick={() => void run(async () => { await testAgentModel(); setTested(true); })}><RefreshCw size={13} className={busy ? "animate-spin" : undefined} />{t("settings.testModel")}</Button>}</div>}
    </> : <>
      <div className="model-editor-heading"><Button size="icon" variant="ghost" disabled={busy} aria-label={t("settings.models.back")} onClick={() => changePage({ type: "list" })}><ArrowLeft size={17} /></Button><div><h4>{t(page.type === "quick" ? "settings.models.quickAdd" : page.profile ? "settings.models.edit" : "settings.models.addApiKey")}</h4><p>{t(page.type === "quick" ? "settings.models.quickHint" : "settings.models.formHint")}</p></div></div>
      {page.type === "form" ? <ModelProfileForm profile={page.profile} busy={busy} onSave={save} onCancel={() => changePage({ type: "list" })} /> : <div className="model-quick-options">
        {(["chat-gpt", "open-router"] as const).map((provider) => <Button variant="ghost" key={provider} className="model-quick-option h-auto" type="button" disabled={busy} onClick={() => void connect(provider)}><span className="model-profile-icon" data-provider={provider}><QuickProviderLogo provider={provider} /></span><span><strong>{t(providerLabelKeys[provider])}</strong><small>{t(provider === "chat-gpt" ? "settings.provider.chatgptAccount" : "settings.provider.openrouterAccount")}</small></span>{oauthProvider === provider ? <LoaderCircle size={18} className="animate-spin" /> : <ExternalLink size={17} />}</Button>)}
        {oauthProvider && <div className="model-oauth-progress" role="status"><p>{t("settings.oauthInProgress")}</p><Button size="sm" variant="secondary" onClick={() => void cancelAgentModelOauth().catch((error: unknown) => setError(appErrorCode(error)))}>{t("common.cancel")}</Button></div>}
        <Button type="button" variant="ghost" className="model-quick-manual h-auto" disabled={busy} onClick={() => changePage({ type: "form", profile: null })}><KeyRound size={14} />{t("settings.models.useApiKey")}</Button>
      </div>}
    </>}
    {error && loaded && <p role="alert" className="text-xs text-red-500">{t(error === "VAULT_LOCKED" ? "settings.models.vaultLocked" : error === "MODEL_AUTH_FAILED" ? "settings.models.authFailed" : "settings.modelError")}</p>}
    {tested && <p role="status" className="text-xs text-emerald-600">{t("settings.modelTestSucceeded")}</p>}
    {removing && <ConfirmDialog title={t("settings.models.removeTitle")} description={t("settings.models.removeHint", { name: removing.name || t(providerLabelKeys[removing.kind]) })} onClose={() => setRemoving(null)} onConfirm={async () => { setProfiles(await removeAgentModelProfile(removing.id)); setTested(false); }} />}
  </div>;
}
