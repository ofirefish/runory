use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::task::Poll;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

pub(super) fn validate_request(request: &MultiChangeSetDraftRequest) -> AppResult<()> {
    if request.targets.is_empty()
        || request.targets.len() > MAX_TARGETS
        || request.title.trim().is_empty()
        || request.title.len() > 256
        || request.batch_size == 0
        || request.batch_size > MAX_TARGETS
        || (request.execution_strategy == ExecutionStrategy::Canary
            && (request.canary_count == 0 || request.canary_count >= request.targets.len()))
        || (request.production && request.execution_strategy == ExecutionStrategy::Parallel)
        || request
            .service_verification
            .as_deref()
            .is_some_and(|service| !valid_service(service))
    {
        return Err(AppError::InvalidOperation);
    }
    Ok(())
}

fn valid_service(service: &str) -> bool {
    !service.is_empty()
        && service.len() <= 128
        && service.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'@' | b':' | b'-')
        })
}

pub(super) fn validate_live_version(run: &MultiChangeSet, version: u64) -> AppResult<()> {
    if run.version == version && run.recovery_state == FleetRecoveryState::Live {
        Ok(())
    } else {
        Err(AppError::InvalidOperation)
    }
}

pub(super) fn exact_targets_match(targets: &[FleetTargetExecution], target_ids: &[Uuid]) -> bool {
    let expected = targets
        .iter()
        .map(|item| item.target_id)
        .collect::<BTreeSet<_>>();
    let actual = target_ids.iter().copied().collect::<BTreeSet<_>>();
    targets.len() == target_ids.len()
        && expected.len() == targets.len()
        && actual.len() == target_ids.len()
        && expected == actual
}

pub(super) fn verification_outcome(
    targets: &[FleetTargetExecution],
    service_verification_required: bool,
) -> (bool, bool) {
    let local = targets
        .iter()
        .all(|target| target.local_verification == VerificationState::Succeeded);
    let service = !service_verification_required
        || targets
            .iter()
            .all(|target| target.service_verification == VerificationState::Succeeded);
    (local, service)
}

pub(super) fn execution_batches(run: &MultiChangeSet) -> AppResult<Vec<Vec<usize>>> {
    let indexes = run
        .targets
        .iter()
        .enumerate()
        .filter_map(|(index, target)| (target.state == FleetTargetState::Pending).then_some(index))
        .collect::<Vec<_>>();
    let count = indexes.len();
    if count == 0 || (run.production && run.execution_strategy == ExecutionStrategy::Parallel) {
        return Err(AppError::InvalidOperation);
    }
    Ok(match run.execution_strategy {
        ExecutionStrategy::Sequential => indexes.into_iter().map(|index| vec![index]).collect(),
        ExecutionStrategy::Parallel => vec![indexes],
        ExecutionStrategy::Canary => {
            let canary = run.canary_count.min(count.saturating_sub(1));
            let mut batches = vec![indexes[..canary].to_vec()];
            batches.extend(indexes[canary..].iter().map(|index| vec![*index]));
            batches
        }
        ExecutionStrategy::RollingBatch => indexes
            .chunks(run.batch_size)
            .map(<[usize]>::to_vec)
            .collect(),
    })
}

pub(super) async fn join_all<'a, T>(
    futures: Vec<Pin<Box<dyn Future<Output = T> + Send + 'a>>>,
) -> Vec<T> {
    let count = futures.len();
    let mut futures = futures.into_iter().map(Some).collect::<Vec<_>>();
    let mut outputs = (0..count).map(|_| None).collect::<Vec<Option<T>>>();
    std::future::poll_fn(move |context| {
        let mut pending = 0;
        for index in 0..count {
            if outputs[index].is_some() {
                continue;
            }
            let Some(future) = futures[index].as_mut() else {
                continue;
            };
            match future.as_mut().poll(context) {
                Poll::Ready(output) => {
                    outputs[index] = Some(output);
                    futures[index] = None;
                }
                Poll::Pending => pending += 1,
            }
        }
        if pending == 0 {
            Poll::Ready(outputs.iter_mut().filter_map(Option::take).collect())
        } else {
            Poll::Pending
        }
    })
    .await
}

pub(super) fn fleet_target(change_set: &ChangeSet) -> FleetTargetExecution {
    FleetTargetExecution {
        target_id: change_set.session_id,
        change_set_id: change_set.id,
        change_set_version: change_set.version,
        state: FleetTargetState::Pending,
        local_verification: VerificationState::Pending,
        service_verification: VerificationState::Pending,
        rollback_state: VerificationState::NotApplicable,
        error_code: None,
        duration_ms: None,
    }
}

pub(super) fn persisted_run(run: &MultiChangeSet) -> PersistedFleetRun {
    PersistedFleetRun {
        schema_version: FLEET_SCHEMA_VERSION,
        id: run.id,
        agent_run_id: run.agent_run_id,
        version: run.version,
        risk: run.risk,
        execution_strategy: run.execution_strategy,
        failure_policy: run.failure_policy,
        batch_size: run.batch_size,
        canary_count: run.canary_count,
        production: run.production,
        service_verification: run.service_verification.clone(),
        cross_target_verification: run.cross_target_verification,
        orchestration_binding: run.orchestration_binding.clone(),
        targets: run.targets.clone(),
        last_approval: run.last_approval.clone(),
        execution_state: run.execution_state,
        verification: run.verification.clone(),
        tool_call_count: run.tool_call_count,
        started_at_epoch_ms: run.started_at_epoch_ms,
        completed_at_epoch_ms: run.completed_at_epoch_ms,
        duration_ms: run.duration_ms,
        audit: run.audit.clone(),
        policy_snapshot: run.policy_snapshot.clone(),
    }
}

pub(super) fn recovered_run(record: PersistedFleetRun) -> MultiChangeSet {
    let execution_state = if matches!(
        record.execution_state,
        FleetExecutionState::Executing
            | FleetExecutionState::Verifying
            | FleetExecutionState::RollingBack
    ) {
        FleetExecutionState::Interrupted
    } else {
        record.execution_state
    };
    let last_approval = record
        .last_approval
        .filter(|approval| approval.targets.len() <= MAX_TARGETS);
    MultiChangeSet {
        id: record.id,
        agent_run_id: record.agent_run_id,
        model: MODEL_CODE,
        title: "recovered-fleet-run".into(),
        version: record.version,
        risk: record.risk,
        execution_strategy: record.execution_strategy,
        failure_policy: record.failure_policy,
        batch_size: record.batch_size,
        canary_count: record.canary_count,
        production: record.production,
        service_verification: record.service_verification,
        cross_target_verification: record.cross_target_verification,
        orchestration_binding: record.orchestration_binding,
        targets: record.targets,
        approval_state: ApprovalState::Invalidated,
        approval: None,
        last_approval,
        execution_state,
        verification: record.verification,
        recovery_state: FleetRecoveryState::MetadataOnly,
        tool_call_count: record.tool_call_count,
        started_at_epoch_ms: record.started_at_epoch_ms,
        completed_at_epoch_ms: record.completed_at_epoch_ms,
        duration_ms: record.duration_ms,
        audit: {
            let mut audit = record
                .audit
                .into_iter()
                .filter(|event| valid_stable_code(&event.code))
                .rev()
                .take(MAX_FLEET_AUDIT_EVENTS.saturating_sub(1))
                .collect::<Vec<_>>();
            audit.reverse();
            audit.push(FleetAuditEvent {
                state: execution_state,
                target_id: None,
                code: "FLEET_RECOVERED_METADATA_ONLY".into(),
                occurred_at_epoch_ms: now_epoch_ms(),
            });
            audit
        },
        policy_evaluation: None,
        policy_snapshot: record.policy_snapshot,
    }
}

fn valid_stable_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

pub(super) fn push_event(run: &mut MultiChangeSet, target_id: Option<Uuid>, code: &'static str) {
    run.audit.push(FleetAuditEvent {
        state: run.execution_state,
        target_id,
        code: code.into(),
        occurred_at_epoch_ms: now_epoch_ms(),
    });
    if run.audit.len() > MAX_FLEET_AUDIT_EVENTS {
        run.audit.drain(..run.audit.len() - MAX_FLEET_AUDIT_EVENTS);
    }
}

pub(super) fn finish(run: &mut MultiChangeSet) {
    let completed = now_epoch_ms();
    run.completed_at_epoch_ms = Some(completed);
    run.duration_ms = run
        .started_at_epoch_ms
        .map(|started| completed.saturating_sub(started));
}

pub(super) fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
