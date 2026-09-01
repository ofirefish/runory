use tauri::State;
use zeroize::Zeroizing;

use crate::cloud::{CloudPolicyService, CloudSyncService};
use crate::domain::{
    AppResult, CloudApplyRequest, CloudApplyResult, CloudDiscardRequest, CloudEncryptedPayload,
    CloudExportRequest, CloudImportPreview, CloudImportRequest,
};
use crate::profiles::ProfileService;

#[tauri::command]
pub async fn cloud_sync_export(
    mut request: CloudExportRequest,
    cloud: State<'_, CloudSyncService>,
) -> AppResult<CloudEncryptedPayload> {
    let passphrase = Zeroizing::new(std::mem::take(&mut request.passphrase));
    cloud.export(request.organization_id, passphrase).await
}

#[tauri::command]
pub async fn cloud_sync_preview(
    mut request: CloudImportRequest,
    cloud: State<'_, CloudSyncService>,
) -> AppResult<CloudImportPreview> {
    let passphrase = Zeroizing::new(std::mem::take(&mut request.passphrase));
    cloud
        .preview(request.organization_id, passphrase, request.payload)
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
