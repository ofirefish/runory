// Agent Runtime V2 domain contracts (AR2-A). Public library API with no
// production caller yet; the legacy Agent paths run unchanged until AR2-B+.
pub mod agent;
mod agentic;
mod cloud;
mod commands;
mod credentials;
mod dashboard;
mod deployment;
mod domain;
mod groups;
mod known_hosts;
mod mcp;
mod operations;
mod policy;
mod profiles;
mod settings;
mod skills;
mod ssh;
mod storage;
// Native tools intentionally have no runtime/UI caller until a later Agent Runtime phase.
#[allow(dead_code)]
mod tools;
mod transfers;

use ssh::ServerSessionManager;
use std::path::Path;
use std::sync::Arc;
use tauri::{DragDropEvent, Manager, WindowEvent};
use tokio::sync::Mutex;

use agent::AgentRuntimeV2Service;
use agentic::{
    AgentRuntimeService, ChangeSetService, FleetExecutionService, IncidentService, ModelGateway,
    ObservationCache,
};
use cloud::{CloudPolicyService, CloudSyncService, CloudSyncStateRepository};
use deployment::{DeploymentHistoryRepository, DeploymentService};
use groups::{GroupRepository, GroupService};
use known_hosts::{KnownHostRepository, KnownHostService};
use mcp::{McpConfigRepository, McpGateway};
use policy::AgentPolicyService;
use profiles::{ProfileRepository, ProfileService};
use settings::{SettingsRepository, SettingsService};
use skills::SkillRegistry;
use storage::{CatalogDatabase, JsonRepository};
use tools::{NativeToolExecutionService, ToolAuditRepository};
use transfers::{LocalFileGrantService, UploadDirectoryHistoryService};

use credentials::{CredentialService, CredentialVault, PlatformKeyStore};
#[cfg(not(mobile))]
use credentials::{NativePlatformKeyStore, StrongholdCredentialVault};
#[cfg(mobile)]
use credentials::{PortableCredentialVault, UnavailablePlatformKeyStore};

fn credential_service(data_directory: &Path) -> CredentialService {
    #[cfg(not(mobile))]
    let vault: Arc<dyn CredentialVault> = Arc::new(StrongholdCredentialVault::new(
        data_directory.join("credentials.hold"),
        data_directory.join("credentials.salt"),
    ));
    #[cfg(mobile)]
    let vault: Arc<dyn CredentialVault> = Arc::new(PortableCredentialVault::new(
        data_directory.join("credentials.mobile.vault"),
    ));

    #[cfg(not(mobile))]
    let platform_key_store: Arc<dyn PlatformKeyStore> = Arc::new(NativePlatformKeyStore::new());
    #[cfg(mobile)]
    let platform_key_store: Arc<dyn PlatformKeyStore> = Arc::new(UnavailablePlatformKeyStore);

    CredentialService::with_platform_key_store(vault, platform_key_store)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ServerSessionManager::default())
        // The assistant panel is LLM-backed: the provider talks to the
        // configured model through ModelGateway and only classifies its
        // output. Registration happens in `setup` once the gateway exists.
        .manage(LocalFileGrantService::default())
        .on_window_event(|window, event| {
            let WindowEvent::DragDrop(event) = event else {
                return;
            };
            match event {
                DragDropEvent::Drop { paths, .. } => {
                    let paths = paths.clone();
                    window
                        .state::<LocalFileGrantService>()
                        .stage_upload_drop(paths);
                }
                _ => {}
            }
        })
        .setup(|app| {
            #[cfg(mobile)]
            app.handle().plugin(tauri_plugin_biometric::init())?;
            let data_directory = app.path().app_data_dir()?;
            let credentials = credential_service(&data_directory);
            if let Err(error) = tauri::async_runtime::block_on(credentials.auto_unlock()) {
                tracing::warn!(
                    error_code = error.code(),
                    "credential Vault automatic unlock was unavailable"
                );
            }
            let write_lock = Arc::new(Mutex::new(()));
            let settings = SettingsService::new(
                SettingsRepository::new(JsonRepository::new(data_directory.join("settings.json"))),
                Arc::clone(&write_lock),
            );
            let models = Arc::new(
                ModelGateway::at_path(data_directory.join("agent-model.json"))?
                    .with_credentials(credentials.clone())
                    .with_settings(settings.clone()),
            );
            tauri::async_runtime::block_on(models.load())?;
            app.manage(models);
            let agent_policy = AgentPolicyService::at_path(
                data_directory.join("agent-policy.json"),
                data_directory.join("agent-policy-audit.json"),
            );
            tauri::async_runtime::block_on(agent_policy.load())?;
            // Rust-only Policy → Risk → Approval → Audit → Registry service; no generic IPC.
            app.manage(
                NativeToolExecutionService::approved_repair_with_agent_policy(
                    ToolAuditRepository::new(JsonRepository::new(
                        data_directory.join("tool-audit.json"),
                    )),
                    agent_policy.clone(),
                ),
            );
            app.manage(AgentRuntimeService::default());
            app.manage(
                AgentRuntimeV2Service::open(&data_directory)
                    .expect("agent runtime v2 database must open"),
            );
            app.manage(ObservationCache::default());
            app.manage(agent_policy);
            let incidents = IncidentService::at_path(data_directory.join("agentic-incidents.json"));
            tauri::async_runtime::block_on(incidents.load())?;
            app.manage(incidents);
            let change_sets =
                ChangeSetService::at_path(data_directory.join("agentic-change-sets.json"));
            tauri::async_runtime::block_on(change_sets.load())?;
            app.manage(change_sets);
            let fleet_runs =
                FleetExecutionService::at_path(data_directory.join("agentic-fleet-runs.json"));
            tauri::async_runtime::block_on(fleet_runs.load())?;
            app.manage(fleet_runs);
            app.manage(SkillRegistry::at_path(data_directory.join("skills"))?);
            let mcp_gateway = McpGateway::new(
                McpConfigRepository::new(JsonRepository::new(
                    data_directory.join("mcp-config.json"),
                )),
                JsonRepository::new(data_directory.join("mcp-audit.json")),
            );
            tauri::async_runtime::block_on(mcp_gateway.load())?;
            app.manage(mcp_gateway);
            // SSH metadata is intentionally separate from the credential Vault and Agent runtime.
            // No legacy JSON import occurs: a new catalog starts with an empty `runory.db`.
            let catalog_database = CatalogDatabase::open(data_directory.join("runory.db"))?;
            let group_repository = GroupRepository::new(catalog_database.clone());
            let profile_repository = ProfileRepository::new(catalog_database.clone());
            let known_host_repository = KnownHostRepository::new(catalog_database);
            app.manage(GroupService::new(
                group_repository.clone(),
                profile_repository.clone(),
                Arc::clone(&write_lock),
            ));
            app.manage(CloudSyncService::new(
                profile_repository.clone(),
                group_repository.clone(),
                Arc::clone(&write_lock),
                CloudSyncStateRepository::at_path(data_directory.join("cloud-sync-state.json")),
            ));
            let cloud_policy =
                CloudPolicyService::at_path(data_directory.join("cloud-policy-bindings.json"))?;
            tauri::async_runtime::block_on(cloud_policy.load())?;
            app.manage(cloud_policy);
            app.manage(settings);
            app.manage(ProfileService::new(
                profile_repository,
                group_repository,
                write_lock,
            ));
            app.manage(KnownHostService::new(
                known_host_repository,
                Arc::new(Mutex::new(())),
            ));
            app.manage(credentials);
            app.manage(UploadDirectoryHistoryService::at_path(
                data_directory.join("upload-directory-history.json"),
            ));
            app.manage(DeploymentService::new(DeploymentHistoryRepository::new(
                JsonRepository::new(data_directory.join("deployment-history.json")),
            )));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::agentic::agent_model_get,
            commands::agentic::agent_model_profiles,
            commands::agentic::agent_model_profile_save,
            commands::agentic::agent_model_profile_activate,
            commands::agentic::agent_model_profile_remove,
            commands::agentic::agent_model_configure,
            commands::agentic::agent_model_clear_api_key,
            commands::agentic::agent_model_test,
            commands::agentic::agent_model_oauth_start,
            commands::agentic::agent_model_oauth_cancel,
            commands::agentic::agent_model_disconnect,
            commands::agentic::agent_incident_run,
            commands::agentic::agent_incident_get,
            commands::agentic::agent_incident_list,
            commands::agentic::agent_incident_attach_changeset,
            commands::agentic::agent_incident_refresh,
            commands::agentic::agent_incident_handoff,
            commands::agentic::agent_incident_close,
            commands::agentic::agent_incident_export,
            commands::agent_v2::agent_v2_run_start,
            commands::agent_v2::agent_v2_run_subscribe,
            commands::agent_v2::agent_v2_run_approve,
            commands::agent_v2::agent_v2_run_reject,
            commands::agent_v2::agent_v2_run_reply,
            commands::agent_v2::agent_v2_run_cancel,
            commands::agent_v2::agent_v2_run_pause,
            commands::agent_v2::agent_v2_run_resume,
            commands::agent_v2::agent_v2_list_resumable_runs,
            commands::agent_v2::agent_v2_history_list,
            commands::agent_v2::agent_v2_history_get,
            commands::agent_v2::agent_v2_bind_resumable_run,
            commands::agentic::agent_policy_effective,
            commands::agentic::agent_skills_list,
            commands::agentic::agent_skills_refresh,
            commands::agentic::agent_skill_set_enabled,
            commands::agentic::agent_mcp_configure,
            commands::agentic::agent_mcp_list,
            commands::agentic::agent_mcp_set_tool_enabled,
            commands::agentic::agent_mcp_set_enabled,
            commands::agentic::agent_mcp_remove,
            commands::catalog::group_list,
            commands::catalog::group_create,
            commands::catalog::group_update,
            commands::catalog::group_delete,
            commands::catalog::group_reorder,
            commands::catalog::profile_list,
            commands::catalog::profile_create,
            commands::catalog::profile_update,
            commands::catalog::profile_delete,
            commands::catalog::profile_reorder,
            commands::cloud::cloud_sync_export,
            commands::cloud::cloud_sync_preview,
            commands::cloud::cloud_sync_apply,
            commands::cloud::cloud_sync_discard,
            commands::cloud_policy::cloud_policy_bind,
            commands::cloud_policy::cloud_policy_refresh,
            commands::cloud_policy::cloud_policy_lock,
            commands::cloud_policy::cloud_policy_unbind,
            commands::cloud_policy::cloud_policy_status,
            commands::known_hosts::known_host_prepare,
            commands::known_hosts::known_host_trust,
            commands::known_hosts::known_host_cancel,
            commands::known_hosts::known_host_list,
            commands::known_hosts::known_host_remove,
            commands::credentials::vault_unlock,
            commands::credentials::vault_initialize,
            commands::credentials::vault_unlock_with_platform,
            commands::credentials::vault_lock,
            commands::credentials::credential_status,
            commands::credentials::credential_forget,
            commands::credentials::private_key_import,
            commands::credentials::private_key_forget,
            commands::ssh::ssh_connect,
            commands::ssh::ssh_reconnect,
            commands::ssh::ssh_test,
            commands::ssh::ssh_write,
            commands::ssh::ssh_resize,
            commands::ssh::ssh_disconnect,
            commands::sftp::sftp_open,
            commands::sftp::list_directory,
            commands::sftp::stat,
            commands::sftp::sftp_preview_image,
            commands::sftp::sftp_preview_text,
            commands::sftp::change_directory,
            commands::sftp::refresh,
            commands::sftp::create_directory,
            commands::sftp::rename,
            commands::sftp::delete,
            commands::sftp::sftp_select_upload_files,
            commands::sftp::sftp_list_upload_directories,
            commands::sftp::sftp_accept_latest_upload_drop,
            commands::sftp::sftp_select_download_target,
            commands::sftp::sftp_start_upload,
            commands::sftp::sftp_start_download,
            commands::sftp::sftp_transfer_list,
            commands::sftp::sftp_transfer_subscribe,
            commands::sftp::sftp_transfer_cancel,
            commands::sftp::sftp_transfer_retry,
            commands::dashboard::dashboard_overview,
            commands::dashboard::dashboard_processes,
            commands::dashboard::dashboard_service_health,
            commands::operations::docker_list,
            commands::operations::docker_action,
            commands::operations::pm2_list,
            commands::operations::pm2_action,
            commands::operations::nginx_action,
            commands::operations::logs_read,
            commands::settings::settings_get,
            commands::settings::settings_update,
            commands::deployment::deployment_git_setup,
            commands::deployment::deployment_run,
            commands::deployment::deployment_environment_write,
            commands::deployment::deployment_ssl_inspect,
            commands::deployment::deployment_ssl_issue,
            commands::deployment::deployment_backup,
            commands::deployment::deployment_cron_list,
            commands::deployment::deployment_cron_add,
            commands::deployment::deployment_cron_remove,
            commands::deployment::deployment_history
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| eprintln!("failed to run Runory: {error}"));
}
