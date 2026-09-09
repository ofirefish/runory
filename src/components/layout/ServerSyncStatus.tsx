import { Cloud, CloudOff, LogIn, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { cloudConfigured } from "../../lib/supabase/client";
import { cloudSession, listOrganizations, loadEncryptedInventory, onCloudAuthStateChange, writeEncryptedInventory } from "../../lib/supabase/cloud";
import { cloudSyncKeyStatus, discardCloudSync, exportCloudSync, previewCloudSync } from "../../lib/tauri/cloud";
import type { CloudEncryptedPayload } from "../../types/cloud";
import { hasCloudSyncChanges } from "../../features/settings/cloud-sync";
import { Button } from "../ui/button";

type SyncSummary =
  | { kind: "checking" | "syncing" | "review" | "local" | "signedOut" | "never" | "error"; updatedAt?: undefined }
  | { kind: "synced"; updatedAt: string };

type SyncTarget = { organizationId: string; keyConfigured: boolean };

export function ServerSyncStatus({ onOpenSync, onOpenAuth }: { onOpenSync: () => void; onOpenAuth: () => void }) {
  const { t, i18n } = useTranslation();
  const [summary, setSummary] = useState<SyncSummary>(() => ({ kind: cloudConfigured ? "checking" : "local" }));
  const [target, setTarget] = useState<SyncTarget | null>(null);

  const refresh = useCallback(async () => {
    setTarget(null);
    if (!cloudConfigured) {
      setSummary({ kind: "local" });
      return;
    }
    try {
      const session = await cloudSession();
      if (!session) {
        setSummary({ kind: "signedOut" });
        return;
      }
      const organizations = await listOrganizations();
      const organization = organizations.find((item) => item.kind === "personal") ?? organizations[0];
      if (!organization) {
        setSummary({ kind: "never" });
        return;
      }
      const [inventory, keyStatus] = await Promise.all([loadEncryptedInventory(organization.id), cloudSyncKeyStatus(organization.id)]);
      setTarget({ organizationId: organization.id, keyConfigured: keyStatus.configured });
      setSummary(inventory ? { kind: "synced", updatedAt: inventory.updated_at } : { kind: "never" });
    } catch {
      setSummary({ kind: "error" });
    }
  }, []);

  useEffect(() => {
    void refresh();
    if (!cloudConfigured) return;
    const subscription = onCloudAuthStateChange(() => { void refresh(); });
    return () => subscription.unsubscribe();
  }, [refresh]);

  const label = t(`serverManagement.syncStatus.${summary.kind}`);
  const detail = summary.kind === "synced"
    ? t("serverManagement.lastSyncedAt", { time: new Intl.DateTimeFormat(i18n.language, { dateStyle: "short", timeStyle: "short" }).format(new Date(summary.updatedAt)) })
    : label;
  const StatusIcon = summary.kind === "local" || summary.kind === "signedOut" ? CloudOff : Cloud;
  const signedOut = summary.kind === "signedOut";
  const busy = summary.kind === "checking" || summary.kind === "syncing";
  const reviewRequired = summary.kind === "review";
  const directSyncAvailable = Boolean(target?.keyConfigured);
  const ActionIcon = signedOut ? LogIn : RefreshCw;
  const actionLabel = signedOut ? "serverManagement.signInAction"
    : reviewRequired ? "serverManagement.reviewSyncAction"
      : directSyncAvailable ? "serverManagement.syncAction"
        : "serverManagement.setupSyncAction";
  const actionTitle = signedOut ? "serverManagement.openSignIn"
    : reviewRequired ? "serverManagement.reviewSync"
      : directSyncAvailable ? "serverManagement.syncNow"
        : "serverManagement.openSync";

  const syncNow = async () => {
    if (!target?.keyConfigured) {
      onOpenSync();
      return;
    }
    setSummary({ kind: "syncing" });
    try {
      const remote = await loadEncryptedInventory(target.organizationId);
      if (!remote) {
        const payload = await exportCloudSync(target.organizationId);
        await writeEncryptedInventory(target.organizationId, payload, 0);
        setSummary({ kind: "synced", updatedAt: new Date().toISOString() });
        return;
      }
      const preview = await previewCloudSync(target.organizationId, remote.encrypted_payload as CloudEncryptedPayload);
      if (hasCloudSyncChanges(preview)) {
        await discardCloudSync(preview.importId);
        setSummary({ kind: "review" });
        return;
      }
      await discardCloudSync(preview.importId);
      setSummary({ kind: "synced", updatedAt: remote.updated_at });
    } catch {
      setSummary({ kind: "error" });
    }
  };

  const runAction = () => {
    if (signedOut) onOpenAuth();
    else if (reviewRequired) onOpenSync();
    else void syncNow();
  };

  return <div className="server-sync-control">
    <span className={`server-sync-status server-sync-status-${summary.kind}`} title={detail} aria-label={detail}>
      <StatusIcon size={14} aria-hidden="true" />
      <span className="server-sync-status-copy"><span>{t("serverManagement.syncLabel")}</span><span aria-hidden="true">·</span><strong>{label}</strong></span>
    </span>
    <Button type="button" variant="secondary" size="sm" disabled={busy} onClick={runAction} title={t(actionTitle)}>
      <ActionIcon className={summary.kind === "syncing" ? "server-sync-spinning" : undefined} size={14} aria-hidden="true" />{t(actionLabel)}
    </Button>
  </div>;
}
