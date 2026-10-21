use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum WorkflowRunState {
    Created,
    Planning,
    AwaitingApproval,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    RollingBack,
    RolledBack,
    Interrupted,
}

impl WorkflowRunState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "Created",
            Self::Planning => "Planning",
            Self::AwaitingApproval => "AwaitingApproval",
            Self::Running => "Running",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
            Self::RollingBack => "RollingBack",
            Self::RolledBack => "RolledBack",
            Self::Interrupted => "Interrupted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum WorkflowStepState {
    Pending,
    Skipped,
    Planned,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl WorkflowStepState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Skipped => "Skipped",
            Self::Planned => "Planned",
            Self::Running => "Running",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunRecord {
    pub run_id: String,
    pub plugin_id: String,
    pub plugin_version: String,
    pub workflow_id: String,
    pub host_id: Option<String>,
    pub state: WorkflowRunState,
    pub steps: Vec<WorkflowStepRecord>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepRecord {
    pub step_id: String,
    pub action_id: String,
    pub state: WorkflowStepState,
    pub summary: Option<String>,
    pub error_code: Option<String>,
    #[serde(default)]
    pub stdout: Option<String>,
    #[serde(default)]
    pub stderr: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
}
