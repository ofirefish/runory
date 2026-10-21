use tauri::State;

use crate::cloud::{CloudPolicyAction, CloudPolicyService};
use crate::deployment::{DeploymentAppsRepository, DeploymentService};
use crate::domain::{
    AppError, AppResult, BackupRequest, CronAddRequest, CronEntry, CronRemoveRequest,
    DeployRequest, DeploymentApp, DeploymentAppDeleteRequest, DeploymentAppUpsertRequest,
    DeploymentAppsListRequest, DeploymentHistoryRequest, DeploymentRecord, DeploymentSetupStatus,
    EnvironmentConfigRequest, GitSetupRequest, GoControlRequest, GoProvisionRequest, GoRuntime,
    HtmlProvisionRequest, JavaControlRequest, JavaInstallJdkRequest, JavaProvisionRequest,
    JavaRuntime, JavaSetDefaultJdkRequest, NodeControlRequest, NodeProvisionRequest, NodeRuntime,
    OperationResult, OperationsRequest, PhpControlRequest, PhpProvisionRequest, PhpRuntime,
    ProjectKind, ProxyProvisionRequest, PythonControlRequest, PythonPrepareEnvRequest,
    PythonProvisionRequest, PythonRuntime, PythonStatusRequest, SslInspectRequest, SslIssueRequest,
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
            request.requirements_path,
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

#[tauri::command]
pub async fn deployment_apps_list(
    request: DeploymentAppsListRequest,
    apps: State<'_, DeploymentAppsRepository>,
) -> AppResult<Vec<DeploymentApp>> {
    // Local content-free app metadata; Deploy policy gates remote operations only.
    apps.list(request.profile_id).await
}

#[tauri::command]
pub async fn deployment_apps_upsert(
    request: DeploymentAppUpsertRequest,
    apps: State<'_, DeploymentAppsRepository>,
) -> AppResult<DeploymentApp> {
    apps.upsert_with_runtimes(
        request.id,
        request.profile_id,
        request.name,
        request.repository_path,
        request.remote_url,
        request.branch,
        request.build,
        request.restart,
        request.project_kind,
        request.python,
        request.php,
        request.java,
        request.node,
        request.go,
        request.html,
        request.proxy,
        request.setup_step,
        request.setup_status,
        request.source_kind,
        request.venv_path,
    )
    .await
}

#[tauri::command]
pub async fn deployment_apps_delete(
    request: DeploymentAppDeleteRequest,
    apps: State<'_, DeploymentAppsRepository>,
) -> AppResult<()> {
    apps.delete(request.profile_id, request.id).await
}

#[tauri::command]
pub async fn deployment_python_provision(
    request: PythonProvisionRequest,
    sessions: State<'_, ServerSessionManager>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    service
        .python_provision(
            &sessions,
            request.session_id,
            request.name,
            request.project_path,
            request.python,
            request.venv_path,
        )
        .await
}

#[tauri::command]
pub async fn deployment_python_status(
    request: PythonStatusRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    DeploymentService::python_status(&sessions, request.session_id, request.service).await
}

#[tauri::command]
pub async fn deployment_python_control(
    request: PythonControlRequest,
    sessions: State<'_, ServerSessionManager>,
    apps: State<'_, DeploymentAppsRepository>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    let profile_id = sessions.profile_id(request.session_id).await?;
    let app = apps
        .list(profile_id)
        .await?
        .into_iter()
        .find(|app| app.id == request.app_id && app.project_kind == ProjectKind::Python)
        .ok_or(AppError::InvalidOperation)?;
    if matches!(
        app.setup_status,
        DeploymentSetupStatus::Draft | DeploymentSetupStatus::Provisioning
    ) {
        return Err(AppError::InvalidOperation);
    }
    service
        .python_control(&sessions, request.session_id, &app.name, request.action)
        .await
}

#[tauri::command]
pub async fn deployment_list_system_users(
    request: OperationsRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<Vec<String>> {
    authorize(&policies, &sessions, request.session_id).await?;
    DeploymentService::list_system_users(&sessions, request.session_id).await
}

#[tauri::command]
pub async fn deployment_list_python_runtimes(
    request: OperationsRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<Vec<PythonRuntime>> {
    authorize(&policies, &sessions, request.session_id).await?;
    DeploymentService::list_python_runtimes(&sessions, request.session_id).await
}

#[tauri::command]
pub async fn deployment_python_prepare_env(
    request: PythonPrepareEnvRequest,
    sessions: State<'_, ServerSessionManager>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    service
        .python_prepare_env(
            &sessions,
            request.session_id,
            request.project_path,
            request.python_bin,
            request.venv_path,
            request.requirements_path,
            request.install_packages,
            request.startup,
            request.protocol,
        )
        .await
}

#[tauri::command]
pub async fn deployment_php_provision(
    request: PhpProvisionRequest,
    sessions: State<'_, ServerSessionManager>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    service
        .php_provision(
            &sessions,
            request.session_id,
            request.name,
            request.project_path,
            request.php,
        )
        .await
}

#[tauri::command]
pub async fn deployment_php_control(
    request: PhpControlRequest,
    sessions: State<'_, ServerSessionManager>,
    apps: State<'_, DeploymentAppsRepository>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    let profile_id = sessions.profile_id(request.session_id).await?;
    let app = apps
        .list(profile_id)
        .await?
        .into_iter()
        .find(|app| app.id == request.app_id && app.project_kind == ProjectKind::Php)
        .ok_or(AppError::InvalidOperation)?;
    if matches!(
        app.setup_status,
        DeploymentSetupStatus::Draft | DeploymentSetupStatus::Provisioning
    ) {
        return Err(AppError::InvalidOperation);
    }
    service
        .php_control(&sessions, request.session_id, &app.name, request.action)
        .await
}

#[tauri::command]
pub async fn deployment_list_php_runtimes(
    request: OperationsRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<Vec<PhpRuntime>> {
    authorize(&policies, &sessions, request.session_id).await?;
    DeploymentService::list_php_runtimes(&sessions, request.session_id).await
}

#[tauri::command]
pub async fn deployment_java_provision(
    request: JavaProvisionRequest,
    sessions: State<'_, ServerSessionManager>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    service
        .java_provision(
            &sessions,
            request.session_id,
            request.name,
            request.project_path,
            request.java,
        )
        .await
}

#[tauri::command]
pub async fn deployment_java_control(
    request: JavaControlRequest,
    sessions: State<'_, ServerSessionManager>,
    apps: State<'_, DeploymentAppsRepository>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    let profile_id = sessions.profile_id(request.session_id).await?;
    let app = apps
        .list(profile_id)
        .await?
        .into_iter()
        .find(|app| app.id == request.app_id && app.project_kind == ProjectKind::Java)
        .ok_or(AppError::InvalidOperation)?;
    if matches!(
        app.setup_status,
        DeploymentSetupStatus::Draft | DeploymentSetupStatus::Provisioning
    ) {
        return Err(AppError::InvalidOperation);
    }
    service
        .java_control(&sessions, request.session_id, &app.name, request.action)
        .await
}

#[tauri::command]
pub async fn deployment_list_java_runtimes(
    request: OperationsRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<Vec<JavaRuntime>> {
    authorize(&policies, &sessions, request.session_id).await?;
    DeploymentService::list_java_runtimes(&sessions, request.session_id).await
}

#[tauri::command]
pub async fn deployment_java_install_jdk(
    request: JavaInstallJdkRequest,
    sessions: State<'_, ServerSessionManager>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    service
        .java_install_jdk(&sessions, request.session_id, request.version)
        .await
}

#[tauri::command]
pub async fn deployment_java_set_default_jdk(
    request: JavaSetDefaultJdkRequest,
    sessions: State<'_, ServerSessionManager>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    service
        .java_set_default_jdk(&sessions, request.session_id, request.binary)
        .await
}

#[tauri::command]
pub async fn deployment_node_provision(
    request: NodeProvisionRequest,
    sessions: State<'_, ServerSessionManager>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    service
        .node_provision(
            &sessions,
            request.session_id,
            request.name,
            request.project_path,
            request.node,
        )
        .await
}

#[tauri::command]
pub async fn deployment_node_control(
    request: NodeControlRequest,
    sessions: State<'_, ServerSessionManager>,
    apps: State<'_, DeploymentAppsRepository>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    let profile_id = sessions.profile_id(request.session_id).await?;
    let app = apps
        .list(profile_id)
        .await?
        .into_iter()
        .find(|app| app.id == request.app_id && app.project_kind == ProjectKind::Node)
        .ok_or(AppError::InvalidOperation)?;
    if matches!(
        app.setup_status,
        DeploymentSetupStatus::Draft | DeploymentSetupStatus::Provisioning
    ) {
        return Err(AppError::InvalidOperation);
    }
    service
        .node_control(&sessions, request.session_id, &app.name, request.action)
        .await
}

#[tauri::command]
pub async fn deployment_list_node_runtimes(
    request: OperationsRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<Vec<NodeRuntime>> {
    authorize(&policies, &sessions, request.session_id).await?;
    DeploymentService::list_node_runtimes(&sessions, request.session_id).await
}

#[tauri::command]
pub async fn deployment_go_provision(
    request: GoProvisionRequest,
    sessions: State<'_, ServerSessionManager>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    service
        .go_provision(
            &sessions,
            request.session_id,
            request.name,
            request.project_path,
            request.go,
        )
        .await
}

#[tauri::command]
pub async fn deployment_go_control(
    request: GoControlRequest,
    sessions: State<'_, ServerSessionManager>,
    apps: State<'_, DeploymentAppsRepository>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    let profile_id = sessions.profile_id(request.session_id).await?;
    let app = apps
        .list(profile_id)
        .await?
        .into_iter()
        .find(|app| app.id == request.app_id && app.project_kind == ProjectKind::Go)
        .ok_or(AppError::InvalidOperation)?;
    if matches!(
        app.setup_status,
        DeploymentSetupStatus::Draft | DeploymentSetupStatus::Provisioning
    ) {
        return Err(AppError::InvalidOperation);
    }
    service
        .go_control(&sessions, request.session_id, &app.name, request.action)
        .await
}

#[tauri::command]
pub async fn deployment_list_go_runtimes(
    request: OperationsRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<Vec<GoRuntime>> {
    authorize(&policies, &sessions, request.session_id).await?;
    DeploymentService::list_go_runtimes(&sessions, request.session_id).await
}

#[tauri::command]
pub async fn deployment_html_provision(
    request: HtmlProvisionRequest,
    sessions: State<'_, ServerSessionManager>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    service
        .html_provision(
            &sessions,
            request.session_id,
            request.name,
            request.project_path,
            request.html,
        )
        .await
}

#[tauri::command]
pub async fn deployment_proxy_provision(
    request: ProxyProvisionRequest,
    sessions: State<'_, ServerSessionManager>,
    service: State<'_, DeploymentService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<OperationResult> {
    authorize(&policies, &sessions, request.session_id).await?;
    service
        .proxy_provision(&sessions, request.session_id, request.name, request.proxy)
        .await
}
