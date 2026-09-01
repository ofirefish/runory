use tauri::State;

use crate::cloud::{
    CloudPolicyBindingRequest, CloudPolicyCredentialRequest, CloudPolicyOrganizationRequest,
    CloudPolicyService, CloudPolicyStatus,
};
use crate::domain::AppResult;

#[tauri::command]
pub async fn cloud_policy_bind(
    request: CloudPolicyBindingRequest,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<CloudPolicyStatus> {
    policies.bind(request).await
}

#[tauri::command]
pub async fn cloud_policy_refresh(
    request: CloudPolicyCredentialRequest,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<()> {
    policies.refresh(request).await
}

#[tauri::command]
pub async fn cloud_policy_lock(policies: State<'_, CloudPolicyService>) -> AppResult<()> {
    policies.lock().await;
    Ok(())
}

#[tauri::command]
pub async fn cloud_policy_unbind(
    request: CloudPolicyOrganizationRequest,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<CloudPolicyStatus> {
    policies.unbind(request.organization_id).await
}

#[tauri::command]
pub async fn cloud_policy_status(
    request: CloudPolicyOrganizationRequest,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<CloudPolicyStatus> {
    policies.status(request.organization_id).await
}
