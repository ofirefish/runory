use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::SessionId;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildPreset {
    None,
    Npm,
    Pnpm,
    Cargo,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum RestartTarget {
    None,
    Systemd { service: String },
    Pm2 { process: String },
    DockerCompose { service: String },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitSetupRequest {
    pub session_id: SessionId,
    pub repository_path: String,
    pub remote_url: String,
    pub branch: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployRequest {
    pub session_id: SessionId,
    pub repository_path: String,
    pub branch: String,
    pub build: BuildPreset,
    pub restart: RestartTarget,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentEntry {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentConfigRequest {
    pub session_id: SessionId,
    pub path: String,
    pub entries: Vec<EnvironmentEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SslInspectRequest {
    pub session_id: SessionId,
    pub domain: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SslIssueRequest {
    pub session_id: SessionId,
    pub domain: String,
    pub email: String,
    pub webroot: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRequest {
    pub session_id: SessionId,
    pub source_path: String,
    pub destination_directory: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CronSchedule {
    Hourly,
    Daily,
    Weekly,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum CronTask {
    Backup {
        source_path: String,
        destination_directory: String,
    },
    ServiceRestart {
        service: String,
    },
    GitPull {
        repository_path: String,
        branch: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronAddRequest {
    pub session_id: SessionId,
    pub schedule: CronSchedule,
    pub task: CronTask,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronRemoveRequest {
    pub session_id: SessionId,
    pub cron_id: Uuid,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CronEntry {
    pub id: Uuid,
    pub schedule: CronSchedule,
    pub task_kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentRecord {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub operation: String,
    pub target: String,
    pub started_at_epoch_seconds: u64,
    pub success: bool,
    pub error_code: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentHistoryRequest {
    pub profile_id: Option<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentApp {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub name: String,
    pub repository_path: String,
    pub remote_url: String,
    pub branch: String,
    pub build: BuildPreset,
    pub restart: RestartTarget,
    pub updated_at_epoch_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentAppsListRequest {
    pub profile_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentAppUpsertRequest {
    pub id: Option<Uuid>,
    pub profile_id: Uuid,
    pub name: String,
    pub repository_path: String,
    pub remote_url: String,
    pub branch: String,
    pub build: BuildPreset,
    pub restart: RestartTarget,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentAppDeleteRequest {
    pub id: Uuid,
    pub profile_id: Uuid,
}
