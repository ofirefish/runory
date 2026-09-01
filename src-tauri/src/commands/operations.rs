use tauri::State;

use crate::cloud::{CloudPolicyAction, CloudPolicyService};
use crate::domain::{
    AppResult, DockerActionRequest, DockerContainer, LogRequest, NginxActionRequest,
    OperationResult, OperationsRequest, Pm2ActionRequest, Pm2Process,
};
use crate::operations::OperationsService;
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
