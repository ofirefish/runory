use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::SessionId;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationsRequest {
    pub session_id: SessionId,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceAction {
    Start,
    Stop,
    Restart,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerActionRequest {
    pub session_id: SessionId,
    pub container: String,
    pub action: ResourceAction,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub ports: String,
    pub cpu_percent: f64,
    pub memory_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerImage {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
    pub created_at_epoch_seconds: u64,
    pub used_by: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DockerImageAction {
    Pull {
        reference: String,
    },
    Remove {
        ids: Vec<String>,
    },
    Prune,
    CreateContainer {
        image: String,
        name: String,
        #[serde(default)]
        publish_ports: Vec<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerImageActionRequest {
    pub session_id: SessionId,
    pub action: DockerImageAction,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerNetwork {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub ipv4_subnet: String,
    pub ipv4_gateway: String,
    pub labels: String,
    pub created_at_epoch_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DockerNetworkAction {
    Create {
        name: String,
        #[serde(default = "default_network_driver")]
        driver: String,
        #[serde(default)]
        subnet: String,
        #[serde(default)]
        gateway: String,
        #[serde(default)]
        labels: Vec<String>,
    },
    Remove {
        ids: Vec<String>,
    },
    Prune,
}

fn default_network_driver() -> String {
    "bridge".into()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerNetworkActionRequest {
    pub session_id: SessionId,
    pub action: DockerNetworkAction,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerVolume {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub scope: String,
    pub labels: String,
    pub created_at_epoch_seconds: u64,
    pub used_by: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DockerVolumeAction {
    Create {
        name: String,
        #[serde(default = "default_volume_driver")]
        driver: String,
        #[serde(default)]
        labels: Vec<String>,
    },
    Remove {
        names: Vec<String>,
    },
    Prune,
}

fn default_volume_driver() -> String {
    "local".into()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerVolumeActionRequest {
    pub session_id: SessionId,
    pub action: DockerVolumeAction,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerOnlineImage {
    pub name: String,
    pub description: String,
    pub star_count: u64,
    pub is_official: bool,
    pub is_automated: bool,
    /// Recent Hub tags for this repository (newest-first when available).
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerImagesSearchRequest {
    pub session_id: SessionId,
    pub query: String,
    #[serde(default = "default_docker_search_limit")]
    pub limit: u32,
}

fn default_docker_search_limit() -> u32 {
    25
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pm2ActionRequest {
    pub session_id: SessionId,
    pub process: String,
    pub action: ResourceAction,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pm2Process {
    pub id: u32,
    pub name: String,
    pub status: String,
    pub cpu_percent: f64,
    pub memory_bytes: u64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NginxAction {
    Test,
    Reload,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NginxActionRequest {
    pub session_id: SessionId,
    pub action: NginxAction,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LogSource {
    System,
    Auth,
    NginxAccess,
    NginxError,
    Docker,
    Pm2,
    Service,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRequest {
    pub session_id: SessionId,
    pub source: LogSource,
    pub target: Option<String>,
    pub lines: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    pub success: bool,
    pub output: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerLogOpts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_file: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerDaemonConfig {
    #[serde(default)]
    pub registry_mirrors: Vec<String>,
    #[serde(default)]
    pub insecure_registries: Vec<String>,
    #[serde(default)]
    pub log_driver: String,
    #[serde(default)]
    pub log_opts: DockerLogOpts,
    #[serde(default)]
    pub live_restore: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerEngineInfo {
    pub server_version: String,
    pub storage_driver: String,
    pub logging_driver: String,
    pub operating_system: String,
    pub architecture: String,
    pub ncpu: u32,
    pub mem_total_bytes: u64,
    pub docker_root_dir: String,
    pub live_restore_enabled: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerEngineSettingsView {
    pub info: DockerEngineInfo,
    pub config: DockerDaemonConfig,
    pub config_path: String,
    pub config_exists: bool,
    pub config_raw_preserved: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerSettingsApplyRequest {
    pub session_id: SessionId,
    pub config: DockerDaemonConfig,
    #[serde(default = "default_docker_settings_restart")]
    pub restart: bool,
}

fn default_docker_settings_restart() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerRegistry {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub url: String,
    pub name: String,
    pub username: String,
    pub namespace: String,
    pub remarks: String,
    pub updated_at_epoch_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerRegistryUpsertRequest {
    pub session_id: SessionId,
    pub id: Option<Uuid>,
    pub url: String,
    pub name: String,
    pub username: String,
    /// Transient secret for `docker login`. Required on create; optional on update
    /// (omit or empty to keep remote credentials and only update metadata).
    #[serde(default)]
    pub password: String,
    pub namespace: String,
    #[serde(default)]
    pub remarks: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerRegistryDeleteRequest {
    pub session_id: SessionId,
    pub ids: Vec<Uuid>,
}
