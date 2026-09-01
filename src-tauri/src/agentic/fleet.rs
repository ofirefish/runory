use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::{ApprovalState, ChangeSet, ChangeSetService, ExecutionState};
use crate::domain::{AppError, AppResult, ServiceStatus};
use crate::policy::AgentPolicyService;
use crate::policy::{PolicyEvaluation, PolicySnapshot};
use crate::ssh::ServerSessionManager;
use crate::storage::JsonRepository;
use crate::tools::{
    NativeToolExecutionService, NativeToolInvocation, NativeToolRequest, RiskLevel, ToolData,
    ToolExecutionAuthority,
};

mod model;
pub(crate) use model::*;
mod state;
use state::*;

const MAX_TARGETS: usize = 10;
const MAX_FLEET_RUNS: usize = 2_000;
const MAX_FLEET_AUDIT_EVENTS: usize = 512;
const FLEET_SCHEMA_VERSION: u8 = 1;
const MODEL_CODE: &str = "runory-local-doctor-v1";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedFleetRun {
    schema_version: u8,
    id: Uuid,
    agent_run_id: Uuid,
    version: u64,
    risk: RiskLevel,
    execution_strategy: ExecutionStrategy,
    failure_policy: FailurePolicy,
    batch_size: usize,
    canary_count: usize,
    production: bool,
    service_verification: Option<String>,
    cross_target_verification: bool,
    targets: Vec<FleetTargetExecution>,
    #[serde(default)]
    last_approval: Option<FleetApprovalBinding>,
    execution_state: FleetExecutionState,
    verification: FleetVerification,
    tool_call_count: u32,
    started_at_epoch_ms: Option<u64>,
    completed_at_epoch_ms: Option<u64>,
    duration_ms: Option<u64>,
    #[serde(default)]
    audit: Vec<FleetAuditEvent>,
    #[serde(default)]
    policy_snapshot: Option<PolicySnapshot>,
}

#[derive(Clone, Default)]
pub(crate) struct FleetExecutionService {
    items: Arc<Mutex<HashMap<Uuid, MultiChangeSet>>>,
    repository: Option<JsonRepository<Vec<PersistedFleetRun>>>,
}

impl FleetExecutionService {
    pub(crate) fn at_path(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            items: Arc::new(Mutex::new(HashMap::new())),
            repository: Some(JsonRepository::new(path)),
        }
    }

    pub(crate) async fn load(&self) -> AppResult<()> {
        let Some(repository) = &self.repository else {
            return Ok(());
        };
        let records = repository.load_or_default().await?;
        let mut items = self.items.lock().await;
        items.clear();
        for record in records
            .into_iter()
            .filter(|item| {
                item.schema_version == FLEET_SCHEMA_VERSION
                    && !item.targets.is_empty()
                    && item.targets.len() <= MAX_TARGETS
            })
            .take(MAX_FLEET_RUNS)
        {
            let run = recovered_run(record);
            items.insert(run.id, run);
        }
        self.persist_locked(&items).await
    }

    pub(crate) async fn draft(
        &self,
        changes: &ChangeSetService,
        request: MultiChangeSetDraftRequest,
    ) -> AppResult<MultiChangeSet> {
        validate_request(&request)?;
        let mut sessions = BTreeSet::new();
        if request
            .targets
            .iter()
            .any(|target| !sessions.insert(target.session_id))
        {
            return Err(AppError::InvalidOperation);
        }
        let mut change_sets = Vec::with_capacity(request.targets.len());
        for target in request.targets {
            match changes.draft(target).await {
                Ok(change_set) => change_sets.push(change_set),
                Err(error) => return Err(error),
            }
        }
        let risk = change_sets
            .iter()
            .map(|item| item.risk)
            .max()
            .unwrap_or(RiskLevel::R2);
        let now = now_epoch_ms();
        let run = MultiChangeSet {
            id: Uuid::new_v4(),
            agent_run_id: request.agent_run_id,
            model: MODEL_CODE,
            title: request.title,
            version: 1,
            risk,
            execution_strategy: request.execution_strategy,
            failure_policy: request.failure_policy,
            batch_size: request.batch_size,
            canary_count: request.canary_count,
            production: request.production,
            service_verification: request.service_verification,
            cross_target_verification: request.cross_target_verification,
            targets: change_sets.iter().map(fleet_target).collect(),
            approval_state: ApprovalState::Draft,
            approval: None,
            last_approval: None,
            execution_state: FleetExecutionState::Draft,
            verification: FleetVerification {
                cross_target: VerificationState::Pending,
                service_level: VerificationState::Pending,
            },
            recovery_state: FleetRecoveryState::Live,
            tool_call_count: 0,
            started_at_epoch_ms: None,
            completed_at_epoch_ms: None,
            duration_ms: None,
            audit: vec![FleetAuditEvent {
                state: FleetExecutionState::Draft,
                target_id: None,
                code: "FLEET_DRAFTED".into(),
                occurred_at_epoch_ms: now,
            }],
            policy_evaluation: None,
            policy_snapshot: None,
        };
        let mut items = self.items.lock().await;
        items.insert(run.id, run.clone());
        if let Err(error) = self.persist_locked(&items).await {
            items.remove(&run.id);
            return Err(error);
        }
        Ok(run)
    }

    pub(crate) async fn bind_policy(
        &self,
        id: Uuid,
        evaluation: PolicyEvaluation,
    ) -> AppResult<MultiChangeSet> {
        let mut items = self.items.lock().await;
        let run = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        run.policy_snapshot = Some(PolicySnapshot::from(&evaluation));
        run.policy_evaluation = Some(evaluation);
        let output = run.clone();
        self.persist_locked(&items).await?;
        Ok(output)
    }

    pub(crate) async fn get(&self, id: Uuid) -> AppResult<MultiChangeSet> {
        self.items
            .lock()
            .await
            .get(&id)
            .cloned()
            .ok_or(AppError::InvalidOperation)
    }

    pub(crate) async fn list(&self) -> AppResult<Vec<MultiChangeSet>> {
        let mut runs = self
            .items
            .lock()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        runs.sort_by_key(|item| item.id);
        Ok(runs)
    }

    pub(crate) async fn approve(
        &self,
        changes: &ChangeSetService,
        id: Uuid,
        version: u64,
        exact_target_ids: Vec<Uuid>,
    ) -> AppResult<MultiChangeSet> {
        let snapshot = self.get(id).await?;
        validate_live_version(&snapshot, version)?;
        if snapshot.execution_state != FleetExecutionState::Draft
            || !exact_targets_match(&snapshot.targets, &exact_target_ids)
        {
            return Err(AppError::InvalidOperation);
        }
        let mut bindings = Vec::with_capacity(snapshot.targets.len());
        for target in &snapshot.targets {
            let current = changes.get(target.change_set_id).await?;
            if current.session_id != target.target_id
                || current.version != target.change_set_version
                || current.execution_state != ExecutionState::NotStarted
            {
                self.invalidate(id, "FLEET_APPROVAL_TARGET_CHANGED").await?;
                return Err(AppError::InvalidOperation);
            }
            bindings.push(TargetApprovalBinding {
                target_id: current.session_id,
                change_set_id: current.id,
                change_set_version: current.version,
            });
        }
        for binding in &bindings {
            changes
                .approve(binding.change_set_id, binding.change_set_version)
                .await?;
        }
        let mut items = self.items.lock().await;
        let run = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        validate_live_version(run, version)?;
        run.approval_state = ApprovalState::Approved;
        run.execution_state = FleetExecutionState::Approved;
        let approval = FleetApprovalBinding {
            fleet_version: version,
            target_ids: run.targets.iter().map(|item| item.target_id).collect(),
            targets: bindings,
            approved_at_epoch_ms: now_epoch_ms(),
        };
        run.approval = Some(approval.clone());
        run.last_approval = Some(approval);
        push_event(run, None, "FLEET_APPROVED");
        let output = run.clone();
        if let Err(error) = self.persist_locked(&items).await {
            drop(items);
            for binding in output
                .approval
                .as_ref()
                .into_iter()
                .flat_map(|item| &item.targets)
            {
                let _ = changes
                    .reject(binding.change_set_id, binding.change_set_version)
                    .await;
            }
            self.items.lock().await.insert(id, snapshot);
            return Err(error);
        }
        Ok(output)
    }

    pub(crate) async fn invalidate_target(
        &self,
        change_set_id: Uuid,
        target_id: Uuid,
        change_set_version: u64,
    ) -> AppResult<()> {
        let mut items = self.items.lock().await;
        let mut changed = false;
        for run in items.values_mut().filter(|run| {
            run.targets
                .iter()
                .any(|target| target.change_set_id == change_set_id)
                && matches!(
                    run.execution_state,
                    FleetExecutionState::Draft | FleetExecutionState::Approved
                )
        }) {
            if let Some(target) = run
                .targets
                .iter_mut()
                .find(|target| target.change_set_id == change_set_id)
            {
                target.target_id = target_id;
                target.change_set_version = change_set_version;
                target.state = FleetTargetState::Pending;
                target.local_verification = VerificationState::Pending;
                target.service_verification = VerificationState::Pending;
                target.rollback_state = VerificationState::NotApplicable;
                target.error_code = None;
            }
            run.approval_state = ApprovalState::Invalidated;
            run.approval = None;
            run.execution_state = FleetExecutionState::Draft;
            run.version = run.version.saturating_add(1);
            push_event(run, None, "FLEET_APPROVAL_INVALIDATED");
            changed = true;
        }
        if changed {
            self.persist_locked(&items).await?;
        }
        Ok(())
    }

    async fn invalidate(&self, id: Uuid, code: &'static str) -> AppResult<()> {
        let mut items = self.items.lock().await;
        let run = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        run.approval_state = ApprovalState::Invalidated;
        run.approval = None;
        run.execution_state = FleetExecutionState::Draft;
        push_event(run, None, code);
        self.persist_locked(&items).await
    }

    pub(crate) async fn invalidate_policy_change(&self, id: Uuid) -> AppResult<()> {
        self.invalidate(id, "FLEET_POLICY_SNAPSHOT_CHANGED").await
    }

    pub(crate) async fn execute(
        &self,
        changes: &ChangeSetService,
        sessions: &ServerSessionManager,
        tools: &NativeToolExecutionService,
        policies: &AgentPolicyService,
        id: Uuid,
        version: u64,
    ) -> AppResult<MultiChangeSet> {
        let snapshot = self.claim_execution(changes, id, version).await?;
        let batches = execution_batches(&snapshot)?;
        let mut should_stop = false;
        for batch in batches {
            if should_stop {
                break;
            }
            self.mark_targets(id, &batch, FleetTargetState::Executing, "TARGET_EXECUTING")
                .await?;
            let futures = batch
                .iter()
                .map(|index| {
                    let target = &snapshot.targets[*index];
                    Box::pin(async move {
                        let started = Instant::now();
                        let result = changes
                            .execute_with_policy(
                                target.change_set_id,
                                target.change_set_version,
                                sessions,
                                tools,
                                policies,
                            )
                            .await;
                        (
                            result,
                            started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                        )
                    })
                        as Pin<Box<dyn Future<Output = (AppResult<ChangeSet>, u64)> + Send + '_>>
                })
                .collect();
            let results = join_all(futures).await;
            let mut batch_failed = false;
            for (index, (result, duration_ms)) in batch.into_iter().zip(results) {
                let target = &snapshot.targets[index];
                let failed = self
                    .record_execution_result(id, target.target_id, result, duration_ms)
                    .await?;
                batch_failed |= failed;
            }
            if batch_failed {
                should_stop = self
                    .apply_failure_policy(id, changes, sessions, tools)
                    .await?;
            }
        }

        let current = self.get(id).await?;
        if !matches!(
            current.execution_state,
            FleetExecutionState::PausedForReview
                | FleetExecutionState::Failed
                | FleetExecutionState::RolledBack
                | FleetExecutionState::RollbackFailed
        ) {
            self.verify_fleet(id, changes, sessions, tools).await?;
        }
        let audited_tool_calls = tools.audit().await.ok().map(|records| {
            records
                .iter()
                .filter(|record| record.agent_run_id == Some(snapshot.agent_run_id))
                .count()
                .min(u32::MAX as usize) as u32
        });
        let mut items = self.items.lock().await;
        let run = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        if let Some(tool_calls) = audited_tool_calls {
            run.tool_call_count = tool_calls;
        }
        if run.completed_at_epoch_ms.is_none() {
            finish(run);
        }
        let output = run.clone();
        self.persist_locked(&items).await?;
        Ok(output)
    }

    async fn claim_execution(
        &self,
        changes: &ChangeSetService,
        id: Uuid,
        version: u64,
    ) -> AppResult<MultiChangeSet> {
        let snapshot = self.get(id).await?;
        validate_live_version(&snapshot, version)?;
        let approval = snapshot
            .approval
            .as_ref()
            .ok_or(AppError::InvalidOperation)?;
        if !matches!(
            snapshot.execution_state,
            FleetExecutionState::Approved | FleetExecutionState::PausedForReview
        ) || snapshot.approval_state != ApprovalState::Approved
            || approval.fleet_version != version
            || !exact_targets_match(&snapshot.targets, &approval.target_ids)
        {
            return Err(AppError::InvalidOperation);
        }
        for binding in &approval.targets {
            let current = changes.get(binding.change_set_id).await?;
            if current.session_id != binding.target_id
                || current.version != binding.change_set_version
                || current.approved_version != Some(binding.change_set_version)
                || current.approval_state != ApprovalState::Approved
            {
                self.invalidate(id, "FLEET_APPROVAL_BINDING_CHANGED")
                    .await?;
                return Err(AppError::InvalidOperation);
            }
        }
        let mut items = self.items.lock().await;
        let run = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        run.execution_state = FleetExecutionState::Executing;
        run.started_at_epoch_ms.get_or_insert_with(now_epoch_ms);
        run.completed_at_epoch_ms = None;
        run.duration_ms = None;
        push_event(run, None, "FLEET_EXECUTION_CLAIMED");
        let output = run.clone();
        // The execution claim is durable before any remote side effect.
        self.persist_locked(&items).await?;
        Ok(output)
    }

    async fn mark_targets(
        &self,
        id: Uuid,
        indexes: &[usize],
        state: FleetTargetState,
        code: &'static str,
    ) -> AppResult<()> {
        let mut items = self.items.lock().await;
        let run = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        for index in indexes {
            let target = run
                .targets
                .get_mut(*index)
                .ok_or(AppError::InvalidOperation)?;
            target.state = state;
            let target_id = target.target_id;
            push_event(run, Some(target_id), code);
        }
        self.persist_locked(&items).await
    }

    async fn record_execution_result(
        &self,
        id: Uuid,
        target_id: Uuid,
        result: AppResult<ChangeSet>,
        duration_ms: u64,
    ) -> AppResult<bool> {
        let mut items = self.items.lock().await;
        let run = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        let target = run
            .targets
            .iter_mut()
            .find(|item| item.target_id == target_id)
            .ok_or(AppError::InvalidOperation)?;
        target.duration_ms = Some(duration_ms);
        run.tool_call_count = run.tool_call_count.saturating_add(1);
        let failed = match result {
            Ok(change_set) if change_set.execution_state == ExecutionState::Committed => {
                target.state = FleetTargetState::Succeeded;
                target.local_verification = VerificationState::Succeeded;
                target.error_code = None;
                false
            }
            Ok(change_set) => {
                target.state = match change_set.execution_state {
                    ExecutionState::RolledBack => FleetTargetState::RolledBack,
                    ExecutionState::RollbackFailed => FleetTargetState::RollbackFailed,
                    _ => FleetTargetState::Failed,
                };
                target.local_verification = VerificationState::Failed;
                target.rollback_state = match change_set.execution_state {
                    ExecutionState::RolledBack => VerificationState::Succeeded,
                    ExecutionState::RollbackFailed => VerificationState::Failed,
                    _ => VerificationState::NotApplicable,
                };
                target.error_code = change_set
                    .steps
                    .iter()
                    .find_map(|step| step.error_code.clone())
                    .or_else(|| Some("TARGET_EXECUTION_FAILED".into()));
                true
            }
            Err(error) => {
                target.state = FleetTargetState::Failed;
                target.local_verification = VerificationState::Failed;
                target.error_code = Some(error.code().to_owned());
                true
            }
        };
        let code = if failed {
            "TARGET_EXECUTION_FAILED"
        } else {
            "TARGET_EXECUTION_SUCCEEDED"
        };
        push_event(run, Some(target_id), code);
        self.persist_locked(&items).await?;
        Ok(failed)
    }

    async fn apply_failure_policy(
        &self,
        id: Uuid,
        changes: &ChangeSetService,
        sessions: &ServerSessionManager,
        tools: &NativeToolExecutionService,
    ) -> AppResult<bool> {
        let policy = self.get(id).await?.failure_policy;
        match policy {
            FailurePolicy::Continue => Ok(false),
            FailurePolicy::Stop => {
                self.set_state(id, FleetExecutionState::Failed, "FLEET_STOPPED_ON_FAILURE")
                    .await?;
                Ok(true)
            }
            FailurePolicy::PauseForReview => {
                self.set_state(
                    id,
                    FleetExecutionState::PausedForReview,
                    "FLEET_PAUSED_FOR_REVIEW",
                )
                .await?;
                Ok(true)
            }
            FailurePolicy::Rollback => {
                self.rollback_completed(id, changes, sessions, tools)
                    .await?;
                Ok(true)
            }
        }
    }

    async fn rollback_completed(
        &self,
        id: Uuid,
        changes: &ChangeSetService,
        sessions: &ServerSessionManager,
        tools: &NativeToolExecutionService,
    ) -> AppResult<()> {
        self.set_state(
            id,
            FleetExecutionState::RollingBack,
            "FLEET_ROLLBACK_STARTED",
        )
        .await?;
        let targets = self.get(id).await?.targets;
        let completed = targets
            .iter()
            .rev()
            .filter(|target| target.state == FleetTargetState::Succeeded)
            .cloned()
            .collect::<Vec<_>>();
        let mut any_failed = targets
            .iter()
            .any(|target| target.state == FleetTargetState::RollbackFailed);
        for target in completed {
            self.mark_target_rollback(id, target.target_id, FleetTargetState::RollbackPending)
                .await?;
            let result = changes
                .rollback(
                    target.change_set_id,
                    target.change_set_version,
                    sessions,
                    tools,
                )
                .await;
            let succeeded = result
                .as_ref()
                .is_ok_and(|item| item.execution_state == ExecutionState::RolledBack);
            any_failed |= !succeeded;
            self.record_rollback_result(id, target.target_id, result)
                .await?;
        }
        self.set_state(
            id,
            if any_failed {
                FleetExecutionState::RollbackFailed
            } else {
                FleetExecutionState::RolledBack
            },
            if any_failed {
                "FLEET_ROLLBACK_FAILED"
            } else {
                "FLEET_ROLLED_BACK"
            },
        )
        .await
    }

    async fn mark_target_rollback(
        &self,
        id: Uuid,
        target_id: Uuid,
        state: FleetTargetState,
    ) -> AppResult<()> {
        let mut items = self.items.lock().await;
        let run = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        let target = run
            .targets
            .iter_mut()
            .find(|item| item.target_id == target_id)
            .ok_or(AppError::InvalidOperation)?;
        target.state = state;
        target.rollback_state = VerificationState::Pending;
        push_event(run, Some(target_id), "TARGET_ROLLBACK_PENDING");
        self.persist_locked(&items).await
    }

    async fn record_rollback_result(
        &self,
        id: Uuid,
        target_id: Uuid,
        result: AppResult<ChangeSet>,
    ) -> AppResult<()> {
        let mut items = self.items.lock().await;
        let run = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        run.tool_call_count = run.tool_call_count.saturating_add(1);
        let target = run
            .targets
            .iter_mut()
            .find(|item| item.target_id == target_id)
            .ok_or(AppError::InvalidOperation)?;
        let succeeded = result
            .as_ref()
            .is_ok_and(|item| item.execution_state == ExecutionState::RolledBack);
        target.state = if succeeded {
            FleetTargetState::RolledBack
        } else {
            FleetTargetState::RollbackFailed
        };
        target.rollback_state = if succeeded {
            VerificationState::Succeeded
        } else {
            VerificationState::Failed
        };
        if let Err(error) = result {
            target.error_code = Some(error.code().to_owned());
        }
        push_event(
            run,
            Some(target_id),
            if succeeded {
                "TARGET_ROLLED_BACK"
            } else {
                "TARGET_ROLLBACK_FAILED"
            },
        );
        self.persist_locked(&items).await
    }

    async fn verify_fleet(
        &self,
        id: Uuid,
        changes: &ChangeSetService,
        sessions: &ServerSessionManager,
        tools: &NativeToolExecutionService,
    ) -> AppResult<()> {
        self.set_state(id, FleetExecutionState::Verifying, "FLEET_VERIFYING")
            .await?;
        let snapshot = self.get(id).await?;
        if let Some(service) = snapshot.service_verification.clone() {
            for target in snapshot
                .targets
                .iter()
                .filter(|target| target.state == FleetTargetState::Succeeded)
            {
                let result = tools
                    .execute(
                        sessions,
                        NativeToolRequest::approved(
                            target.target_id,
                            NativeToolInvocation::ServiceStatus {
                                service: service.clone(),
                            },
                            ToolExecutionAuthority {
                                agent_run_id: snapshot.agent_run_id,
                                change_set_id: target.change_set_id,
                                change_set_version: target.change_set_version,
                                rollback: false,
                            },
                        ),
                    )
                    .await?;
                let active = result.success
                    && matches!(
                        result.data,
                        Some(ToolData::ServiceStatus(ref data))
                            if matches!(data.service.status, ServiceStatus::Active)
                    );
                self.record_service_verification(id, target.target_id, active)
                    .await?;
            }
        } else {
            let mut items = self.items.lock().await;
            let run = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
            for target in &mut run.targets {
                target.service_verification = VerificationState::NotApplicable;
            }
            run.verification.service_level = VerificationState::NotApplicable;
            self.persist_locked(&items).await?;
        }
        let mut items = self.items.lock().await;
        let run = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        let (local_ok, service_ok) =
            verification_outcome(&run.targets, run.service_verification.is_some());
        run.verification.service_level = if run.service_verification.is_some() {
            if service_ok {
                VerificationState::Succeeded
            } else {
                VerificationState::Failed
            }
        } else {
            VerificationState::NotApplicable
        };
        run.verification.cross_target = if run.cross_target_verification {
            if local_ok && service_ok {
                VerificationState::Succeeded
            } else {
                VerificationState::Failed
            }
        } else {
            VerificationState::NotApplicable
        };
        let verification_ok = local_ok && service_ok;
        run.execution_state = if verification_ok {
            FleetExecutionState::Succeeded
        } else {
            match run.failure_policy {
                FailurePolicy::PauseForReview => FleetExecutionState::PausedForReview,
                _ => FleetExecutionState::Failed,
            }
        };
        push_event(
            run,
            None,
            if verification_ok {
                "FLEET_VERIFICATION_SUCCEEDED"
            } else {
                "FLEET_VERIFICATION_FAILED"
            },
        );
        let needs_rollback = !verification_ok && run.failure_policy == FailurePolicy::Rollback;
        self.persist_locked(&items).await?;
        drop(items);
        if needs_rollback {
            self.rollback_completed(id, changes, sessions, tools)
                .await?;
        }
        Ok(())
    }

    async fn record_service_verification(
        &self,
        id: Uuid,
        target_id: Uuid,
        succeeded: bool,
    ) -> AppResult<()> {
        let mut items = self.items.lock().await;
        let run = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        run.tool_call_count = run.tool_call_count.saturating_add(1);
        let target = run
            .targets
            .iter_mut()
            .find(|item| item.target_id == target_id)
            .ok_or(AppError::InvalidOperation)?;
        target.service_verification = if succeeded {
            VerificationState::Succeeded
        } else {
            VerificationState::Failed
        };
        push_event(
            run,
            Some(target_id),
            if succeeded {
                "TARGET_SERVICE_VERIFIED"
            } else {
                "TARGET_SERVICE_VERIFICATION_FAILED"
            },
        );
        self.persist_locked(&items).await
    }

    async fn set_state(
        &self,
        id: Uuid,
        state: FleetExecutionState,
        code: &'static str,
    ) -> AppResult<()> {
        let mut items = self.items.lock().await;
        let run = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        run.execution_state = state;
        push_event(run, None, code);
        self.persist_locked(&items).await
    }

    async fn persist_locked(&self, items: &HashMap<Uuid, MultiChangeSet>) -> AppResult<()> {
        let Some(repository) = &self.repository else {
            return Ok(());
        };
        let mut records = items.values().map(persisted_run).collect::<Vec<_>>();
        records.sort_by_key(|item| item.id);
        if records.len() > MAX_FLEET_RUNS {
            records.drain(..records.len() - MAX_FLEET_RUNS);
        }
        repository.save_atomic(&records).await
    }
}

#[cfg(test)]
mod tests;
