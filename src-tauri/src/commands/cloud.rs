use tauri::State;
use zeroize::Zeroizing;

use crate::cloud::{CloudPolicyService, CloudSyncService};
use crate::domain::{
    AppResult, CloudApplyRequest, CloudApplyResult, CloudDiscardRequest, CloudEncryptedPayload,
    CloudExportRequest, CloudImportPreview, CloudImportRequest,
    CloudRecoveryPassphraseRotateRequest, CloudSyncKeyRequest, CloudSyncKeyStatus,
};
use crate::profiles::ProfileService;

#[tauri::command]
pub async fn cloud_sync_export(
    mut request: CloudExportRequest,
    cloud: State<'_, CloudSyncService>,
) -> AppResult<CloudEncryptedPayload> {
    let recovery_passphrase = request.recovery_passphrase.take().map(Zeroizing::new);
    cloud
        .export(request.organization_id, recovery_passphrase)
        .await
}

#[tauri::command]
pub async fn cloud_sync_preview(
    mut request: CloudImportRequest,
    cloud: State<'_, CloudSyncService>,
) -> AppResult<CloudImportPreview> {
    let recovery_passphrase = request.recovery_passphrase.take().map(Zeroizing::new);
    cloud
        .preview(
            request.organization_id,
            recovery_passphrase,
            request.payload,
        )
        .await
}

#[tauri::command]
pub async fn cloud_sync_key_status(
    request: CloudSyncKeyRequest,
    cloud: State<'_, CloudSyncService>,
) -> AppResult<CloudSyncKeyStatus> {
    cloud.key_status(request.organization_id).await
}

#[tauri::command]
pub async fn cloud_sync_forget_key(
    request: CloudSyncKeyRequest,
    cloud: State<'_, CloudSyncService>,
) -> AppResult<()> {
    cloud.forget_key(request.organization_id).await
}

#[tauri::command]
pub async fn cloud_sync_rotate_recovery_passphrase(
    mut request: CloudRecoveryPassphraseRotateRequest,
    cloud: State<'_, CloudSyncService>,
) -> AppResult<CloudEncryptedPayload> {
    let new_recovery_passphrase =
        Zeroizing::new(std::mem::take(&mut request.new_recovery_passphrase));
    cloud
        .rotate_recovery_passphrase(
            request.organization_id,
            new_recovery_passphrase,
            request.payload,
        )
        .await
}

#[tauri::command]
pub async fn cloud_sync_apply(
    request: CloudApplyRequest,
    cloud: State<'_, CloudSyncService>,
    profiles: State<'_, ProfileService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<CloudApplyResult> {
    let result = cloud.apply(request.import_id, request.decisions).await?;
    let reconciliation = match profiles.list().await {
        Ok(profiles) => {
            policies
                .reconcile_profiles(profiles.into_iter().map(|profile| profile.id).collect())
                .await
        }
        Err(error) => Err(error),
    };
    if let Err(error) = reconciliation {
        tracing::warn!(
            error_code = error.code(),
            "could not reconcile cloud policy profile snapshot after sync"
        );
    }
    Ok(result)
}

#[tauri::command]
pub async fn cloud_sync_discard(
    request: CloudDiscardRequest,
    cloud: State<'_, CloudSyncService>,
) -> AppResult<()> {
    cloud.discard(request.import_id).await
}
