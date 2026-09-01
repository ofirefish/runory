use tauri::State;

use crate::cloud::CloudPolicyService;
use crate::credentials::CredentialService;
use crate::domain::{
    AppResult, CreateGroupRequest, CreateProfileRequest, DeleteGroupRequest, DeleteProfileRequest,
    HostGroup, KeySource, ReorderGroupsRequest, ReorderProfilesRequest, ServerProfile,
    UpdateGroupRequest, UpdateProfileRequest,
};
use crate::groups::GroupService;
use crate::profiles::ProfileService;

#[tauri::command]
pub async fn group_list(groups: State<'_, GroupService>) -> AppResult<Vec<HostGroup>> {
    groups.list().await
}

#[tauri::command]
pub async fn group_create(
    request: CreateGroupRequest,
    groups: State<'_, GroupService>,
) -> AppResult<HostGroup> {
    groups.create(request).await
}

#[tauri::command]
pub async fn group_update(
    request: UpdateGroupRequest,
    groups: State<'_, GroupService>,
) -> AppResult<HostGroup> {
    groups.update(request).await
}

#[tauri::command]
pub async fn group_delete(
    request: DeleteGroupRequest,
    groups: State<'_, GroupService>,
) -> AppResult<()> {
    groups.delete(request.id).await
}

#[tauri::command]
pub async fn group_reorder(
    request: ReorderGroupsRequest,
    groups: State<'_, GroupService>,
) -> AppResult<Vec<HostGroup>> {
    groups.reorder(request).await
}

#[tauri::command]
pub async fn profile_list(profiles: State<'_, ProfileService>) -> AppResult<Vec<ServerProfile>> {
    profiles.list().await
}

#[tauri::command]
pub async fn profile_create(
    request: CreateProfileRequest,
    profiles: State<'_, ProfileService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<ServerProfile> {
    let profile = profiles.create(request).await?;
    reconcile_policy_profiles(&profiles, &policies).await;
    Ok(profile)
}

#[tauri::command]
pub async fn profile_update(
    request: UpdateProfileRequest,
    profiles: State<'_, ProfileService>,
    credentials: State<'_, CredentialService>,
) -> AppResult<ServerProfile> {
    let existing = profiles.get(request.id).await?;
    let updated = profiles.update(request).await?;
    if let Some(KeySource::Vault { key_id }) = existing.key_source {
        let retained = matches!(updated.key_source, Some(KeySource::Vault { key_id: current }) if current == key_id);
        if !retained {
            if let Err(error) = credentials.forget_private_key(key_id).await {
                tracing::warn!(key_id = %key_id, error_code = error.code(), "could not remove replaced private key from vault");
            }
        }
    }
    Ok(updated)
}

#[tauri::command]
pub async fn profile_delete(
    request: DeleteProfileRequest,
    profiles: State<'_, ProfileService>,
    credentials: State<'_, CredentialService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<()> {
    let profile = profiles.get(request.id).await?;
    credentials.prepare_profile_delete(request.id).await?;
    profiles.delete(request.id).await?;
    if let Some(KeySource::Vault { key_id }) = profile.key_source {
        if let Err(error) = credentials.forget_private_key(key_id).await {
            tracing::warn!(key_id = %key_id, error_code = error.code(), "could not remove deleted profile private key from vault");
        }
    }
    reconcile_policy_profiles(&profiles, &policies).await;
    Ok(())
}

#[tauri::command]
pub async fn profile_reorder(
    request: ReorderProfilesRequest,
    profiles: State<'_, ProfileService>,
) -> AppResult<Vec<ServerProfile>> {
    profiles.reorder(request).await
}

async fn reconcile_policy_profiles(profiles: &ProfileService, policies: &CloudPolicyService) {
    let result = match profiles.list().await {
        Ok(profiles) => {
            policies
                .reconcile_profiles(profiles.into_iter().map(|profile| profile.id).collect())
                .await
        }
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        tracing::warn!(
            error_code = error.code(),
            "could not reconcile cloud policy profile snapshot"
        );
    }
}
