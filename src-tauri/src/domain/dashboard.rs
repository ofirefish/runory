use serde::{Deserialize, Serialize};

use super::SessionId;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardRequest {
    pub session_id: SessionId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceHealthRequest {
    pub session_id: SessionId,
    pub services: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerDashboard {
    pub cpu_usage_percent: f64,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub uptime_seconds: u64,
    pub network_received_bytes: u64,
    pub network_transmitted_bytes: u64,
    pub disks: Vec<DiskUsage>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    pub mount: String,
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub usage_percent: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub user: String,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub command: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceHealth {
    pub name: String,
    pub status: ServiceStatus,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceStatus {
    Active,
    Inactive,
    Failed,
    Unknown,
}
