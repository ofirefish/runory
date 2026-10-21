use std::sync::Arc;

use tauri::State;
use zeroize::Zeroizing;

use crate::domain::{AppError, AppResult};
use crate::plugins::api::{WorkflowPlanRequest, WorkflowRunDto};
use crate::plugins::contributions::{
    contribution_menus, list_enabled_plugin_agent_tools, list_plugin_workspace_entries,
    PluginAgentToolDto, PluginWorkspaceEntryDto,
};
use crate::plugins::manifest::model::PluginWorkspaceCategory;
use crate::plugins::marketplace::update::{
    marketplace_listing, MarketplaceListing, UpdateCheckResult,
};
use crate::plugins::marketplace::{apply_revocation_feed, MarketplaceClient};
use crate::plugins::registry::model::{PluginDto, PluginHostMenuDto};
use crate::plugins::registry::review::{ConfirmPluginInstall, PluginInstallReview};
use crate::plugins::registry::PluginRegistryService;
use crate::plugins::secrets::{
    PluginSecretBindRequest, PluginSecretUnbindRequest, SecretBindingMeta,
};
use crate::plugins::workflow::{WorkflowExecutor, WorkflowPlan};
use crate::settings::SettingsService;

#[tauri::command]
pub async fn plugin_list_installed(
    plugins: State<'_, Arc<PluginRegistryService>>,
) -> AppResult<Vec<PluginDto>> {
    plugins.list_installed().await.map_err(AppError::from)
}

#[tauri::command]
pub async fn plugin_inspect_package(
    path: String,
    plugins: State<'_, Arc<PluginRegistryService>>,
    settings: State<'_, SettingsService>,
) -> AppResult<PluginInstallReview> {
    let developer_mode = settings.get().await?.plugin_developer_mode;
    plugins
        .prepare_install(std::path::Path::new(&path), developer_mode, None)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn plugin_install(
    request: ConfirmPluginInstall,
    plugins: State<'_, Arc<PluginRegistryService>>,
    settings: State<'_, SettingsService>,
) -> AppResult<PluginDto> {
    let app_settings = settings.get().await?;
    plugins
        .confirm_install(request, app_settings.plugin_developer_mode)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn plugin_discard_install_review(
    review_id: String,
    plugins: State<'_, Arc<PluginRegistryService>>,
) -> AppResult<()> {
    plugins
        .discard_install_review(&review_id)
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn plugin_marketplace_list(
    plugins: State<'_, Arc<PluginRegistryService>>,
    settings: State<'_, SettingsService>,
) -> AppResult<Vec<MarketplaceListing>> {
    let app_settings = settings.get().await?;
    let client = MarketplaceClient::new(plugins.repository(), app_settings.marketplace_url);
    // Best-effort trust sync for official publisher keys.
    let _ = client.sync_publisher_keys("runory").await;
    let catalog = client.list_plugins().await.map_err(AppError::from)?;
    let repository = plugins.repository();
    catalog
        .into_iter()
        .map(|plugin| {
            let installed = repository.get_active(&plugin.id).map_err(AppError::from)?;
            Ok(marketplace_listing(
                plugin,
                installed.map(|record| record.version),
            ))
        })
        .collect()
}

#[tauri::command]
pub async fn plugin_marketplace_sync_revocations(
    plugins: State<'_, Arc<PluginRegistryService>>,
    settings: State<'_, SettingsService>,
) -> AppResult<usize> {
    let app_settings = settings.get().await?;
    let client = MarketplaceClient::new(plugins.repository(), app_settings.marketplace_url);
    let feed = client.fetch_revocations().await.map_err(AppError::from)?;
    apply_revocation_feed(plugins.repository().as_ref(), &feed).map_err(AppError::from)
}

#[tauri::command]
pub async fn plugin_marketplace_prepare_install(
    plugin_id: String,
    version: Option<String>,
    plugins: State<'_, Arc<PluginRegistryService>>,
    settings: State<'_, SettingsService>,
) -> AppResult<PluginInstallReview> {
    let app_settings = settings.get().await?;
    let client = MarketplaceClient::new(plugins.repository(), app_settings.marketplace_url.clone());
    let _ = client.sync_publisher_keys("runory").await;
    let staging = plugins.root().join(".download");
    let revocations = client.fetch_revocations().await.map_err(AppError::from)?;
    apply_revocation_feed(plugins.repository().as_ref(), &revocations).map_err(AppError::from)?;
    let (package_path, selected) = client
        .download_and_stage(&plugin_id, version.as_deref(), &staging)
        .await
        .map_err(AppError::from)?;
    let result = plugins
        .prepare_install(
            &package_path,
            app_settings.plugin_developer_mode,
            Some((&plugin_id, &selected.version)),
        )
        .await;
    let _ = std::fs::remove_file(&package_path);
    result.map_err(AppError::from)
}

#[tauri::command]
pub async fn plugin_agent_tools(
    plugins: State<'_, Arc<PluginRegistryService>>,
) -> AppResult<Vec<PluginAgentToolDto>> {
    list_enabled_plugin_agent_tools(plugins.inner())
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn plugin_activate_version(
    plugin_id: String,
    version: String,
    plugins: State<'_, Arc<PluginRegistryService>>,
) -> AppResult<PluginDto> {
    plugins
        .activate_version(&plugin_id, &version)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn plugin_check_update(
    plugin_id: String,
    available_version: String,
    candidate_permissions: Vec<String>,
    plugins: State<'_, Arc<PluginRegistryService>>,
) -> AppResult<UpdateCheckResult> {
    let (_record, current) = plugins
        .require_enabled(&plugin_id)
        .map_err(AppError::from)?;
    let mut candidate = current.clone();
    candidate.version = available_version.clone();
    candidate.permissions = candidate_permissions
        .into_iter()
        .map(crate::plugins::manifest::model::PermissionRequest::Simple)
        .collect();
    Ok(crate::plugins::marketplace::update::build_update_check(
        &plugin_id,
        &current.version,
        &available_version,
        &current,
        &candidate,
    ))
}

#[tauri::command]
pub async fn plugin_set_enabled(
    plugin_id: String,
    enabled: bool,
    plugins: State<'_, Arc<PluginRegistryService>>,
) -> AppResult<()> {
    plugins
        .set_enabled(&plugin_id, enabled)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn plugin_uninstall(
    plugin_id: String,
    plugins: State<'_, Arc<PluginRegistryService>>,
) -> AppResult<()> {
    plugins.uninstall(&plugin_id).await.map_err(AppError::from)
}

#[tauri::command]
pub async fn plugin_host_menus(
    host_os: Option<String>,
    host_arch: Option<String>,
    plugins: State<'_, Arc<PluginRegistryService>>,
) -> AppResult<Vec<PluginHostMenuDto>> {
    let installed = plugins.list_installed().await.map_err(AppError::from)?;
    let mut menus = Vec::new();
    for plugin in installed.into_iter().filter(|plugin| plugin.enabled) {
        let Ok((_record, manifest)) = plugins.require_enabled(&plugin.id) else {
            continue;
        };
        menus.extend(contribution_menus(
            &manifest,
            host_os.as_deref(),
            host_arch.as_deref(),
        ));
    }
    Ok(menus)
}

#[tauri::command]
pub async fn plugin_workspace_entries(
    category: Option<PluginWorkspaceCategory>,
    plugins: State<'_, Arc<PluginRegistryService>>,
) -> AppResult<Vec<PluginWorkspaceEntryDto>> {
    list_plugin_workspace_entries(plugins.inner(), plugins.runory_version(), category)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn plugin_workflow_get_form(
    plugin_id: String,
    workflow_id: String,
    executor: State<'_, Arc<WorkflowExecutor>>,
) -> AppResult<serde_json::Value> {
    executor
        .get_form(&plugin_id, &workflow_id)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn plugin_workflow_plan(
    request: WorkflowPlanRequest,
    executor: State<'_, Arc<WorkflowExecutor>>,
) -> AppResult<WorkflowPlan> {
    executor.plan(request).await.map_err(AppError::from)
}

#[tauri::command]
pub async fn plugin_workflow_execute(
    plan_id: String,
    app: tauri::AppHandle,
    executor: State<'_, Arc<WorkflowExecutor>>,
) -> AppResult<WorkflowRunDto> {
    executor
        .execute(&plan_id, Some(app))
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn plugin_workflow_cancel(
    run_id: String,
    executor: State<'_, Arc<WorkflowExecutor>>,
) -> AppResult<()> {
    executor.cancel(&run_id).await.map_err(AppError::from)
}

/// Bind a `secret://…` reference to plaintext in process memory.
/// Never persists to plugins.db; never returns the value.
#[tauri::command]
pub async fn plugin_secret_bind(
    mut request: PluginSecretBindRequest,
    executor: State<'_, Arc<WorkflowExecutor>>,
) -> AppResult<()> {
    let value = Zeroizing::new(std::mem::take(&mut request.value));
    let store = executor
        .secret_store()
        .ok_or(AppError::PluginWorkflowInvalid)?;
    store
        .bind(request.secret_ref, value)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn plugin_secret_unbind(
    request: PluginSecretUnbindRequest,
    executor: State<'_, Arc<WorkflowExecutor>>,
) -> AppResult<()> {
    let store = executor
        .secret_store()
        .ok_or(AppError::PluginWorkflowInvalid)?;
    store
        .unbind(&request.secret_ref)
        .await
        .map_err(AppError::from)
}

/// List bound secret refs (redacted metadata only).
#[tauri::command]
pub async fn plugin_secret_list(
    executor: State<'_, Arc<WorkflowExecutor>>,
) -> AppResult<Vec<SecretBindingMeta>> {
    let store = executor
        .secret_store()
        .ok_or(AppError::PluginWorkflowInvalid)?;
    Ok(store.list_refs().await)
}
