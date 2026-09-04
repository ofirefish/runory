import { Plus, ScrollText, Shield, ShieldCheck, ShieldOff, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { SelectControl } from "../../components/ui/select-control";
import { cloudEndpoint, cloudPublishableKey } from "../../lib/supabase/client";
import {
  createAccessPolicy,
  cloudSession,
  deleteAccessPolicy,
  getMyMembership,
  listAccessPolicies,
  listCloudAuditRecords,
} from "../../lib/supabase/cloud";
import { bindCloudPolicy, cloudPolicyStatus, unbindCloudPolicy } from "../../lib/tauri/cloud-policy";
import { useCatalogStore } from "../../stores/catalog-store";
import type {
  AccessPolicy,
  AccessPolicyAction,
  AccessPolicyEffect,
  CloudAuditRecord,
  OrganizationRole,
} from "../../types/cloud";

const PAGE_SIZE = 30;
const actions: AccessPolicyAction[] = ["connect", "read-files", "write-files", "operate", "deploy", "ai-execute"];

export function CloudGovernancePanel({ organizationId, userId }: { organizationId: string; userId: string }) {
  const { t } = useTranslation();
  const [role, setRole] = useState<OrganizationRole | null>(null);
  const [policies, setPolicies] = useState<AccessPolicy[]>([]);
  const [audit, setAudit] = useState<CloudAuditRecord[]>([]);
  const [hasMoreAudit, setHasMoreAudit] = useState(false);
  const [name, setName] = useState("");
  const [effect, setEffect] = useState<AccessPolicyEffect>("deny");
  const [action, setAction] = useState<AccessPolicyAction>("connect");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const [enforced, setEnforced] = useState(false);
  const [authenticated, setAuthenticated] = useState(false);
  const profiles = useCatalogStore((state) => state.profiles);

  const load = useCallback(async () => {
    const [membership, policyRows, status] = await Promise.all([
      getMyMembership(organizationId, userId),
      listAccessPolicies(organizationId),
      cloudPolicyStatus(organizationId),
    ]);
    setEnforced(status.enabled); setAuthenticated(status.authenticated);
    setRole(membership?.role ?? null);
    setPolicies(policyRows);
    if (membership?.role === "owner" || membership?.role === "admin") {
      const auditRows = await listCloudAuditRecords(organizationId, null, PAGE_SIZE);
      setAudit(auditRows); setHasMoreAudit(auditRows.length === PAGE_SIZE);
    } else {
      setAudit([]); setHasMoreAudit(false);
    }
  }, [organizationId, userId]);

  useEffect(() => {
    setPolicies([]); setAudit([]); setFailed(false);
    void load().catch(() => setFailed(true));
  }, [load]);

  const addPolicy = async () => {
    if (!name.trim()) return;
    setBusy(true); setFailed(false);
    try { await createAccessPolicy(organizationId, name.trim(), effect, action); setName(""); await load(); }
    catch { setFailed(true); } finally { setBusy(false); }
  };
  const removePolicy = async (id: string) => {
    setBusy(true); setFailed(false);
    try { await deleteAccessPolicy(id); await load(); }
    catch { setFailed(true); } finally { setBusy(false); }
  };
  const loadMoreAudit = async () => {
    const cursor = audit.at(-1);
    if (!cursor) return;
    setBusy(true); setFailed(false);
    try {
      const rows = await listCloudAuditRecords(organizationId, cursor, PAGE_SIZE);
      setAudit((current) => [...current, ...rows]); setHasMoreAudit(rows.length === PAGE_SIZE);
    } catch { setFailed(true); } finally { setBusy(false); }
  };
  const toggleEnforcement = async () => {
    setBusy(true); setFailed(false);
    try {
      if (enforced) {
        const status = await unbindCloudPolicy(organizationId); setEnforced(status.enabled); setAuthenticated(status.authenticated); return;
      }
      const session = await cloudSession();
      if (!session || !session.expires_at || !cloudEndpoint || !cloudPublishableKey || profiles.length === 0) throw new Error("POLICY_BIND_UNAVAILABLE");
      const status = await bindCloudPolicy({ organizationId, profileIds: profiles.map((profile) => profile.id), supabaseUrl: cloudEndpoint, publishableKey: cloudPublishableKey, accessToken: session.access_token, expiresAt: session.expires_at });
      setEnforced(status.enabled); setAuthenticated(status.authenticated);
    } catch { setFailed(true); } finally { setBusy(false); }
  };

  const canManage = role === "owner" || role === "admin";
  return <section className="mt-4 border-t pt-3">
    <div className="flex items-center gap-2"><Shield size={14} /><h4 className="text-xs font-medium">{t("cloud.governance")}</h4></div>
    <p className="mt-1 text-xs text-[hsl(var(--muted))]">{t("cloud.policyFoundationHint")}</p>
    <Button className="mt-2" size="sm" variant={enforced ? "default" : "secondary"} disabled={busy || (!enforced && profiles.length === 0)} onClick={() => void toggleEnforcement()}>{enforced ? <ShieldOff size={13} /> : <ShieldCheck size={13} />}{t(enforced ? "cloud.disablePolicyEnforcement" : "cloud.enablePolicyEnforcement")}</Button>
    {enforced && <p className={`mt-1 text-xs ${authenticated ? "text-emerald-600" : "text-amber-600"}`}>{t(authenticated ? "cloud.policyAuthenticated" : "cloud.policyAuthenticationRequired")}</p>}
    <div className="mt-3 grid gap-2">{policies.map((policy) => <div key={policy.id} className="flex items-center justify-between gap-2 rounded border p-2 text-xs"><span className="min-w-0 flex-1 truncate">{policy.name} · {t(`cloud.effect.${policy.effect}`)} · {t(`cloud.action.${policy.action}`)}</span>{canManage && <Button size="icon" variant="ghost" disabled={busy} aria-label={t("cloud.deletePolicy")} onClick={() => void removePolicy(policy.id)}><Trash2 size={13} /></Button>}</div>)}</div>
    {canManage && <div className="mt-3 grid gap-2"><Input value={name} onChange={(event) => setName(event.target.value)} placeholder={t("cloud.policyName")} aria-label={t("cloud.policyName")} /><div className="flex gap-2"><SelectControl className="w-28 shrink-0 text-xs" value={effect} onValueChange={setEffect} disabled={busy} label={t("cloud.policyEffect")} options={(["deny", "allow"] as const).map((value) => ({ value, label: t(`cloud.effect.${value}`) }))} /><SelectControl className="min-w-0 flex-1 text-xs" value={action} onValueChange={setAction} disabled={busy} label={t("cloud.policyAction")} options={actions.map((value) => ({ value, label: t(`cloud.action.${value}`) }))} /><Button size="icon" disabled={busy || !name.trim()} aria-label={t("cloud.createPolicy")} onClick={() => void addPolicy()}><Plus size={13} /></Button></div></div>}
    {canManage && <div className="mt-4 border-t pt-3"><div className="flex items-center gap-2"><ScrollText size={14} /><h5 className="text-xs font-medium">{t("cloud.audit")}</h5></div><div className="mt-2 grid gap-2">{audit.map((record) => <div key={record.id} className="rounded border p-2 text-xs"><div className="flex justify-between gap-2"><span className="truncate">{record.action} · {record.resource_type}</span><span className={record.result === "failed" || record.result === "denied" ? "text-red-500" : "text-[hsl(var(--muted))]"}>{t(`cloud.auditResult.${record.result}`)}</span></div><p className="mt-1 text-[hsl(var(--muted))]">{new Date(record.occurred_at).toLocaleString()}</p></div>)}</div>{hasMoreAudit && <Button className="mt-2" size="sm" variant="secondary" disabled={busy} onClick={() => void loadMoreAudit()}>{t("cloud.loadMoreAudit")}</Button>}</div>}
    {failed && <p className="mt-2 text-xs text-red-500">{t("cloud.governanceError")}</p>}
  </section>;
}
