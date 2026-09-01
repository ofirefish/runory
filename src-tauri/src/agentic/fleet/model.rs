use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::super::{ApprovalState, ChangeSetDraftRequest};
use crate::policy::{PolicyEvaluation, PolicySnapshot};
use crate::tools::RiskLevel;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ExecutionStrategy {
    Sequential,
    Parallel,
    Canary,
    RollingBatch,
}

impl Default for ExecutionStrategy {
    fn default() -> Self {
        Self::Sequential
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum FailurePolicy {
    Stop,
    PauseForReview,
    Continue,
    Rollback,
}

impl Default for FailurePolicy {
    fn default() -> Self {
        Self::PauseForReview
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum FleetExecutionState {
    Draft,
    Approved,
    Executing,
    Verifying,
    PausedForReview,
    Succeeded,
    Failed,
    RollingBack,
    RolledBack,
    RollbackFailed,
    Interrupted,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum FleetTargetState {
    Pending,
    Executing,
    Succeeded,
    Failed,
    RollbackPending,
    RolledBack,
    RollbackFailed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum VerificationState {
    Pending,
    Succeeded,
    Failed,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum FleetRecoveryState {
    Live,
    MetadataOnly,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MultiChangeSetDraftRequest {
    pub agent_run_id: Uuid,
    pub title: String,
    pub targets: Vec<ChangeSetDraftRequest>,
    #[serde(default)]
    pub execution_strategy: ExecutionStrategy,
    #[serde(default)]
    pub failure_policy: FailurePolicy,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_canary_count")]
    pub canary_count: usize,
    #[serde(default = "default_production")]
    pub production: bool,
    pub service_verification: Option<String>,
    #[serde(default = "default_cross_target_verification")]
    pub cross_target_verification: bool,
}

const fn default_batch_size() -> usize {
    2
}

const fn default_canary_count() -> usize {
    1
}

const fn default_production() -> bool {
    true
}

const fn default_cross_target_verification() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TargetApprovalBinding {
    pub target_id: Uuid,
    pub change_set_id: Uuid,
    pub change_set_version: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FleetApprovalBinding {
    pub fleet_version: u64,
    pub target_ids: Vec<Uuid>,
    pub targets: Vec<TargetApprovalBinding>,
    pub approved_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FleetTargetExecution {
    pub target_id: Uuid,
    pub change_set_id: Uuid,
    pub change_set_version: u64,
    pub state: FleetTargetState,
    pub local_verification: VerificationState,
    pub service_verification: VerificationState,
    pub rollback_state: VerificationState,
    pub error_code: Option<String>,
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FleetVerification {
    pub cross_target: VerificationState,
    pub service_level: VerificationState,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FleetAuditEvent {
    pub state: FleetExecutionState,
    pub target_id: Option<Uuid>,
    pub code: String,
    pub occurred_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MultiChangeSet {
    pub id: Uuid,
    pub agent_run_id: Uuid,
    pub model: &'static str,
    pub title: String,
    pub version: u64,
    pub risk: RiskLevel,
    pub execution_strategy: ExecutionStrategy,
    pub failure_policy: FailurePolicy,
    pub batch_size: usize,
    pub canary_count: usize,
    pub production: bool,
    pub service_verification: Option<String>,
    pub cross_target_verification: bool,
    pub targets: Vec<FleetTargetExecution>,
    pub approval_state: ApprovalState,
    pub approval: Option<FleetApprovalBinding>,
    pub last_approval: Option<FleetApprovalBinding>,
    pub execution_state: FleetExecutionState,
    pub verification: FleetVerification,
    pub recovery_state: FleetRecoveryState,
    pub tool_call_count: u32,
    pub started_at_epoch_ms: Option<u64>,
    pub completed_at_epoch_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub audit: Vec<FleetAuditEvent>,
    pub policy_evaluation: Option<PolicyEvaluation>,
    pub policy_snapshot: Option<PolicySnapshot>,
}
