use tauri::State;

use crate::cloud::{CloudPolicyAction, CloudPolicyService};
use crate::dashboard::DashboardService;
use crate::domain::{
    AppResult, DashboardRequest, ProcessInfo, ServerDashboard, ServiceHealth, ServiceHealthRequest,
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
pub async fn dashboard_overview(
    request: DashboardRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<ServerDashboard> {
    authorize(&policies, &sessions, request.session_id).await?;
    DashboardService::overview(&sessions, request.session_id).await
}

#[tauri::command]
pub async fn dashboard_processes(
    request: DashboardRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<Vec<ProcessInfo>> {
    authorize(&policies, &sessions, request.session_id).await?;
    DashboardService::processes(&sessions, request.session_id).await
}

#[tauri::command]
pub async fn dashboard_service_health(
    request: ServiceHealthRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<Vec<ServiceHealth>> {
    authorize(&policies, &sessions, request.session_id).await?;
    DashboardService::service_health(&sessions, request.session_id, request.services).await
}
