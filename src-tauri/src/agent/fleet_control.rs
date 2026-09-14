//! Content-free Fleet approval and event contracts.
//!
//! Fleet approval never authorizes a shell command directly. It only seals a
//! validated orchestration graph. M3 may claim an approved graph and must
//! still route every write through the existing ChangeSet approval boundary.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::fleet_run::{FleetRecoveryStateV2, FleetRunStateV2, FleetRunV2};
use super::fleet_target::FleetTargetBinding;
use super::repository::AgentStoreError;

pub const FLEET_APPROVAL_GRAPH_CHANGED: &str = "FLEET_APPROVAL_GRAPH_CHANGED";
pub const FLEET_APPROVAL_TARGETS_CHANGED: &str = "FLEET_APPROVAL_TARGETS_CHANGED";
pub const FLEET_APPROVAL_POLICY_CHANGED: &str = "FLEET_APPROVAL_POLICY_CHANGED";
pub const FLEET_APPROVAL_NOT_PENDING: &str = "FLEET_APPROVAL_NOT_PENDING";
pub const FLEET_APPROVAL_RECOVERED: &str = "FLEET_APPROVAL_RECOVERED";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FleetApprovalStateV2 {
    Pending,
    Granted,
    Rejected,
    Invalidated,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetApprovalV2 {
    pub id: Uuid,
    pub fleet_run_id: Uuid,
    pub fleet_version: u64,
    pub graph_digest: String,
    pub targets: Vec<FleetTargetBinding>,
    pub policy_version: u64,
    pub policy_hash: String,
    pub state: FleetApprovalStateV2,
    pub invalidation_code: Option<String>,
    pub created_at_epoch_ms: u64,
    pub decided_at_epoch_ms: Option<u64>,
}

impl FleetApprovalV2 {
    pub fn bind(
        run: &FleetRunV2,
        policy_version: u64,
        policy_hash: String,
        now_epoch_ms: u64,
    ) -> Result<Self, &'static str> {
        if run.recovery_state != FleetRecoveryStateV2::Live
            || run.state != FleetRunStateV2::AwaitingApproval
        {
            return Err(FLEET_APPROVAL_RECOVERED);
        }
        Ok(Self {
            id: Uuid::new_v4(),
            fleet_run_id: run.id,
            fleet_version: run.version,
            graph_digest: run.graph_digest.clone(),
            targets: run.targets.clone(),
            policy_version,
            policy_hash,
            state: FleetApprovalStateV2::Pending,
            invalidation_code: None,
            created_at_epoch_ms: now_epoch_ms,
            decided_at_epoch_ms: None,
        })
    }

    pub fn validate(
        &self,
        run: &FleetRunV2,
        policy_version: u64,
        policy_hash: &str,
    ) -> Result<(), &'static str> {
        if self.state != FleetApprovalStateV2::Pending {
            return Err(FLEET_APPROVAL_NOT_PENDING);
        }
        if run.recovery_state != FleetRecoveryStateV2::Live {
            return Err(FLEET_APPROVAL_RECOVERED);
        }
        if run.state != FleetRunStateV2::AwaitingApproval {
            return Err(FLEET_APPROVAL_GRAPH_CHANGED);
        }
        if self.fleet_run_id != run.id
            || self.fleet_version != run.version
            || self.graph_digest != run.graph_digest
        {
            return Err(FLEET_APPROVAL_GRAPH_CHANGED);
        }
        if self.targets != run.targets {
            return Err(FLEET_APPROVAL_TARGETS_CHANGED);
        }
        if self.policy_version != policy_version || self.policy_hash != policy_hash {
            return Err(FLEET_APPROVAL_POLICY_CHANGED);
        }
        Ok(())
    }

    pub fn decide(&mut self, state: FleetApprovalStateV2, now_epoch_ms: u64) {
        self.state = state;
        self.decided_at_epoch_ms = Some(now_epoch_ms);
    }

    pub fn invalidate(&mut self, code: &'static str, now_epoch_ms: u64) {
        self.state = FleetApprovalStateV2::Invalidated;
        self.invalidation_code = Some(code.into());
        self.decided_at_epoch_ms = Some(now_epoch_ms);
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FleetEventKindV2 {
    DraftCreated,
    ApprovalRequired,
    ApprovalGranted,
    ApprovalRejected,
    ApprovalInvalidated,
    ChildClaimed,
    StateChanged,
    RecoveryInterrupted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetEventEnvelopeV2 {
    pub fleet_run_id: Uuid,
    pub seq: u64,
    pub timestamp_ms: u64,
    pub kind: FleetEventKindV2,
    pub state: FleetRunStateV2,
    pub approval_id: Option<Uuid>,
    pub code: Option<String>,
}

pub trait FleetControlStore: Send + Sync {
    fn insert_fleet_approval(
        &self,
        run: &FleetRunV2,
        approval: &FleetApprovalV2,
        event: &FleetEventEnvelopeV2,
    ) -> Result<(), AgentStoreError>;
    fn decide_fleet_approval(
        &self,
        run: &FleetRunV2,
        approval: &FleetApprovalV2,
        event: &FleetEventEnvelopeV2,
    ) -> Result<(), AgentStoreError>;
    fn get_fleet_approval(
        &self,
        approval_id: Uuid,
    ) -> Result<Option<FleetApprovalV2>, AgentStoreError>;
    fn fleet_events_after(
        &self,
        fleet_run_id: Uuid,
        after_seq: u64,
    ) -> Result<Vec<FleetEventEnvelopeV2>, AgentStoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::fleet_run::{
        FleetExecutionStrategyV2, FleetFailurePolicyV2, FleetRunDraft, FleetStageDraft,
    };

    fn awaiting_run() -> FleetRunV2 {
        let targets = (0..2)
            .map(|ordinal| FleetTargetBinding {
                profile_id: Uuid::new_v4(),
                session_id: Uuid::new_v4(),
                role: Some(format!("node-{ordinal}")),
                ordinal,
            })
            .collect::<Vec<_>>();
        let mut run = FleetRunV2::draft(
            FleetRunDraft {
                production: true,
                failure_policy: FleetFailurePolicyV2::PauseForReview,
                stages: vec![FleetStageDraft {
                    id: Uuid::new_v4(),
                    summary: "Inspect".into(),
                    target_ids: vec![targets[0].profile_id],
                    depends_on: vec![],
                    execution_strategy: FleetExecutionStrategyV2::Sequential,
                    concurrency_limit: 1,
                }],
                targets,
            },
            10,
        )
        .expect("draft");
        run.state = FleetRunStateV2::AwaitingApproval;
        run
    }

    #[test]
    fn approval_is_bound_to_graph_targets_and_policy() {
        let run = awaiting_run();
        let approval = FleetApprovalV2::bind(&run, 7, "policy".into(), 20).expect("bind");
        assert_eq!(approval.validate(&run, 7, "policy"), Ok(()));

        let mut changed = run.clone();
        changed.version += 1;
        assert_eq!(
            approval.validate(&changed, 7, "policy"),
            Err(FLEET_APPROVAL_GRAPH_CHANGED)
        );
        let mut changed = run.clone();
        changed.targets[0].session_id = Uuid::new_v4();
        assert_eq!(
            approval.validate(&changed, 7, "policy"),
            Err(FLEET_APPROVAL_TARGETS_CHANGED)
        );
        assert_eq!(
            approval.validate(&run, 8, "policy-2"),
            Err(FLEET_APPROVAL_POLICY_CHANGED)
        );
    }

    #[test]
    fn recovered_metadata_cannot_create_or_use_approval() {
        let mut run = awaiting_run();
        let approval = FleetApprovalV2::bind(&run, 1, "policy".into(), 20).expect("bind");
        run.recovery_state = FleetRecoveryStateV2::MetadataOnly;
        assert_eq!(
            approval.validate(&run, 1, "policy"),
            Err(FLEET_APPROVAL_RECOVERED)
        );
        assert_eq!(
            FleetApprovalV2::bind(&run, 1, "policy".into(), 20),
            Err(FLEET_APPROVAL_RECOVERED)
        );
    }
}
