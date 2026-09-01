use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::changes::ChangeSet;
use super::context::{AgentContextItem, ContextBudget, ContextSnapshot, ContextTrust};
use crate::tools::{NativeToolName, RiskLevel, ToolResult};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AgentRunState {
    GatheringContext,
    Investigating,
    Diagnosing,
    NeedsInput,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    PolicyBlocked,
    BudgetExceeded,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AgentProgressStage {
    GatheringContext,
    Planning,
    RunningTools,
    DraftingChangeSet,
    Complete,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentProgress {
    pub stage: AgentProgressStage,
    pub tool_names: Vec<NativeToolName>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct AgentBudget {
    pub max_model_calls: u32,
    pub max_tool_calls: u32,
    pub max_input_tokens: u32,
    pub max_output_tokens: u32,
    pub time_budget_ms: u64,
    pub max_cost_microusd: Option<u64>,
    pub context: ContextBudget,
}

impl Default for AgentBudget {
    fn default() -> Self {
        Self {
            max_model_calls: 4,
            max_tool_calls: 20,
            max_input_tokens: 16_384,
            max_output_tokens: 4_096,
            time_budget_ms: 5 * 60 * 1_000,
            max_cost_microusd: None,
            context: ContextBudget::default(),
        }
    }
}

impl AgentBudget {
    pub(crate) fn is_valid(self) -> bool {
        (1..=32).contains(&self.max_model_calls)
            && (1..=100).contains(&self.max_tool_calls)
            && (256..=1_000_000).contains(&self.max_input_tokens)
            && (64..=100_000).contains(&self.max_output_tokens)
            && (1_000..=30 * 60 * 1_000).contains(&self.time_budget_ms)
            && self.context.max_items > 0
            && self.context.max_items <= 256
            && self.context.max_bytes > 0
            && self.context.max_bytes <= 1024 * 1024
            && self.context.max_tokens > 0
            && self.context.max_tokens <= self.max_input_tokens
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentDoctorRequest {
    pub run_id: Uuid,
    pub session_id: Uuid,
    pub user_request: String,
    pub service: Option<String>,
    pub http_url: Option<String>,
    pub port_host: Option<String>,
    pub port: Option<u16>,
    pub include_nginx_test: bool,
    pub skill_id: Option<String>,
    pub mcp_context: Option<McpContextRequest>,
    pub incident_id: Option<Uuid>,
    pub budget: Option<AgentBudget>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpContextRequest {
    pub server_id: Uuid,
    pub tool_name: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentToolActivity {
    pub invocation_id: Uuid,
    pub tool_name: NativeToolName,
    pub success: bool,
    pub error_code: Option<&'static str>,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Evidence {
    pub id: Uuid,
    pub source: String,
    pub invocation_id: Uuid,
    pub trust: ContextTrust,
    pub summary: &'static str,
    pub result: ToolResult,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Diagnosis {
    pub id: Uuid,
    pub title_code: String,
    pub root_cause_code: String,
    pub confidence: f32,
    pub evidence_ids: Vec<Uuid>,
    pub affected_components: Vec<String>,
    pub alternative_codes: Vec<String>,
    pub recommended_action_code: String,
    pub risk: RiskLevel,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentRun {
    pub id: Uuid,
    pub model: String,
    pub session_id: Uuid,
    pub state: AgentRunState,
    pub context: Vec<AgentContextItem>,
    pub context_snapshot: ContextSnapshot,
    pub activities: Vec<AgentToolActivity>,
    pub evidence: Vec<Evidence>,
    pub external_evidence: Vec<ExternalEvidence>,
    pub diagnosis: Option<Diagnosis>,
    pub answer: Option<String>,
    pub answer_evidence_ids: Vec<Uuid>,
    pub goal_achieved: bool,
    pub clarification_question: Option<String>,
    pub failure_code: Option<&'static str>,
    pub change_set: Option<ChangeSet>,
    pub max_tool_calls: u32,
    pub used_tool_calls: u32,
    pub max_model_tokens: u32,
    pub used_model_tokens: u32,
    pub timeout_ms: u64,
    pub started_at_epoch_ms: u64,
    pub completed_at_epoch_ms: u64,
    pub metrics: AgentRunMetrics,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentRunMetrics {
    pub run_id: Uuid,
    pub incident_id: Option<Uuid>,
    pub model_calls: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub tool_calls: u32,
    pub duplicate_calls: u32,
    pub mcp_calls: u32,
    pub context_size_bytes: usize,
    pub compaction_count: u32,
    pub duration_ms: u64,
    pub diagnosis_latency_ms: Option<u64>,
    pub resolution_latency_ms: Option<u64>,
    pub verification_result: Option<String>,
    pub rollback_result: Option<String>,
    pub estimated_cost_microusd: Option<u64>,
    pub cache_hits: u32,
    pub parallel_read_batches: u32,
}

impl AgentRunMetrics {
    pub(crate) fn new(run_id: Uuid, incident_id: Option<Uuid>) -> Self {
        Self {
            run_id,
            incident_id,
            model_calls: 0,
            input_tokens: 0,
            output_tokens: 0,
            tool_calls: 0,
            duplicate_calls: 0,
            mcp_calls: 0,
            context_size_bytes: 0,
            compaction_count: 0,
            duration_ms: 0,
            diagnosis_latency_ms: None,
            resolution_latency_ms: None,
            verification_result: None,
            rollback_result: None,
            estimated_cost_microusd: None,
            cache_hits: 0,
            parallel_read_batches: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExternalEvidence {
    pub id: Uuid,
    pub source: String,
    pub trust: ContextTrust,
    pub data: Value,
}

pub(crate) fn validate_request(request: &AgentDoctorRequest) -> bool {
    !request.user_request.trim().is_empty()
        && request.user_request.len() <= 8 * 1024
        && request.service.as_deref().is_none_or(valid_identifier)
        && request
            .http_url
            .as_deref()
            .is_none_or(|value| value.len() <= 2048)
        && request.port_host.as_deref().is_none_or(|value| {
            !value.is_empty()
                && value.len() <= 255
                && !value
                    .chars()
                    .any(|item| item.is_control() || item.is_whitespace())
        })
        && request.port.is_none_or(|value| value > 0)
        && request.port_host.is_some() == request.port.is_some()
        && request
            .skill_id
            .as_deref()
            .is_none_or(|value| !value.is_empty() && value.len() <= 128)
        && request.mcp_context.as_ref().is_none_or(|value| {
            !value.tool_name.is_empty()
                && value.tool_name.len() <= 128
                && value.arguments.is_object()
        })
        && request.budget.unwrap_or_default().is_valid()
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'@' | b':' | b'-')
        })
}
