use tauri::State;

use crate::cloud::{CloudPolicyAction, CloudPolicyService};
use crate::deployment::DeploymentService;
use crate::domain::{
    AppResult, BackupRequest, CronAddRequest, CronEntry, CronRemoveRequest, DeployRequest,
    DeploymentHistoryRequest, DeploymentRecord, EnvironmentConfigRequest, GitSetupRequest,
    OperationResult, OperationsRequest, SslInspectRequest, SslIssueRequest,
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
            CloudPolicyAction::Deploy,
        )
        .await
}

#[tauri::command]
pub async fn deployment_git_setup(
    request: GitSetupRequest,
    sessions: State<'_, ServerSessionManager>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    service
        .git_setup(
            &sessions,
            request.session_id,
            request.repository_path,
            request.remote_url,
            request.branch,
        )
        .await
}
#[tauri::command]
pub async fn deployment_run(
    request: DeployRequest,
    sessions: State<'_, ServerSessionManager>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    service
        .deploy(
            &sessions,
            request.session_id,
            request.repository_path,
            request.branch,
            request.build,
            request.restart,
        )
        .await
}
#[tauri::command]
pub async fn deployment_environment_write(
    request: EnvironmentConfigRequest,
    sessions: State<'_, ServerSessionManager>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    service
        .write_environment(&sessions, request.session_id, request.path, request.entries)
        .await
}
#[tauri::command]
pub async fn deployment_ssl_inspect(
    request: SslInspectRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    DeploymentService::ssl_inspect(&sessions, request.session_id, request.domain).await
}
#[tauri::command]
pub async fn deployment_ssl_issue(
    request: SslIssueRequest,
    sessions: State<'_, ServerSessionManager>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    service
        .ssl_issue(
            &sessions,
            request.session_id,
            request.domain,
            request.email,
            request.webroot,
        )
        .await
}
#[tauri::command]
pub async fn deployment_backup(
    request: BackupRequest,
    sessions: State<'_, ServerSessionManager>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    service.backup(&sessions, request).await
}
#[tauri::command]
pub async fn deployment_cron_list(
    request: OperationsRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<Vec<CronEntry>> {
    authorize(&policies, &sessions, request.session_id).await?;
    DeploymentService::cron_list(&sessions, request.session_id).await
}
#[tauri::command]
pub async fn deployment_cron_add(
    request: CronAddRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<CronEntry> {
    authorize(&policies, &sessions, request.session_id).await?;
    DeploymentService::cron_add(
        &sessions,
        request.session_id,
        request.schedule,
        request.task,
    )
    .await
}
#[tauri::command]
pub async fn deployment_cron_remove(
    request: CronRemoveRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    DeploymentService::cron_remove(&sessions, request.session_id, request.cron_id).await
}
#[tauri::command]
pub async fn deployment_history(
    request: DeploymentHistoryRequest,
    service: State<'_, DeploymentService>,
) -> AppResult<Vec<DeploymentRecord>> {
    service.history(request.profile_id).await
}
