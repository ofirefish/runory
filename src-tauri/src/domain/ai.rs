use serde::{Deserialize, Serialize};

use super::SessionId;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiTask {
    ExplainCommand,
    GenerateCommand,
    DiagnoseOutput,
    ProposeFix,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiRisk {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiPurpose {
    DiskUsage,
    MemoryUsage,
    ProcessList,
    NetworkSockets,
    ReadLogs,
    ContainerList,
    ServiceStatus,
    PackageManagement,
    FileInspection,
    FileMutation,
    SystemControl,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiSignal {
    ReadOnly,
    ElevatedPrivileges,
    DestructiveFileOperation,
    NetworkDownload,
    ShellPipeline,
    OutputRedirection,
    ServiceMutation,
    ContainerMutation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiDiagnosis {
    PermissionDenied,
    CommandNotFound,
    DiskFull,
    PortInUse,
    ConnectionRefused,
    OutOfMemory,
    ResourceNotFound,
    AuthenticationFailed,
    TimedOut,
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCommandRequest {
    pub session_id: SessionId,
    pub command: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiGenerateRequest {
    pub session_id: SessionId,
    pub intent: String,
    /// Commands already executed in the current session context. The plan
    /// builder excludes them so "继续下一步" never repeats a settled step.
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiOutputRequest {
    pub session_id: SessionId,
    pub output: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCommandProposal {
    pub command: String,
    pub risk: AiRisk,
    pub purpose: AiPurpose,
    pub requires_confirmation: bool,
}

/// A multi-command execution plan produced by the model. Steps are shown to
/// the user for per-command confirmation before anything enters a terminal.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPlanProposal {
    pub summary: String,
    pub commands: Vec<AiCommandProposal>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAssistantResponse {
    pub task: AiTask,
    pub provider: &'static str,
    pub risk: AiRisk,
    pub purpose: AiPurpose,
    pub signals: Vec<AiSignal>,
    pub diagnosis: Option<AiDiagnosis>,
    pub context_used: bool,
    pub proposals: Vec<AiCommandProposal>,
}
