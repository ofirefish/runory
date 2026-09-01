import { invoke } from "@tauri-apps/api/core";
import type { CloudApplyResult, CloudConflictDecision, CloudEncryptedPayload, CloudImportPreview } from "../../types/cloud";

export const exportCloudSync = (organizationId: string, passphrase: string) =>
  invoke<CloudEncryptedPayload>("cloud_sync_export", { request: { organizationId, passphrase } });
export const previewCloudSync = (organizationId: string, passphrase: string, payload: CloudEncryptedPayload) =>
  invoke<CloudImportPreview>("cloud_sync_preview", { request: { organizationId, passphrase, payload } });
export const applyCloudSync = (importId: string, decisions: CloudConflictDecision[]) =>
  invoke<CloudApplyResult>("cloud_sync_apply", { request: { importId, decisions: decisions.map(({ kind, id, localUpdatedAt, resolution }) => ({ kind, id, expectedLocalUpdatedAt: localUpdatedAt, resolution })) } });
export const discardCloudSync = (importId: string) =>
  invoke<void>("cloud_sync_discard", { request: { importId } });
