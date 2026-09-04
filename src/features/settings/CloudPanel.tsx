import { Check, Cloud, DownloadCloud, LogOut, Plus, UploadCloud, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import type { Session } from "@supabase/supabase-js";
import { cloudConfigured, cloudEndpoint, cloudPublishableKey } from "../../lib/supabase/client";
import { cloudSession, cloudSignIn, cloudSignOut, cloudSignUp, createOrganization, listOrganizations, loadEncryptedInventory, onCloudAuthStateChange, writeEncryptedInventory } from "../../lib/supabase/cloud";
import { applyCloudSync, discardCloudSync, exportCloudSync, previewCloudSync } from "../../lib/tauri/cloud";
import { lockCloudPolicy, refreshCloudPolicy } from "../../lib/tauri/cloud-policy";
import { useCatalogStore } from "../../stores/catalog-store";
import type { CloudEncryptedPayload, CloudImportPreview, Organization } from "../../types/cloud";
import { CloudTeamPanel } from "./CloudTeamPanel";
import { CloudGovernancePanel } from "./CloudGovernancePanel";
import { buildConflictDecisions, conflictKey } from "./cloud-sync";

async function refreshPolicyCredentials(session: Session) {
  if (!session.expires_at || !cloudEndpoint || !cloudPublishableKey) return;
  await refreshCloudPolicy({ supabaseUrl: cloudEndpoint, publishableKey: cloudPublishableKey, accessToken: session.access_token, expiresAt: session.expires_at });
}

export function CloudPanel() {
  const { t } = useTranslation();
  const email = useRef<HTMLInputElement>(null);
  const password = useRef<HTMLInputElement>(null);
  const syncPassphrase = useRef<HTMLInputElement>(null);
  const pendingImportId = useRef<string | null>(null);
  const [userId, setUserId] = useState<string | null>(null);
  const [organizations, setOrganizations] = useState<Organization[]>([]);
  const [organizationName, setOrganizationName] = useState("");
  const [selectedOrganizationId, setSelectedOrganizationId] = useState<string | null>(null);
  const [preview, setPreview] = useState<CloudImportPreview | null>(null);
  const [conflictDecisions, setConflictDecisions] = useState<Record<string, "keepLocal" | "useRemote">>({});
  const [syncComplete, setSyncComplete] = useState(false);
  const [governanceRevision, setGovernanceRevision] = useState(0);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const [confirmationSent, setConfirmationSent] = useState(false);

  const loadOrganizations = async () => {
    const values = await listOrganizations();
    setOrganizations(values);
    setSelectedOrganizationId((current) => values.some((item) => item.id === current) ? current : (values[0]?.id ?? null));
  };
  useEffect(() => {
    if (!cloudConfigured) return;
    void cloudSession().then((session) => {
      setUserId(session?.user.id ?? null);
      if (session) void loadOrganizations().catch(() => setFailed(true));
    }).catch(() => setFailed(true));
  }, []);
  useEffect(() => {
    if (!cloudConfigured) return;
    const subscription = onCloudAuthStateChange((event, session) => {
      if (event === "SIGNED_OUT") {
        queueMicrotask(() => void lockCloudPolicy());
      } else if (session && (event === "SIGNED_IN" || event === "TOKEN_REFRESHED" || event === "INITIAL_SESSION")) {
        queueMicrotask(() => void refreshPolicyCredentials(session).catch(() => setFailed(true)));
      }
    });
    return () => subscription.unsubscribe();
  }, []);
  useEffect(() => () => {
    if (pendingImportId.current) void discardCloudSync(pendingImportId.current);
  }, []);

  const authenticate = async (signup: boolean) => {
    const emailValue = email.current?.value.trim();
    const passwordValue = password.current?.value;
    if (!emailValue || !passwordValue) return;
    setBusy(true); setFailed(false); setConfirmationSent(false);
    try {
      if (signup) {
        const user = await cloudSignUp(emailValue, passwordValue);
        setConfirmationSent(Boolean(user));
      } else {
        const session = await cloudSignIn(emailValue, passwordValue);
        await refreshPolicyCredentials(session);
        setUserId(session.user.id);
        await loadOrganizations();
      }
    } catch { setFailed(true); } finally {
      if (password.current) password.current.value = "";
      setBusy(false);
    }
  };
  const addOrganization = async () => {
    if (!userId || !organizationName.trim()) return;
    setBusy(true); setFailed(false);
    try {
      const created = await createOrganization(organizationName.trim(), userId);
      setOrganizations((current) => [...current, created]); setOrganizationName("");
    } catch { setFailed(true); } finally { setBusy(false); }
  };
  const signOut = async () => {
    setBusy(true); setFailed(false);
    try {
      if (pendingImportId.current) await discardCloudSync(pendingImportId.current);
      pendingImportId.current = null;
      await lockCloudPolicy(); await cloudSignOut(); setUserId(null); setOrganizations([]); setSelectedOrganizationId(null); setPreview(null); setConflictDecisions({});
    } catch { setFailed(true); } finally { setBusy(false); }
  };

  const withSyncPassphrase = async (operation: (organizationId: string, passphrase: string) => Promise<void>) => {
    const organizationId = selectedOrganizationId;
    const value = syncPassphrase.current?.value;
    if (!organizationId || !value || value.length < 12) return;
    setBusy(true); setFailed(false); setSyncComplete(false);
    try { await operation(organizationId, value); } catch { setFailed(true); }
    finally { if (syncPassphrase.current) syncPassphrase.current.value = ""; setBusy(false); }
  };
  const pushSync = () => withSyncPassphrase(async (organizationId, passphrase) => {
    if (pendingImportId.current) await discardCloudSync(pendingImportId.current);
    pendingImportId.current = null;
    const payload = await exportCloudSync(organizationId, passphrase);
    await writeEncryptedInventory(organizationId, payload);
    setPreview(null); setConflictDecisions({}); setSyncComplete(true);
  });
  const pullSync = () => withSyncPassphrase(async (organizationId, passphrase) => {
    const remote = await loadEncryptedInventory(organizationId);
    if (!remote) throw new Error("SYNC_NOT_FOUND");
    const payload = remote.encrypted_payload as CloudEncryptedPayload;
    if (pendingImportId.current) await discardCloudSync(pendingImportId.current);
    const nextPreview = await previewCloudSync(organizationId, passphrase, payload);
    pendingImportId.current = nextPreview.importId;
    setConflictDecisions({});
    setPreview(nextPreview);
  });
  const applyPreview = async () => {
    if (!preview) return;
    setBusy(true); setFailed(false);
    try {
      const decisions = buildConflictDecisions(preview.conflictItems, conflictDecisions);
      await applyCloudSync(preview.importId, decisions);
      await useCatalogStore.getState().load();
      pendingImportId.current = null;
      setPreview(null); setConflictDecisions({}); setSyncComplete(true);
    } catch { setFailed(true); } finally { setBusy(false); }
  };
  const discardPreview = async () => {
    if (!preview) return;
    setBusy(true); setFailed(false);
    try { await discardCloudSync(preview.importId); pendingImportId.current = null; setPreview(null); setConflictDecisions({}); } catch { setFailed(true); } finally { setBusy(false); }
  };
  const selectOrganization = async (organizationId: string) => {
    if (pendingImportId.current) {
      try { await discardCloudSync(pendingImportId.current); } catch { setFailed(true); return; }
    }
    pendingImportId.current = null;
    setPreview(null); setConflictDecisions({}); setSyncComplete(false); setSelectedOrganizationId(organizationId);
  };
  const refreshGovernance = async () => {
    await loadOrganizations();
    setGovernanceRevision((value) => value + 1);
  };

  return <section className="mt-4 border-t pt-4"><div className="flex items-center gap-2"><Cloud size={14} /><h3 className="text-xs font-medium text-[hsl(var(--secondary))]">{t("cloud.title")}</h3></div>
    {!cloudConfigured && <><p className="mt-1 text-xs text-[hsl(var(--muted))]">{t("cloud.localOnly")}</p><p className="mt-2 text-xs text-amber-600">{t("cloud.configureHint")}</p></>}
    {cloudConfigured && !userId && <div className="mt-3 grid gap-2"><Input ref={email} type="email" autoComplete="email" placeholder={t("cloud.email")} aria-label={t("cloud.email")} /><Input ref={password} type="password" autoComplete="current-password" placeholder={t("cloud.password")} aria-label={t("cloud.password")} /><div className="flex gap-2"><Button size="sm" disabled={busy} onClick={() => void authenticate(false)}>{t("cloud.signIn")}</Button><Button size="sm" variant="secondary" disabled={busy} onClick={() => void authenticate(true)}>{t("cloud.signUp")}</Button></div>{confirmationSent && <p className="text-xs text-emerald-600">{t("cloud.confirmEmail")}</p>}</div>}
    {cloudConfigured && userId && <div className="mt-3"><div className="flex items-center justify-between"><p className="text-xs text-emerald-600">{t("cloud.signedIn")}</p><Button size="sm" variant="ghost" disabled={busy} onClick={() => void signOut()}><LogOut size={14} />{t("cloud.signOut")}</Button></div><h4 className="mt-3 text-xs font-medium">{t("cloud.organizations")}</h4><div className="mt-2 grid gap-2">{organizations.map((organization) => <Button type="button" variant="ghost" aria-pressed={selectedOrganizationId === organization.id} key={organization.id} onClick={() => void selectOrganization(organization.id)} className={`h-auto justify-start whitespace-normal rounded border p-2 text-left text-xs ${selectedOrganizationId === organization.id ? "border-[hsl(var(--primary))]" : ""}`}>{organization.name}</Button>)}</div><div className="mt-2 flex gap-2"><Input value={organizationName} onChange={(event) => setOrganizationName(event.target.value)} placeholder={t("cloud.organizationName")} aria-label={t("cloud.organizationName")} /><Button size="icon" aria-label={t("cloud.createOrganization")} disabled={busy || !organizationName.trim()} onClick={() => void addOrganization()}><Plus size={15} /></Button></div>
      {selectedOrganizationId && <div className="mt-4 border-t pt-3"><h4 className="text-xs font-medium">{t("cloud.syncTitle")}</h4><p className="mt-1 text-xs text-[hsl(var(--muted))]">{t("cloud.syncHint")}</p><Input ref={syncPassphrase} className="mt-2" type="password" autoComplete="off" placeholder={t("cloud.syncPassphrase")} aria-label={t("cloud.syncPassphrase")} /><div className="mt-2 flex gap-2"><Button size="sm" disabled={busy} onClick={() => void pushSync()}><UploadCloud size={14} />{t("cloud.push")}</Button><Button size="sm" variant="secondary" disabled={busy} onClick={() => void pullSync()}><DownloadCloud size={14} />{t("cloud.pullPreview")}</Button></div>{syncComplete && <p className="mt-2 text-xs text-emerald-600">{t("cloud.syncComplete")}</p>}
        {preview && <div className="mt-3 rounded border p-3 text-xs"><p>{t("cloud.previewCounts", { groups: preview.groupAdditions + preview.groupUpdates, profiles: preview.profileAdditions + preview.profileUpdates })}</p><p>{t("cloud.previewDeletes", { groups: preview.groupDeletions, profiles: preview.profileDeletions })}</p><p className="mt-1 text-amber-600">{t("cloud.previewConflicts", { local: preview.localNewer, conflicts: preview.conflicts })}</p>{preview.conflictItems.length > 0 && <div className="mt-2 grid gap-2">{preview.conflictItems.map((item) => { const key = conflictKey(item); const resolution = conflictDecisions[key] ?? "keepLocal"; return <div key={key} className="rounded border p-2"><p className="truncate">{item.label} · {t(item.remoteDeleted ? "cloud.remoteDeleted" : "cloud.remoteChanged")}</p><div className="mt-1 flex gap-1"><Button size="sm" variant={resolution === "keepLocal" ? "default" : "secondary"} onClick={() => setConflictDecisions((current) => ({ ...current, [key]: "keepLocal" }))}>{t("cloud.keepLocal")}</Button><Button size="sm" variant={resolution === "useRemote" ? "default" : "secondary"} onClick={() => setConflictDecisions((current) => ({ ...current, [key]: "useRemote" }))}>{t("cloud.useRemote")}</Button></div></div>; })}</div>}<div className="mt-2 flex gap-2"><Button size="sm" disabled={busy} onClick={() => void applyPreview()}><Check size={14} />{t("cloud.apply")}</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => void discardPreview()}><X size={14} />{t("common.cancel")}</Button></div></div>}
      </div>}
      <CloudTeamPanel organizationId={selectedOrganizationId} userId={userId} onMembershipChanged={refreshGovernance} />
      {selectedOrganizationId && <CloudGovernancePanel key={`${selectedOrganizationId}:${governanceRevision}`} organizationId={selectedOrganizationId} userId={userId} />}
    </div>}
    {failed && <p className="mt-2 text-xs text-red-500">{t("cloud.error")}</p>}
  </section>;
}
