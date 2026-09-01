use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AiRisk, DiskUsage, DockerContainer, ProcessInfo, SessionId};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AiTerminalPreset {
    DiskUsage,
    MemoryUsage,
    ListeningPorts,
    RecentErrors,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "tool",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum AiToolInput {
    TerminalExec { preset: AiTerminalPreset },
    FileRead { path: String },
    FileWrite { path: String, content: String },
    SystemMetrics,
    ProcessList,
    DockerList,
    DockerRestart { container: String },
    NginxTest,
    NginxReload,
}

impl AiToolInput {
    pub const fn name(&self) -> AiToolName {
        match self {
            Self::TerminalExec { .. } => AiToolName::TerminalExec,
            Self::FileRead { .. } => AiToolName::FileRead,
            Self::FileWrite { .. } => AiToolName::FileWrite,
            Self::SystemMetrics => AiToolName::SystemMetrics,
            Self::ProcessList => AiToolName::ProcessList,
            Self::DockerList => AiToolName::DockerList,
            Self::DockerRestart { .. } => AiToolName::DockerRestart,
            Self::NginxTest => AiToolName::NginxTest,
            Self::NginxReload => AiToolName::NginxReload,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AiToolName {
    TerminalExec,
    FileRead,
    FileWrite,
    SystemMetrics,
    ProcessList,
    DockerList,
    DockerRestart,
    NginxTest,
    NginxReload,
}

#[derive(Clone, Debug, Serialize)]
#[serde(
    tag = "tool",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum AiToolSummary {
    TerminalExec { preset: AiTerminalPreset },
    FileRead { path: String },
    FileWrite { path: String, bytes: u64 },
    SystemMetrics,
    ProcessList,
    DockerList,
    DockerRestart { container: String },
    NginxTest,
    NginxReload,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPlanRequest {
    pub session_ids: Vec<SessionId>,
    pub goal: String,
    pub tools: Vec<AiToolInput>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AiStepStatus {
    PendingApproval,
    Approved,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPlanStep {
    pub id: Uuid,
    pub session_id: SessionId,
    pub tool: AiToolSummary,
    pub risk: AiRisk,
    pub status: AiStepStatus,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAgentPlan {
    pub id: Uuid,
    pub goal: String,
    pub steps: Vec<AiPlanStep>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPlanStepRequest {
    pub plan_id: Uuid,
    pub step_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPlanGetRequest {
    pub plan_id: Uuid,
}

#[derive(Clone, Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum AiToolOutput {
    Text {
        value: String,
    },
    File {
        path: String,
        content: String,
    },
    FileWritten {
        path: String,
        bytes: u64,
    },
    Metrics {
        cpu_usage_percent: f64,
        memory_used_bytes: u64,
        memory_total_bytes: u64,
        uptime_seconds: u64,
        network_received_bytes: u64,
        network_transmitted_bytes: u64,
        disks: Vec<DiskUsage>,
    },
    Processes {
        processes: Vec<ProcessInfo>,
    },
    Containers {
        containers: Vec<DockerContainer>,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolExecution {
    pub plan: AiAgentPlan,
    pub step_id: Uuid,
    pub output: AiToolOutput,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAuditRecord {
    pub id: Uuid,
    pub plan_id: Uuid,
    pub step_id: Uuid,
    pub profile_id: Uuid,
    pub tool: AiToolName,
    pub risk: AiRisk,
    pub target: Option<String>,
    pub started_at_epoch_seconds: u64,
    pub succeeded: bool,
    pub error_code: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAuditListRequest {
    pub profile_id: Option<Uuid>,
}
