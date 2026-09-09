import { Check, LockKeyhole, Plus, RefreshCw, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import type { Session } from "@supabase/supabase-js";
import { cloudConfigured } from "../../lib/supabase/client";
import { cloudSession, createOrganization, ensureMyCloudProfile, ensurePersonalWorkspace, listOrganizations, loadEncryptedInventory, onCloudAuthStateChange, writeEncryptedInventory } from "../../lib/supabase/cloud";
import { applyCloudSync, cloudSyncKeyStatus, discardCloudSync, exportCloudSync, forgetCloudSyncKey, previewCloudSync, rotateCloudRecoveryPassphrase } from "../../lib/tauri/cloud";
import { lockCloudPolicy } from "../../lib/tauri/cloud-policy";
import { useCatalogStore } from "../../stores/catalog-store";
import type { CloudEncryptedPayload, CloudImportPreview, CloudSyncKeyStatus, Organization } from "../../types/cloud";
import { CloudTeamPanel } from "./CloudTeamPanel";
import { CloudGovernancePanel } from "./CloudGovernancePanel";
import { SettingsEditorSheet } from "./SettingsEditorSheet";
import { buildConflictDecisions, conflictKey, hasCloudSyncChanges } from "./cloud-sync";

type CloudFeatureTab = "sync" | "team" | "governance";

export function CloudPanel({ onOpenAccount = () => undefined }: { onOpenAccount?: () => void }) {
  const { t } = useTranslation();
  const syncPassphrase = useRef<HTMLInputElement>(null);
  const newRecoveryPassphrase = useRef<HTMLInputElement>(null);
  const recoveryPassphraseConfirmation = useRef<HTMLInputElement>(null);
  const pendingImportId = useRef<string | null>(null);
  const pendingRemoteRevision = useRef<number | null>(null);
  const [session, setSession] = useState<Session | null>(null);
  const [organizations, setOrganizations] = useState<Organization[]>([]);
  const [organizationName, setOrganizationName] = useState("");
  const [creatingOrganization, setCreatingOrganization] = useState(false);
  const [selectedOrganizationId, setSelectedOrganizationId] = useState<string | null>(null);
  const [preview, setPreview] = useState<CloudImportPreview | null>(null);
  const [conflictDecisions, setConflictDecisions] = useState<Record<string, "keepLocal" | "useRemote">>({});
  const [syncStatus, setSyncStatus] = useState<"idle" | "syncing" | "reviewRequired" | "complete" | "error">("idle");
  const [lastSyncAt, setLastSyncAt] = useState<Date | null>(null);
  const [governanceRevision, setGovernanceRevision] = useState(0);
  const [busy, setBusy] = useState(false);
  const [syncValidationError, setSyncValidationError] = useState<string | null>(null);
  const [syncKey, setSyncKey] = useState<CloudSyncKeyStatus | null>(null);
  const [activeFeatureTab, setActiveFeatureTab] = useState<CloudFeatureTab>("sync");
  const [changingRecoveryPassphrase, setChangingRecoveryPassphrase] = useState(false);
  const [recoveryPassphraseValidationError, setRecoveryPassphraseValidationError] = useState<string | null>(null);
  const userId = session?.user.id ?? null;
  const selectedOrganization = organizations.find((item) => item.id === selectedOrganizationId) ?? null;
  const showCloudError = useCallback(() => {
    toast.error(t("cloud.error"), { id: "cloud-operation-error" });
  }, [t]);

  const loadOrganizations = useCallback(async () => {
    const values = await listOrganizations();
    setOrganizations(values);
    setSelectedOrganizationId((current) => values.some((item) => item.id === current)
      ? current
      : (values.find((item) => item.kind === "personal")?.id ?? values[0]?.id ?? null));
  }, []);
  const bootstrapAccount = useCallback(async (nextSession: Session) => {
    const fallbackName = nextSession.user.email?.split("@")[0]?.slice(0, 64) || nextSession.user.id.slice(0, 8);
    await Promise.all([ensureMyCloudProfile(fallbackName), ensurePersonalWorkspace()]);
    setSession(nextSession);
    await loadOrganizations();
  }, [loadOrganizations]);
  useEffect(() => {
    if (!cloudConfigured) return;
    void cloudSession().then((session) => {
      if (session) void bootstrapAccount(session).catch(showCloudError);
    }).catch(showCloudError);
  }, [bootstrapAccount, showCloudError]);
  useEffect(() => {
    if (!cloudConfigured) return;
    const subscription = onCloudAuthStateChange((event, session) => {
      if (event === "SIGNED_OUT") {
        setSession(null);
        pendingRemoteRevision.current = null;
        setSyncStatus("idle");
        queueMicrotask(() => void lockCloudPolicy());
      } else if (session && (event === "SIGNED_IN" || event === "TOKEN_REFRESHED" || event === "INITIAL_SESSION")) {
        setSession(session);
        queueMicrotask(() => void (async () => {
          if (event === "SIGNED_IN") await bootstrapAccount(session);
        })().catch(showCloudError));
      }
    });
    return () => subscription.unsubscribe();
  }, [bootstrapAccount, showCloudError]);
  useEffect(() => () => {
    if (pendingImportId.current) void discardCloudSync(pendingImportId.current);
  }, []);
  useEffect(() => {
    if (!selectedOrganizationId) {
      setSyncKey(null);
      return;
    }
    setSyncKey(null);
    let active = true;
    void cloudSyncKeyStatus(selectedOrganizationId)
      .then((status) => { if (active) setSyncKey(status); })
      .catch(() => { if (active) showCloudError(); });
    return () => { active = false; };
  }, [selectedOrganizationId, showCloudError]);

  const addOrganization = async () => {
    if (!userId || !organizationName.trim()) return;
    setBusy(true);
    try {
      const created = await createOrganization(organizationName.trim(), userId);
      setOrganizations((current) => [...current, created]);
      setSelectedOrganizationId(created.id);
      setOrganizationName("");
      setCreatingOrganization(false);
      toast.success(t("cloud.workspaceCreated"));
    } catch { showCloudError(); } finally { setBusy(false); }
  };
  const runSync = async (operation: (organizationId: string, recoveryPassphrase?: string) => Promise<void>) => {
    const organizationId = selectedOrganizationId;
    if (!organizationId) return;
    const value = syncPassphrase.current?.value;
    if (!syncKey?.configured && !value) {
      setSyncValidationError("cloud.syncPassphraseRequired");
      syncPassphrase.current?.focus();
      return;
    }
    if (!syncKey?.configured && value && value.length < 12) {
      setSyncValidationError("cloud.syncPassphraseTooShort");
      syncPassphrase.current?.focus();
      return;
    }
    setSyncValidationError(null);
    setBusy(true); setSyncStatus("syncing");
    try {
      await operation(organizationId, syncKey?.configured ? undefined : value);
      setSyncKey(await cloudSyncKeyStatus(organizationId));
    } catch { showCloudError(); setSyncStatus("error"); }
    finally { if (syncPassphrase.current) syncPassphrase.current.value = ""; setBusy(false); }
  };
  const syncNow = () => runSync(async (organizationId, recoveryPassphrase) => {
    if (pendingImportId.current) await discardCloudSync(pendingImportId.current);
    pendingImportId.current = null;
    const remote = await loadEncryptedInventory(organizationId);
    if (!remote) {
      const payload = await exportCloudSync(organizationId, recoveryPassphrase);
      await writeEncryptedInventory(organizationId, payload, 0);
      pendingRemoteRevision.current = null;
      setPreview(null); setConflictDecisions({}); setSyncStatus("complete"); setLastSyncAt(new Date());
      toast.success(t("cloud.syncComplete"));
      return;
    }
    const payload = remote.encrypted_payload as CloudEncryptedPayload;
    const nextPreview = await previewCloudSync(organizationId, payload, recoveryPassphrase);
    if (!hasCloudSyncChanges(nextPreview)) {
      await discardCloudSync(nextPreview.importId);
      pendingImportId.current = null;
      pendingRemoteRevision.current = null;
      setConflictDecisions({});
      setPreview(null);
      setSyncStatus("complete");
      setLastSyncAt(new Date());
      toast.success(t("cloud.syncComplete"));
      return;
    }
    pendingImportId.current = nextPreview.importId;
    pendingRemoteRevision.current = remote.revision;
    setConflictDecisions({});
    setPreview(nextPreview);
    setSyncStatus("reviewRequired");
  });
  const applyPreview = async () => {
    const organizationId = selectedOrganizationId;
    const expectedRevision = pendingRemoteRevision.current;
    if (!organizationId || !preview || expectedRevision === null) return;
    setBusy(true); setSyncStatus("syncing");
    try {
      const decisions = buildConflictDecisions(preview.conflictItems, conflictDecisions);
      await applyCloudSync(preview.importId, decisions);
      pendingImportId.current = null;
      await useCatalogStore.getState().load();
      const mergedPayload = await exportCloudSync(organizationId);
      await writeEncryptedInventory(organizationId, mergedPayload, expectedRevision);
      pendingRemoteRevision.current = null;
      setPreview(null); setConflictDecisions({}); setSyncStatus("complete"); setLastSyncAt(new Date());
      toast.success(t("cloud.syncComplete"));
    } catch {
      if (!pendingImportId.current) {
        pendingRemoteRevision.current = null;
        setPreview(null);
        setConflictDecisions({});
      }
      showCloudError(); setSyncStatus("error");
    } finally {
      setBusy(false);
    }
  };
  const forgetSyncKey = async () => {
    if (!selectedOrganizationId) return;
    setBusy(true);
    try {
      await forgetCloudSyncKey(selectedOrganizationId);
      setSyncKey(await cloudSyncKeyStatus(selectedOrganizationId));
      setChangingRecoveryPassphrase(false); setRecoveryPassphraseValidationError(null);
      toast.success(t("cloud.syncKeyForgotten"));
    } catch { showCloudError(); } finally { setBusy(false); }
  };
  const rotateRecoveryPassphrase = async () => {
    const organizationId = selectedOrganizationId;
    const nextPassphrase = newRecoveryPassphrase.current?.value ?? "";
    const confirmation = recoveryPassphraseConfirmation.current?.value ?? "";
    if (!organizationId) return;
    if (!nextPassphrase) {
      setRecoveryPassphraseValidationError("cloud.syncPassphraseRequired");
      newRecoveryPassphrase.current?.focus();
      return;
    }
    if (nextPassphrase.length < 12) {
      setRecoveryPassphraseValidationError("cloud.syncPassphraseTooShort");
      newRecoveryPassphrase.current?.focus();
      return;
    }
    if (nextPassphrase !== confirmation) {
      setRecoveryPassphraseValidationError("cloud.recoveryPassphraseMismatch");
      recoveryPassphraseConfirmation.current?.focus();
      return;
    }
    setBusy(true); setRecoveryPassphraseValidationError(null);
    try {
      const remote = await loadEncryptedInventory(organizationId);
      const payload = (remote?.encrypted_payload as CloudEncryptedPayload | undefined)
        ?? await exportCloudSync(organizationId);
      const rotatedPayload = await rotateCloudRecoveryPassphrase(organizationId, payload, nextPassphrase);
      await writeEncryptedInventory(organizationId, rotatedPayload, remote?.revision ?? 0);
      setSyncKey(await cloudSyncKeyStatus(organizationId));
      if (newRecoveryPassphrase.current) newRecoveryPassphrase.current.value = "";
      if (recoveryPassphraseConfirmation.current) recoveryPassphraseConfirmation.current.value = "";
      setChangingRecoveryPassphrase(false);
      toast.success(t("cloud.recoveryPassphraseChanged"));
    } catch { showCloudError(); } finally { setBusy(false); }
  };
  const discardPreview = async () => {
    if (!preview) return;
    setBusy(true);
    try { await discardCloudSync(preview.importId); pendingImportId.current = null; pendingRemoteRevision.current = null; setPreview(null); setConflictDecisions({}); setSyncStatus("idle"); } catch { showCloudError(); setSyncStatus("error"); } finally { setBusy(false); }
  };
  const selectOrganization = async (organizationId: string) => {
    if (pendingImportId.current) {
      try { await discardCloudSync(pendingImportId.current); } catch { showCloudError(); return; }
    }
    pendingImportId.current = null;
    pendingRemoteRevision.current = null;
    const nextOrganization = organizations.find((item) => item.id === organizationId);
    setPreview(null); setConflictDecisions({}); setSyncStatus("idle"); setLastSyncAt(null); setSyncKey(null); setSelectedOrganizationId(organizationId);
    setChangingRecoveryPassphrase(false); setRecoveryPassphraseValidationError(null);
    if (nextOrganization?.kind !== "team" && activeFeatureTab === "team") setActiveFeatureTab("sync");
  };
  const refreshGovernance = async () => {
    await loadOrganizations();
    setGovernanceRevision((value) => value + 1);
  };
  const navigateFeatureTabs = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    const tabs: CloudFeatureTab[] = selectedOrganization?.kind === "team"
      ? ["sync", "team", "governance"]
      : ["sync", "governance"];
    const currentIndex = Math.max(0, tabs.indexOf(activeFeatureTab));
    const nextIndex = event.key === "Home" ? 0
      : event.key === "End" ? tabs.length - 1
        : (currentIndex + (event.key === "ArrowRight" ? 1 : -1) + tabs.length) % tabs.length;
    const nextTab = tabs[nextIndex];
    event.preventDefault();
    setActiveFeatureTab(nextTab);
    document.getElementById(`cloud-feature-tab-${nextTab}`)?.focus();
  };

  return <section className="mt-4 border-t pt-4">
    {!cloudConfigured && <><p className="mt-1 text-xs text-[hsl(var(--muted))]">{t("cloud.localOnly")}</p><p className="mt-2 text-xs text-amber-600">{t("cloud.configureHint")}</p></>}
    {cloudConfigured && !userId && <div className="mt-3 rounded-md border p-3"><p className="text-xs text-[hsl(var(--muted))]">{t("cloud.signInRequired")}</p><Button className="mt-3" type="button" size="sm" variant="secondary" onClick={onOpenAccount}>{t("cloud.openAccountSettings")}</Button></div>}
    {cloudConfigured && userId && session && <div className="mt-3"><h4 className="text-xs font-medium">{t("cloud.organizations")}</h4><p className="mt-1 text-xs text-[hsl(var(--muted))]">{t("cloud.workspaceHint")}</p><div className="mt-2 grid gap-2">{organizations.map((organization) => <Button type="button" variant="ghost" aria-pressed={selectedOrganizationId === organization.id} key={organization.id} onClick={() => void selectOrganization(organization.id)} className={`h-auto justify-start whitespace-normal rounded border p-2 text-left text-xs ${selectedOrganizationId === organization.id ? "border-[hsl(var(--primary))]" : ""}`}>{organization.kind === "personal" ? t("cloud.personalWorkspace") : organization.name}</Button>)}</div><Button className="mt-2" size="sm" variant="secondary" onClick={() => setCreatingOrganization(true)}><Plus size={15} />{t("cloud.createTeamWorkspace")}</Button>
      {selectedOrganizationId && <div className="mt-4 border-t pt-3">
        <div className="inline-flex h-9 w-fit items-center justify-center rounded-lg bg-[hsl(var(--elevated))] p-[3px] text-[hsl(var(--muted))]" role="tablist" aria-label={t("cloud.featureTabs")} onKeyDown={navigateFeatureTabs}>
          {(["sync", "team", "governance"] as const).map((tab) => {
            const disabled = tab === "team" && selectedOrganization?.kind !== "team";
            return <Button key={tab} id={`cloud-feature-tab-${tab}`} type="button" role="tab" variant="ghost" size="sm" aria-selected={activeFeatureTab === tab} aria-controls={`cloud-feature-panel-${tab}`} aria-disabled={disabled} disabled={disabled} tabIndex={activeFeatureTab === tab ? 0 : -1} className="h-[calc(100%-1px)] flex-none rounded-md border border-transparent px-3 text-xs font-medium text-[hsl(var(--muted))] shadow-none data-[state=active]:border-[hsl(var(--border))] data-[state=active]:bg-[hsl(var(--surface))] data-[state=active]:text-[hsl(var(--foreground))] data-[state=active]:shadow-sm" data-state={activeFeatureTab === tab ? "active" : "inactive"} onClick={() => setActiveFeatureTab(tab)}>{t(tab === "sync" ? "cloud.syncTab" : `cloud.${tab}`)}</Button>;
          })}
        </div>
        {activeFeatureTab === "sync" && <div id="cloud-feature-panel-sync" role="tabpanel" aria-labelledby="cloud-feature-tab-sync" className="pt-3"><h4 className="text-xs font-medium">{t("cloud.syncTitle")}</h4><p className="mt-1 text-xs text-[hsl(var(--muted))]">{t("cloud.syncHint")}</p><div className="mt-2 rounded border px-3 py-2 text-xs"><div className="flex items-center justify-between gap-2"><span>{t("cloud.syncStatusLabel")}</span><span>{t(`cloud.syncStatus.${syncStatus}`)}</span></div><div className="mt-1 flex items-center justify-between gap-2 text-[hsl(var(--muted))]"><span>{t("cloud.lastSync")}</span><span>{lastSyncAt ? new Intl.DateTimeFormat(undefined, { dateStyle: "short", timeStyle: "short" }).format(lastSyncAt) : t("cloud.neverSynced")}</span></div></div>{syncKey?.configured ? <div className="mt-2 rounded border px-3 py-2 text-xs"><p className="flex items-center gap-2"><LockKeyhole size={14} />{t(syncKey.persistedOnDevice ? "cloud.syncKeyDeviceProtected" : "cloud.syncKeySessionOnly")}</p><div className="mt-2 flex flex-wrap gap-2"><Button type="button" size="sm" variant="secondary" disabled={busy || Boolean(preview)} onClick={() => setChangingRecoveryPassphrase(true)}>{t("cloud.changeRecoveryPassphrase")}</Button><Button type="button" size="sm" variant="ghost" disabled={busy || Boolean(preview)} onClick={() => void forgetSyncKey()}>{t("cloud.forgetSyncKey")}</Button></div></div> : <><Input ref={syncPassphrase} className="mt-2" type="password" autoComplete="new-password" required minLength={12} placeholder={t("cloud.recoveryPassphrase")} aria-label={t("cloud.recoveryPassphrase")} aria-invalid={Boolean(syncValidationError)} onInput={() => setSyncValidationError(null)} /><p className="mt-1 text-xs text-[hsl(var(--muted))]">{t(syncKey?.secureStorageAvailable === false ? "cloud.recoveryPassphraseSessionHint" : "cloud.recoveryPassphraseHint")}</p>{syncValidationError && <p role="alert" className="mt-1 text-xs text-red-500">{t(syncValidationError)}</p>}</>}<div className="mt-2 flex gap-2"><Button size="sm" disabled={busy || syncKey === null || Boolean(preview) || changingRecoveryPassphrase} onClick={() => void syncNow()}><RefreshCw size={14} />{t("cloud.syncNow")}</Button></div>
        {preview && <div className="mt-3 rounded border p-3 text-xs"><p>{t("cloud.previewCounts", { groups: preview.groupAdditions + preview.groupUpdates, profiles: preview.profileAdditions + preview.profileUpdates })}</p><p>{t("cloud.previewDeletes", { groups: preview.groupDeletions, profiles: preview.profileDeletions })}</p><p className="mt-1 text-amber-600">{t("cloud.previewConflicts", { local: preview.localNewer, conflicts: preview.conflicts })}</p><p className="mt-2 text-[hsl(var(--muted))]">{t("cloud.applyKeyHint")}</p>{preview.conflictItems.length > 0 && <div className="mt-2 grid gap-2">{preview.conflictItems.map((item) => { const key = conflictKey(item); const resolution = conflictDecisions[key] ?? "keepLocal"; return <div key={key} className="rounded border p-2"><p className="truncate">{item.label} · {t(item.remoteDeleted ? "cloud.remoteDeleted" : "cloud.remoteChanged")}</p><div className="mt-1 flex gap-1"><Button size="sm" variant={resolution === "keepLocal" ? "default" : "secondary"} onClick={() => setConflictDecisions((current) => ({ ...current, [key]: "keepLocal" }))}>{t("cloud.keepLocal")}</Button><Button size="sm" variant={resolution === "useRemote" ? "default" : "secondary"} onClick={() => setConflictDecisions((current) => ({ ...current, [key]: "useRemote" }))}>{t("cloud.useRemote")}</Button></div></div>; })}</div>}<div className="mt-2 flex gap-2"><Button size="sm" disabled={busy} onClick={() => void applyPreview()}><Check size={14} />{t("cloud.applyAndPublish")}</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => void discardPreview()}><X size={14} />{t("common.cancel")}</Button></div></div>}
        </div>}
        {activeFeatureTab === "team" && selectedOrganization?.kind === "team" && <div id="cloud-feature-panel-team" role="tabpanel" aria-labelledby="cloud-feature-tab-team" className="pt-3"><CloudTeamPanel organizationId={selectedOrganizationId} userId={userId} onMembershipChanged={refreshGovernance} /></div>}
        {activeFeatureTab === "governance" && <div id="cloud-feature-panel-governance" role="tabpanel" aria-labelledby="cloud-feature-tab-governance" className="pt-3"><CloudGovernancePanel key={`${selectedOrganizationId}:${governanceRevision}`} organizationId={selectedOrganizationId} userId={userId} /></div>}
      </div>}
    </div>}
    {creatingOrganization && <SettingsEditorSheet title={t("cloud.createTeamWorkspace")} description={t("cloud.workspaceHint")} closeDisabled={busy} onClose={() => { setOrganizationName(""); setCreatingOrganization(false); }}>
      <form className="grid gap-4" onSubmit={(event) => { event.preventDefault(); void addOrganization(); }}>
        <Input autoFocus value={organizationName} onChange={(event) => setOrganizationName(event.target.value)} placeholder={t("cloud.organizationName")} aria-label={t("cloud.organizationName")} />
        <div className="flex justify-end gap-2"><Button type="button" variant="ghost" disabled={busy} onClick={() => { setOrganizationName(""); setCreatingOrganization(false); }}>{t("common.cancel")}</Button><Button type="submit" disabled={busy || !organizationName.trim()}>{t("cloud.createOrganization")}</Button></div>
      </form>
    </SettingsEditorSheet>}
    {changingRecoveryPassphrase && <SettingsEditorSheet title={t("cloud.changeRecoveryPassphrase")} description={t("cloud.recoveryPassphraseHint")} closeDisabled={busy} onClose={() => { setChangingRecoveryPassphrase(false); setRecoveryPassphraseValidationError(null); }}>
      <form className="grid gap-3" onSubmit={(event) => { event.preventDefault(); void rotateRecoveryPassphrase(); }}>
        <Input ref={newRecoveryPassphrase} autoFocus type="password" autoComplete="new-password" required minLength={12} placeholder={t("cloud.newRecoveryPassphrase")} aria-label={t("cloud.newRecoveryPassphrase")} aria-invalid={Boolean(recoveryPassphraseValidationError)} onInput={() => setRecoveryPassphraseValidationError(null)} />
        <Input ref={recoveryPassphraseConfirmation} type="password" autoComplete="new-password" required minLength={12} placeholder={t("cloud.confirmRecoveryPassphrase")} aria-label={t("cloud.confirmRecoveryPassphrase")} aria-invalid={recoveryPassphraseValidationError === "cloud.recoveryPassphraseMismatch"} onInput={() => setRecoveryPassphraseValidationError(null)} />
        {recoveryPassphraseValidationError && <p role="alert" className="text-xs text-red-500">{t(recoveryPassphraseValidationError)}</p>}
        <div className="flex justify-end gap-2"><Button type="button" variant="ghost" disabled={busy} onClick={() => { setChangingRecoveryPassphrase(false); setRecoveryPassphraseValidationError(null); }}>{t("common.cancel")}</Button><Button type="submit" disabled={busy}>{t("cloud.saveRecoveryPassphrase")}</Button></div>
      </form>
    </SettingsEditorSheet>}
  </section>;
}
