import { invoke } from "@tauri-apps/api/core";
import type { CloudApplyResult, CloudConflictDecision, CloudEncryptedPayload, CloudImportPreview, CloudSyncKeyStatus } from "../../types/cloud";

export const exportCloudSync = (organizationId: string, recoveryPassphrase?: string) =>
  invoke<CloudEncryptedPayload>("cloud_sync_export", { request: { organizationId, recoveryPassphrase } });
export const previewCloudSync = (organizationId: string, payload: CloudEncryptedPayload, recoveryPassphrase?: string) =>
  invoke<CloudImportPreview>("cloud_sync_preview", { request: { organizationId, recoveryPassphrase, payload } });
export const cloudSyncKeyStatus = (organizationId: string) =>
  invoke<CloudSyncKeyStatus>("cloud_sync_key_status", { request: { organizationId } });
export const forgetCloudSyncKey = (organizationId: string) =>
  invoke<void>("cloud_sync_forget_key", { request: { organizationId } });
export const rotateCloudRecoveryPassphrase = (organizationId: string, payload: CloudEncryptedPayload, newRecoveryPassphrase: string) =>
  invoke<CloudEncryptedPayload>("cloud_sync_rotate_recovery_passphrase", { request: { organizationId, payload, newRecoveryPassphrase } });
export const applyCloudSync = (importId: string, decisions: CloudConflictDecision[]) =>
  invoke<CloudApplyResult>("cloud_sync_apply", { request: { importId, decisions: decisions.map(({ kind, id, localUpdatedAt, resolution }) => ({ kind, id, expectedLocalUpdatedAt: localUpdatedAt, resolution })) } });
export const discardCloudSync = (importId: string) =>
  invoke<void>("cloud_sync_discard", { request: { importId } });
