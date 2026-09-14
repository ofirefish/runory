//! Rust-only Fleet DAG scheduler.
//!
//! The coordinator issues metadata leases for exact target/session pairs. It
//! does not execute commands and is intentionally not exposed as Tauri IPC.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::fleet_control::{FleetApprovalStateV2, FleetApprovalV2};
use super::fleet_run::{
    FleetChildRun, FleetExecutionStrategyV2, FleetFailurePolicyV2, FleetRecoveryStateV2,
    FleetRunStateV2, FleetRunV2, FleetStageStateV2,
};
use super::fleet_target::FleetTargetBinding;

pub const MAX_FLEET_ACTIVE_CHILDREN: usize = 10;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetChildLeaseV2 {
    pub fleet_run_id: Uuid,
    pub fleet_version: u64,
    pub graph_digest: String,
    pub approval_id: Uuid,
    pub stage_id: Uuid,
    pub agent_run_id: Uuid,
    pub target: FleetTargetBinding,
    pub attempt: u32,
    pub policy_version: u64,
    pub policy_hash: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FleetChildOutcomeV2 {
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum FleetCoordinatorError {
    #[error("fleet execution authority is invalid")]
    InvalidAuthority,
    #[error("fleet run cannot be scheduled")]
    InvalidState,
    #[error("fleet child binding is invalid")]
    InvalidChild,
}

pub struct FleetCoordinatorV2;

impl FleetCoordinatorV2 {
    pub fn claim_ready(
        run: &mut FleetRunV2,
        approval: &FleetApprovalV2,
        current_policy_version: u64,
        current_policy_hash: &str,
        requested_capacity: usize,
        now_epoch_ms: u64,
    ) -> Result<Vec<FleetChildLeaseV2>, FleetCoordinatorError> {
        validate_authority(run, approval, current_policy_version, current_policy_hash)?;
        if requested_capacity == 0 {
            return Ok(Vec::new());
        }
        if run.state == FleetRunStateV2::Approved {
            run.transition_to(FleetRunStateV2::Executing, now_epoch_ms)
                .map_err(|_| FleetCoordinatorError::InvalidState)?;
        } else if run.state != FleetRunStateV2::Executing {
            return Err(FleetCoordinatorError::InvalidState);
        }

        refresh_blocked_stages(run);
        let active = run
            .children
            .iter()
            .filter(|child| is_active(child.state))
            .count();
        let mut capacity = requested_capacity
            .min(MAX_FLEET_ACTIVE_CHILDREN)
            .saturating_sub(active);
        let mut leases = Vec::new();
        let target_map = run
            .targets
            .iter()
            .map(|target| (target.profile_id, target.clone()))
            .collect::<HashMap<_, _>>();
        let succeeded_stages = run
            .stages
            .iter()
            .filter(|stage| stage.state == FleetStageStateV2::Succeeded)
            .map(|stage| stage.id)
            .collect::<HashSet<_>>();

        for stage in &mut run.stages {
            if capacity == 0
                || !matches!(
                    stage.state,
                    FleetStageStateV2::Pending | FleetStageStateV2::Running
                )
                || !stage
                    .depends_on
                    .iter()
                    .all(|id| succeeded_stages.contains(id))
            {
                continue;
            }
            let stage_active = run
                .children
                .iter()
                .filter(|child| child.stage_id == stage.id && is_active(child.state))
                .count();
            let strategy_capacity = stage.concurrency_limit.saturating_sub(stage_active);
            let mut available = capacity.min(strategy_capacity);
            if stage.execution_strategy == FleetExecutionStrategyV2::Sequential && stage_active > 0
            {
                available = 0;
            }
            let completed = run
                .children
                .iter()
                .filter(|child| {
                    child.stage_id == stage.id && child.state == FleetStageStateV2::Succeeded
                })
                .map(|child| child.target_id)
                .collect::<HashSet<_>>();
            let canary_ready = stage.execution_strategy != FleetExecutionStrategyV2::Canary
                || completed.contains(&stage.target_ids[0])
                || !run.children.iter().any(|child| child.stage_id == stage.id);

            for target_id in &stage.target_ids {
                if available == 0 {
                    break;
                }
                if run.children.iter().any(|child| {
                    child.stage_id == stage.id
                        && child.target_id == *target_id
                        && is_settled(child.state)
                }) || run.children.iter().any(|child| {
                    child.stage_id == stage.id
                        && child.target_id == *target_id
                        && is_active(child.state)
                }) {
                    continue;
                }
                if stage.execution_strategy == FleetExecutionStrategyV2::Canary
                    && *target_id != stage.target_ids[0]
                    && !canary_ready
                {
                    continue;
                }
                let target = target_map
                    .get(target_id)
                    .cloned()
                    .ok_or(FleetCoordinatorError::InvalidChild)?;
                let attempt = run
                    .children
                    .iter()
                    .filter(|child| child.stage_id == stage.id && child.target_id == *target_id)
                    .map(|child| child.attempt)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1);
                let agent_run_id = Uuid::new_v4();
                run.children.push(FleetChildRun {
                    stage_id: stage.id,
                    target_id: *target_id,
                    agent_run_id: Some(agent_run_id),
                    attempt,
                    state: FleetStageStateV2::Running,
                    error_code: None,
                });
                leases.push(FleetChildLeaseV2 {
                    fleet_run_id: run.id,
                    fleet_version: run.version,
                    graph_digest: run.graph_digest.clone(),
                    approval_id: approval.id,
                    stage_id: stage.id,
                    agent_run_id,
                    target,
                    attempt,
                    policy_version: current_policy_version,
                    policy_hash: current_policy_hash.into(),
                });
                stage.state = FleetStageStateV2::Running;
                available -= 1;
                capacity -= 1;
                if stage.execution_strategy == FleetExecutionStrategyV2::Canary
                    && !completed.contains(&stage.target_ids[0])
                {
                    break;
                }
            }
        }
        run.updated_at_epoch_ms = now_epoch_ms;
        Ok(leases)
    }

    pub fn record_outcome(
        run: &mut FleetRunV2,
        agent_run_id: Uuid,
        outcome: FleetChildOutcomeV2,
        error_code: Option<String>,
        now_epoch_ms: u64,
    ) -> Result<(), FleetCoordinatorError> {
        if run.recovery_state != FleetRecoveryStateV2::Live
            || run.state != FleetRunStateV2::Executing
        {
            return Err(FleetCoordinatorError::InvalidState);
        }
        if error_code.as_ref().is_some_and(|code| {
            code.is_empty()
                || code.len() > 96
                || !code
                    .bytes()
                    .all(|value| value.is_ascii_uppercase() || value == b'_')
        }) || (outcome == FleetChildOutcomeV2::Succeeded && error_code.is_some())
        {
            return Err(FleetCoordinatorError::InvalidChild);
        }
        let child = run
            .children
            .iter_mut()
            .find(|child| child.agent_run_id == Some(agent_run_id) && is_active(child.state))
            .ok_or(FleetCoordinatorError::InvalidChild)?;
        child.state = match outcome {
            FleetChildOutcomeV2::Succeeded => FleetStageStateV2::Succeeded,
            FleetChildOutcomeV2::Failed => FleetStageStateV2::Failed,
            FleetChildOutcomeV2::Cancelled => FleetStageStateV2::Cancelled,
        };
        child.error_code = error_code;
        let stage_id = child.stage_id;
        refresh_stage(run, stage_id);
        apply_failure_policy(run, now_epoch_ms)?;
        if run.state == FleetRunStateV2::PausedForReview {
            for sibling in &mut run.children {
                if is_active(sibling.state) {
                    sibling.state = FleetStageStateV2::Blocked;
                }
            }
        }
        if run.state == FleetRunStateV2::Executing
            && run
                .stages
                .iter()
                .all(|stage| stage.state == FleetStageStateV2::Succeeded)
        {
            run.transition_to(FleetRunStateV2::Verifying, now_epoch_ms)
                .map_err(|_| FleetCoordinatorError::InvalidState)?;
        }
        run.updated_at_epoch_ms = now_epoch_ms;
        Ok(())
    }

    pub fn pause(
        run: &mut FleetRunV2,
        now_epoch_ms: u64,
    ) -> Result<Vec<Uuid>, FleetCoordinatorError> {
        if run.recovery_state != FleetRecoveryStateV2::Live
            || run.state != FleetRunStateV2::Executing
        {
            return Err(FleetCoordinatorError::InvalidState);
        }
        let active = active_agent_ids(run);
        run.transition_to(FleetRunStateV2::PausedForReview, now_epoch_ms)
            .map_err(|_| FleetCoordinatorError::InvalidState)?;
        for child in &mut run.children {
            if is_active(child.state) {
                child.state = FleetStageStateV2::Blocked;
            }
        }
        Ok(active)
    }

    /// Continues the same approved Fleet graph after explicit review. Failed
    /// or cancelled targets remain settled and are never retried implicitly;
    /// explicitly paused children are returned so their old Runtime V2 runs
    /// can be cancelled before Rust creates fresh attempts.
    pub fn continue_after_review(
        run: &mut FleetRunV2,
        approval: &FleetApprovalV2,
        current_policy_version: u64,
        current_policy_hash: &str,
        now_epoch_ms: u64,
    ) -> Result<Vec<Uuid>, FleetCoordinatorError> {
        validate_authority_for_states(
            run,
            approval,
            current_policy_version,
            current_policy_hash,
            &[FleetRunStateV2::PausedForReview],
        )?;
        let paused = run
            .children
            .iter()
            .filter(|child| child.state == FleetStageStateV2::Blocked)
            .filter_map(|child| child.agent_run_id)
            .collect::<Vec<_>>();
        let children = &run.children;
        for stage in &mut run.stages {
            if matches!(
                stage.state,
                FleetStageStateV2::Failed | FleetStageStateV2::Blocked
            ) && stage.target_ids.iter().any(|target_id| {
                !children.iter().any(|child| {
                    child.stage_id == stage.id
                        && child.target_id == *target_id
                        && is_settled(child.state)
                })
            }) {
                stage.state = FleetStageStateV2::Running;
            }
        }
        run.transition_to(FleetRunStateV2::Executing, now_epoch_ms)
            .map_err(|_| FleetCoordinatorError::InvalidState)?;
        Ok(paused)
    }

    pub fn cancel(
        run: &mut FleetRunV2,
        now_epoch_ms: u64,
    ) -> Result<Vec<Uuid>, FleetCoordinatorError> {
        if run.recovery_state != FleetRecoveryStateV2::Live || run.state.is_terminal() {
            return Err(FleetCoordinatorError::InvalidState);
        }
        let active = active_agent_ids(run);
        for child in &mut run.children {
            if is_active(child.state) {
                child.state = FleetStageStateV2::Cancelled;
            }
        }
        run.transition_to(FleetRunStateV2::Cancelled, now_epoch_ms)
            .map_err(|_| FleetCoordinatorError::InvalidState)?;
        Ok(active)
    }
}

fn validate_authority(
    run: &FleetRunV2,
    approval: &FleetApprovalV2,
    policy_version: u64,
    policy_hash: &str,
) -> Result<(), FleetCoordinatorError> {
    validate_authority_for_states(
        run,
        approval,
        policy_version,
        policy_hash,
        &[FleetRunStateV2::Approved, FleetRunStateV2::Executing],
    )
}

fn validate_authority_for_states(
    run: &FleetRunV2,
    approval: &FleetApprovalV2,
    policy_version: u64,
    policy_hash: &str,
    allowed_states: &[FleetRunStateV2],
) -> Result<(), FleetCoordinatorError> {
    if run.recovery_state != FleetRecoveryStateV2::Live
        || !allowed_states.contains(&run.state)
        || approval.state != FleetApprovalStateV2::Granted
        || approval.fleet_run_id != run.id
        || approval.fleet_version != run.version
        || approval.graph_digest != run.graph_digest
        || approval.targets != run.targets
        || approval.policy_version != policy_version
        || approval.policy_hash != policy_hash
    {
        return Err(FleetCoordinatorError::InvalidAuthority);
    }
    Ok(())
}

fn is_active(state: FleetStageStateV2) -> bool {
    matches!(
        state,
        FleetStageStateV2::Running
            | FleetStageStateV2::AwaitingApproval
            | FleetStageStateV2::Verifying
    )
}

fn is_settled(state: FleetStageStateV2) -> bool {
    matches!(
        state,
        FleetStageStateV2::Succeeded
            | FleetStageStateV2::Failed
            | FleetStageStateV2::Cancelled
            | FleetStageStateV2::RolledBack
            | FleetStageStateV2::RollbackFailed
    )
}

fn active_agent_ids(run: &FleetRunV2) -> Vec<Uuid> {
    run.children
        .iter()
        .filter(|child| is_active(child.state))
        .filter_map(|child| child.agent_run_id)
        .collect()
}

fn refresh_blocked_stages(run: &mut FleetRunV2) {
    let failed = run
        .stages
        .iter()
        .filter(|stage| {
            matches!(
                stage.state,
                FleetStageStateV2::Failed | FleetStageStateV2::Blocked
            )
        })
        .map(|stage| stage.id)
        .collect::<HashSet<_>>();
    for stage in &mut run.stages {
        if stage.state == FleetStageStateV2::Pending
            && stage.depends_on.iter().any(|id| failed.contains(id))
        {
            stage.state = FleetStageStateV2::Blocked;
        }
    }
}

fn refresh_stage(run: &mut FleetRunV2, stage_id: Uuid) {
    let Some(stage) = run.stages.iter_mut().find(|stage| stage.id == stage_id) else {
        return;
    };
    let children = run
        .children
        .iter()
        .filter(|child| child.stage_id == stage_id)
        .collect::<Vec<_>>();
    if children
        .iter()
        .any(|child| child.state == FleetStageStateV2::Failed)
    {
        stage.state = FleetStageStateV2::Failed;
    } else if stage.target_ids.iter().all(|target_id| {
        children.iter().any(|child| {
            child.target_id == *target_id && child.state == FleetStageStateV2::Succeeded
        })
    }) {
        stage.state = FleetStageStateV2::Succeeded;
    } else if children.iter().any(|child| is_active(child.state)) {
        stage.state = FleetStageStateV2::Running;
    }
}

fn apply_failure_policy(
    run: &mut FleetRunV2,
    now_epoch_ms: u64,
) -> Result<(), FleetCoordinatorError> {
    if !run
        .stages
        .iter()
        .any(|stage| stage.state == FleetStageStateV2::Failed)
    {
        return Ok(());
    }
    let next = match run.failure_policy {
        FleetFailurePolicyV2::Stop => Some(FleetRunStateV2::Failed),
        FleetFailurePolicyV2::PauseForReview => Some(FleetRunStateV2::PausedForReview),
        FleetFailurePolicyV2::Rollback => Some(FleetRunStateV2::RollingBack),
        FleetFailurePolicyV2::Continue => None,
    };
    if let Some(next) = next {
        run.transition_to(next, now_epoch_ms)
            .map_err(|_| FleetCoordinatorError::InvalidState)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::fleet_control::FleetApprovalV2;
    use crate::agent::fleet_run::{FleetRunDraft, FleetStageDraft};

    fn target(ordinal: usize) -> FleetTargetBinding {
        FleetTargetBinding {
            profile_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            role: Some(format!("node-{ordinal}")),
            ordinal,
        }
    }

    fn approved_run(strategy: FleetExecutionStrategyV2) -> (FleetRunV2, FleetApprovalV2) {
        let targets = vec![target(0), target(1), target(2)];
        let mut run = FleetRunV2::draft(
            FleetRunDraft {
                production: strategy != FleetExecutionStrategyV2::Parallel,
                failure_policy: FleetFailurePolicyV2::PauseForReview,
                stages: vec![FleetStageDraft {
                    id: Uuid::new_v4(),
                    summary: "Configure nodes".into(),
                    target_ids: targets.iter().map(|target| target.profile_id).collect(),
                    depends_on: vec![],
                    execution_strategy: strategy,
                    concurrency_limit: 2,
                }],
                targets,
            },
            1,
        )
        .expect("draft");
        run.state = FleetRunStateV2::AwaitingApproval;
        let mut approval = FleetApprovalV2::bind(&run, 4, "policy".into(), 2).expect("approval");
        approval.decide(FleetApprovalStateV2::Granted, 3);
        run.state = FleetRunStateV2::Approved;
        (run, approval)
    }

    #[test]
    fn canary_claims_one_then_releases_bounded_batch() {
        let (mut run, approval) = approved_run(FleetExecutionStrategyV2::Canary);
        let first = FleetCoordinatorV2::claim_ready(&mut run, &approval, 4, "policy", 10, 4)
            .expect("claim canary");
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].target, run.targets[0]);
        FleetCoordinatorV2::record_outcome(
            &mut run,
            first[0].agent_run_id,
            FleetChildOutcomeV2::Succeeded,
            None,
            5,
        )
        .expect("canary succeeds");
        let next = FleetCoordinatorV2::claim_ready(&mut run, &approval, 4, "policy", 10, 6)
            .expect("claim batch");
        assert_eq!(next.len(), 2);
    }

    #[test]
    fn policy_or_target_drift_blocks_all_claims() {
        let (mut run, approval) = approved_run(FleetExecutionStrategyV2::RollingBatch);
        assert_eq!(
            FleetCoordinatorV2::claim_ready(&mut run, &approval, 5, "changed", 2, 4),
            Err(FleetCoordinatorError::InvalidAuthority)
        );
        run.targets[0].session_id = Uuid::new_v4();
        assert_eq!(
            FleetCoordinatorV2::claim_ready(&mut run, &approval, 4, "policy", 2, 4),
            Err(FleetCoordinatorError::InvalidAuthority)
        );
        assert!(run.children.is_empty());
    }

    #[test]
    fn pause_and_cancel_return_only_owned_child_runs() {
        let (mut first_run, approval) = approved_run(FleetExecutionStrategyV2::RollingBatch);
        let first = FleetCoordinatorV2::claim_ready(&mut first_run, &approval, 4, "policy", 2, 4)
            .expect("claim first fleet");
        let (mut other_run, other_approval) = approved_run(FleetExecutionStrategyV2::RollingBatch);
        let other =
            FleetCoordinatorV2::claim_ready(&mut other_run, &other_approval, 4, "policy", 2, 4)
                .expect("claim other fleet");

        let paused = FleetCoordinatorV2::pause(&mut first_run, 5).expect("pause");
        assert_eq!(paused.len(), first.len());
        assert!(first_run
            .children
            .iter()
            .all(|child| child.state == FleetStageStateV2::Blocked));
        assert!(paused
            .iter()
            .all(|id| !other.iter().any(|lease| lease.agent_run_id == *id)));
        let continued =
            FleetCoordinatorV2::continue_after_review(&mut first_run, &approval, 4, "policy", 6)
                .expect("continue exact approved fleet");
        assert_eq!(continued, paused);
        let fresh = FleetCoordinatorV2::claim_ready(&mut first_run, &approval, 4, "policy", 2, 7)
            .expect("claim fresh attempts");
        assert_eq!(fresh.len(), first.len());
        assert!(fresh.iter().all(|lease| lease.attempt == 2));
        let cancelled = FleetCoordinatorV2::cancel(&mut other_run, 6).expect("cancel");
        assert_eq!(cancelled.len(), other.len());
        assert_eq!(other_run.state, FleetRunStateV2::Cancelled);
    }

    #[test]
    fn continue_never_retries_a_failed_target_implicitly() {
        let (mut run, approval) = approved_run(FleetExecutionStrategyV2::RollingBatch);
        let first = FleetCoordinatorV2::claim_ready(&mut run, &approval, 4, "policy", 2, 4)
            .expect("claim first batch");
        let failed_target = first[0].target.profile_id;
        FleetCoordinatorV2::record_outcome(
            &mut run,
            first[0].agent_run_id,
            FleetChildOutcomeV2::Failed,
            Some("COMMAND_FAILED".into()),
            5,
        )
        .expect("pause after failure");
        assert_eq!(run.state, FleetRunStateV2::PausedForReview);
        assert_eq!(
            run.children
                .iter()
                .find(|child| child.agent_run_id == Some(first[1].agent_run_id))
                .map(|child| child.state),
            Some(FleetStageStateV2::Blocked)
        );

        FleetCoordinatorV2::continue_after_review(&mut run, &approval, 4, "policy", 6)
            .expect("continue independent targets");
        let next = FleetCoordinatorV2::claim_ready(&mut run, &approval, 4, "policy", 3, 7)
            .expect("claim remaining targets");
        assert!(!next
            .iter()
            .any(|lease| lease.target.profile_id == failed_target));
    }
}
