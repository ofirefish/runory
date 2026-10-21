use serde::{Deserialize, Serialize};

use crate::plugins::changeset::PluginChangeSetSummary;
use crate::plugins::workflow::state::{WorkflowRunState, WorkflowStepState};
use crate::tools::RiskLevel;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowPlanRequest {
    pub plugin_id: String,
    pub workflow_id: String,
    #[serde(default)]
    pub host_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub inputs: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowPlanDto {
    pub id: String,
    pub plugin_id: String,
    pub plugin_version: String,
    pub workflow_id: String,
    #[serde(default)]
    pub host_id: Option<String>,
    pub risk: RiskLevel,
    pub requires_approval: bool,
    pub operations: Vec<PlannedOperationDto>,
    #[serde(default)]
    pub changeset: Option<PluginChangeSetSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedOperationDto {
    pub step_id: String,
    pub action_id: String,
    pub summary: String,
    pub risk: RiskLevel,
    pub requires_approval: bool,
    pub has_runtime_values: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowExecuteRequest {
    pub plan_id: String,
    #[serde(default)]
    pub plugin_id: Option<String>,
    #[serde(default)]
    pub workflow_id: Option<String>,
    #[serde(default)]
    pub host_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub inputs: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunDto {
    pub run_id: String,
    pub plugin_id: String,
    pub plugin_version: String,
    pub workflow_id: String,
    #[serde(default)]
    pub host_id: Option<String>,
    pub state: WorkflowRunState,
    pub steps: Vec<WorkflowStepStatusDto>,
    #[serde(default)]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepStatusDto {
    pub step_id: String,
    pub action_id: String,
    pub state: WorkflowStepState,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub stdout: Option<String>,
    #[serde(default)]
    pub stderr: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCancelRequest {
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowFormDto {
    pub plugin_id: String,
    pub workflow_id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
}
