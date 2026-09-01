use serde::{Deserialize, Serialize};

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
