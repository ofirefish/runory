use tauri::State;

use crate::cloud::{CloudPolicyAction, CloudPolicyService};
use crate::domain::{
    AppResult, DockerActionRequest, DockerContainer, DockerEngineSettingsView, DockerImage,
    DockerImageActionRequest, DockerImagesSearchRequest, DockerNetwork, DockerNetworkActionRequest,
    DockerOnlineImage, DockerRegistry, DockerRegistryDeleteRequest, DockerRegistryUpsertRequest,
    DockerSettingsApplyRequest, DockerVolume, DockerVolumeActionRequest, LogRequest,
    NginxActionRequest, OperationResult, OperationsRequest, Pm2ActionRequest, Pm2Process,
};
use crate::operations::{
    DockerRegistriesRepository, DockerRegistriesService, DockerSettingsService, OperationsService,
};
use crate::ssh::ServerSessionManager;

async fn authorize(
    policies: &CloudPolicyService,
    sessions: &ServerSessionManager,
    session_id: uuid::Uuid,
) -> AppResult<()> {
    policies
        .authorize(
            sessions.profile_id(session_id).await?,
            CloudPolicyAction::Operate,
        )
        .await
}

#[tauri::command]
pub async fn docker_list(
    request: OperationsRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<Vec<DockerContainer>> {
    authorize(&policies, &sessions, request.session_id).await?;
    OperationsService::docker_list(&sessions, request.session_id).await
}
#[tauri::command]
pub async fn docker_action(
    request: DockerActionRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    OperationsService::docker_action(
        &sessions,
        request.session_id,
        request.container,
        request.action,
    )
    .await
}
#[tauri::command]
pub async fn docker_images_list(
    request: OperationsRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<Vec<DockerImage>> {
    authorize(&policies, &sessions, request.session_id).await?;
    OperationsService::docker_images_list(&sessions, request.session_id).await
}
#[tauri::command]
pub async fn docker_images_search(
    request: DockerImagesSearchRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<Vec<DockerOnlineImage>> {
    authorize(&policies, &sessions, request.session_id).await?;
    // Hub search runs on the local client; session auth still scopes the UI context.
    OperationsService::docker_images_search(request.query, request.limit).await
}
#[tauri::command]
pub async fn docker_image_action(
    request: DockerImageActionRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    OperationsService::docker_image_action(&sessions, request.session_id, request.action).await
}
#[tauri::command]
pub async fn docker_networks_list(
    request: OperationsRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<Vec<DockerNetwork>> {
    authorize(&policies, &sessions, request.session_id).await?;
    OperationsService::docker_networks_list(&sessions, request.session_id).await
}
#[tauri::command]
pub async fn docker_network_action(
    request: DockerNetworkActionRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    OperationsService::docker_network_action(&sessions, request.session_id, request.action).await
}
#[tauri::command]
pub async fn docker_volumes_list(
    request: OperationsRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<Vec<DockerVolume>> {
    authorize(&policies, &sessions, request.session_id).await?;
    OperationsService::docker_volumes_list(&sessions, request.session_id).await
}
#[tauri::command]
pub async fn docker_volume_action(
    request: DockerVolumeActionRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    OperationsService::docker_volume_action(&sessions, request.session_id, request.action).await
}
#[tauri::command]
pub async fn docker_settings_get(
    request: OperationsRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<DockerEngineSettingsView> {
    authorize(&policies, &sessions, request.session_id).await?;
    DockerSettingsService::get(&sessions, request.session_id).await
}
#[tauri::command]
pub async fn docker_settings_apply(
    request: DockerSettingsApplyRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    DockerSettingsService::apply(
        &sessions,
        request.session_id,
        request.config,
        request.restart,
    )
    .await
}
#[tauri::command]
pub async fn docker_registries_list(
    request: OperationsRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
    registries: State<'_, DockerRegistriesRepository>,
) -> AppResult<Vec<DockerRegistry>> {
    authorize(&policies, &sessions, request.session_id).await?;
    DockerRegistriesService::list(&registries, &sessions, request.session_id).await
}
#[tauri::command]
pub async fn docker_registries_upsert(
    request: DockerRegistryUpsertRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
    registries: State<'_, DockerRegistriesRepository>,
) -> AppResult<DockerRegistry> {
    authorize(&policies, &sessions, request.session_id).await?;
    DockerRegistriesService::upsert(
        &registries,
        &sessions,
        request.session_id,
        request.id,
        request.url,
        request.name,
        request.username,
        request.password,
        request.namespace,
        request.remarks,
    )
    .await
}
#[tauri::command]
pub async fn docker_registries_delete(
    request: DockerRegistryDeleteRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
    registries: State<'_, DockerRegistriesRepository>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    DockerRegistriesService::delete(&registries, &sessions, request.session_id, request.ids).await
}
#[tauri::command]
pub async fn pm2_list(
    request: OperationsRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<Vec<Pm2Process>> {
    authorize(&policies, &sessions, request.session_id).await?;
    OperationsService::pm2_list(&sessions, request.session_id).await
}
#[tauri::command]
pub async fn pm2_action(
    request: Pm2ActionRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    OperationsService::pm2_action(
        &sessions,
        request.session_id,
        request.process,
        request.action,
    )
    .await
}
#[tauri::command]
pub async fn nginx_action(
    request: NginxActionRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    OperationsService::nginx_action(&sessions, request.session_id, request.action).await
}
#[tauri::command]
pub async fn logs_read(
    request: LogRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    OperationsService::logs(
        &sessions,
        request.session_id,
        request.source,
        request.target,
        request.lines,
    )
    .await
}
