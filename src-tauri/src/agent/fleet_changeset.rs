//! Adapter from Runtime V2 Fleet plans to the established Fleet ChangeSet
//! execution service. It adds no write primitive: every step remains a Native
//! Typed Tool owned by `ChangeSetService`.

use std::collections::{HashMap, HashSet};

use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::fleet_run::{FleetRecoveryStateV2, FleetRunStateV2, FleetRunV2};
use crate::agentic::{
    ChangeSet, ChangeSetDraftRequest, ChangeSetService, ChangeStepDraft, ExecutionStrategy,
    FailurePolicy, FleetExecutionService, FleetOrchestrationBinding,
    FleetOrchestrationTargetBinding, MultiChangeSet, MultiChangeSetDraftRequest,
    PolicyCheckContext,
};
use crate::domain::{AppError, AppResult};
use crate::policy::{
    AgentPolicyService, PolicyExecutionStrategy, PolicyInvocationSource, PolicyTarget,
};
use crate::ssh::ServerSessionManager;
use crate::tools::NativeToolExecutionService;

const MAX_FLEET_CHANGE_TITLE_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FleetChangeExecutionStrategy {
    Sequential,
    Canary,
    RollingBatch,
    Parallel,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetTargetChangeDraft {
    pub profile_id: Uuid,
    pub title: String,
    pub steps: Vec<ChangeStepDraft>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetChangeSetDraftRequest {
    pub fleet_run_id: Uuid,
    pub fleet_run_version: u64,
    pub title: String,
    pub targets: Vec<FleetTargetChangeDraft>,
    pub execution_strategy: FleetChangeExecutionStrategy,
    pub batch_size: usize,
    pub canary_count: usize,
    pub service_verification: Option<String>,
    pub cross_target_verification: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetTargetChangeReview {
    pub profile_id: Uuid,
    pub session_id: Uuid,
    pub role: Option<String>,
    pub change_set: ChangeSet,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetChangeSetReview {
    pub fleet_run_id: Uuid,
    pub fleet_run_version: u64,
    pub graph_digest: String,
    pub orchestration_binding_digest: String,
    pub execution: MultiChangeSet,
    pub targets: Vec<FleetTargetChangeReview>,
}

impl FleetChangeSetReview {
    /// Returns the newest live ChangeSet execution bound to this exact Fleet
    /// graph. This is a read-only projection for the approval UI; it never
    /// reconstructs missing change content from metadata-only recovery data.
    pub async fn latest_bound(
        run: &FleetRunV2,
        changes: &ChangeSetService,
        fleets: &FleetExecutionService,
    ) -> AppResult<Option<Self>> {
        let execution = fleets
            .list()
            .await?
            .into_iter()
            .filter(|execution| validate_execution_binding(run, execution).is_ok())
            .max_by_key(|execution| {
                execution
                    .audit
                    .last()
                    .map(|event| event.occurred_at_epoch_ms)
                    .unwrap_or_default()
            });
        let Some(execution) = execution else {
            return Ok(None);
        };
        if execution.recovery_state != crate::agentic::FleetRecoveryState::Live {
            return Ok(None);
        }
        let binding = execution
            .orchestration_binding
            .as_ref()
            .ok_or(AppError::InvalidOperation)?;
        let mut targets = Vec::with_capacity(execution.targets.len());
        for target in &execution.targets {
            let exact = run
                .targets
                .iter()
                .find(|candidate| candidate.session_id == target.target_id)
                .ok_or(AppError::InvalidOperation)?;
            let change_set = changes.get(target.change_set_id).await?;
            if change_set.version != target.change_set_version
                || change_set.session_id != exact.session_id
            {
                return Err(AppError::InvalidOperation);
            }
            targets.push(FleetTargetChangeReview {
                profile_id: exact.profile_id,
                session_id: exact.session_id,
                role: exact.role.clone(),
                change_set,
            });
        }
        Ok(Some(Self {
            fleet_run_id: run.id,
            fleet_run_version: run.version,
            graph_digest: run.graph_digest.clone(),
            orchestration_binding_digest: binding.binding_digest.clone(),
            execution,
            targets,
        }))
    }

    pub async fn draft(
        run: &FleetRunV2,
        request: FleetChangeSetDraftRequest,
        changes: &ChangeSetService,
        fleets: &FleetExecutionService,
        policies: &AgentPolicyService,
    ) -> AppResult<Self> {
        validate_parent(run, request.fleet_run_id, request.fleet_run_version)?;
        if request.title.trim().is_empty()
            || request.title.len() > MAX_FLEET_CHANGE_TITLE_BYTES
            || request.targets.len() != run.targets.len()
        {
            return Err(AppError::InvalidOperation);
        }
        let strategy = map_strategy(request.execution_strategy);
        if run.production && strategy == ExecutionStrategy::Parallel {
            return Err(AppError::InvalidOperation);
        }
        let binding = binding(run, &request, strategy)?;
        let mut drafts = request
            .targets
            .into_iter()
            .map(|target| (target.profile_id, target))
            .collect::<HashMap<_, _>>();
        if drafts.len() != run.targets.len()
            || run
                .targets
                .iter()
                .any(|target| !drafts.contains_key(&target.profile_id))
        {
            return Err(AppError::InvalidOperation);
        }
        let target_requests = run
            .targets
            .iter()
            .map(|binding| {
                let target = drafts
                    .remove(&binding.profile_id)
                    .ok_or(AppError::InvalidOperation)?;
                Ok(ChangeSetDraftRequest {
                    agent_run_id: run.id,
                    session_id: binding.session_id,
                    title: target.title,
                    steps: target.steps,
                })
            })
            .collect::<AppResult<Vec<_>>>()?;
        let execution = fleets
            .draft_bound(
                changes,
                MultiChangeSetDraftRequest {
                    agent_run_id: run.id,
                    title: request.title,
                    targets: target_requests,
                    execution_strategy: strategy,
                    failure_policy: map_failure(run),
                    batch_size: request.batch_size,
                    canary_count: request.canary_count,
                    production: run.production,
                    service_verification: request.service_verification.clone(),
                    cross_target_verification: request.cross_target_verification,
                },
                binding.clone(),
            )
            .await?;

        let policy_strategy = map_policy_strategy(strategy);
        let target_count = run.targets.len();
        let mut reviews = Vec::with_capacity(execution.targets.len());
        for target in &execution.targets {
            let exact = run
                .targets
                .iter()
                .find(|binding| binding.session_id == target.target_id)
                .ok_or(AppError::InvalidOperation)?;
            let change_set = changes
                .check_policy(
                    target.change_set_id,
                    target.change_set_version,
                    policies,
                    PolicyCheckContext {
                        target: PolicyTarget {
                            server_id: exact.profile_id,
                            group_id: None,
                            environment: None,
                        },
                        target_count,
                        execution_strategy: policy_strategy,
                        source: PolicyInvocationSource::MultiServerChangeSet,
                    },
                )
                .await?;
            reviews.push(FleetTargetChangeReview {
                profile_id: exact.profile_id,
                session_id: exact.session_id,
                role: exact.role.clone(),
                change_set,
            });
        }
        Ok(Self {
            fleet_run_id: run.id,
            fleet_run_version: run.version,
            graph_digest: run.graph_digest.clone(),
            orchestration_binding_digest: binding.binding_digest,
            execution,
            targets: reviews,
        })
    }

    /// Captures fresh read-only preconditions and replaces each public
    /// ChangeSet projection so the approval UI reviews the exact bound state.
    pub async fn capture_preconditions(
        mut self,
        changes: &ChangeSetService,
        sessions: &ServerSessionManager,
        tools: &NativeToolExecutionService,
    ) -> AppResult<Self> {
        for target in &mut self.targets {
            target.change_set = changes
                .capture_preconditions(
                    target.change_set.id,
                    target.change_set.version,
                    sessions,
                    tools,
                )
                .await?;
        }
        Ok(self)
    }

    pub async fn approve(
        run: &FleetRunV2,
        execution_id: Uuid,
        execution_version: u64,
        changes: &ChangeSetService,
        fleets: &FleetExecutionService,
    ) -> AppResult<MultiChangeSet> {
        let execution = fleets.get(execution_id).await?;
        validate_execution_binding(run, &execution)?;
        fleets
            .approve(
                changes,
                execution_id,
                execution_version,
                execution
                    .targets
                    .iter()
                    .map(|target| target.target_id)
                    .collect(),
            )
            .await
    }

    pub async fn execute(
        run: &FleetRunV2,
        execution_id: Uuid,
        execution_version: u64,
        changes: &ChangeSetService,
        fleets: &FleetExecutionService,
        sessions: &ServerSessionManager,
        tools: &NativeToolExecutionService,
        policies: &AgentPolicyService,
    ) -> AppResult<MultiChangeSet> {
        let execution = fleets.get(execution_id).await?;
        validate_execution_binding(run, &execution)?;
        fleets
            .execute(
                changes,
                sessions,
                tools,
                policies,
                execution_id,
                execution_version,
            )
            .await
    }

    pub async fn rollback(
        run: &FleetRunV2,
        execution_id: Uuid,
        execution_version: u64,
        changes: &ChangeSetService,
        fleets: &FleetExecutionService,
        sessions: &ServerSessionManager,
        tools: &NativeToolExecutionService,
    ) -> AppResult<MultiChangeSet> {
        let execution = fleets.get(execution_id).await?;
        validate_execution_binding(run, &execution)?;
        fleets
            .rollback_explicit(changes, sessions, tools, execution_id, execution_version)
            .await
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VerificationMaterial<'a> {
    service: &'a Option<String>,
    cross_target: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BindingMaterial<'a> {
    fleet_run_id: Uuid,
    fleet_run_version: u64,
    graph_digest: &'a str,
    targets: &'a [FleetOrchestrationTargetBinding],
    execution_strategy: ExecutionStrategy,
    failure_policy: FailurePolicy,
    batch_size: usize,
    canary_count: usize,
    production: bool,
    verification_contract_digest: &'a str,
}

fn binding(
    run: &FleetRunV2,
    request: &FleetChangeSetDraftRequest,
    strategy: ExecutionStrategy,
) -> AppResult<FleetOrchestrationBinding> {
    let targets = run
        .targets
        .iter()
        .map(|target| FleetOrchestrationTargetBinding {
            profile_id: target.profile_id,
            session_id: target.session_id,
            role: target.role.clone(),
            ordinal: target.ordinal,
        })
        .collect::<Vec<_>>();
    let verification_contract_digest = hash(&VerificationMaterial {
        service: &request.service_verification,
        cross_target: request.cross_target_verification,
    })?;
    let binding_digest = hash(&BindingMaterial {
        fleet_run_id: run.id,
        fleet_run_version: run.version,
        graph_digest: &run.graph_digest,
        targets: &targets,
        execution_strategy: strategy,
        failure_policy: map_failure(run),
        batch_size: request.batch_size,
        canary_count: request.canary_count,
        production: run.production,
        verification_contract_digest: &verification_contract_digest,
    })?;
    Ok(FleetOrchestrationBinding {
        fleet_run_id: run.id,
        fleet_run_version: run.version,
        graph_digest: run.graph_digest.clone(),
        targets,
        verification_contract_digest,
        binding_digest,
    })
}

fn validate_parent(run: &FleetRunV2, id: Uuid, version: u64) -> AppResult<()> {
    if run.id != id
        || run.version != version
        || run.recovery_state != FleetRecoveryStateV2::Live
        || !matches!(
            run.state,
            FleetRunStateV2::Approved
                | FleetRunStateV2::Executing
                | FleetRunStateV2::Verifying
                | FleetRunStateV2::PausedForReview
        )
    {
        return Err(AppError::InvalidOperation);
    }
    Ok(())
}

fn validate_execution_binding(run: &FleetRunV2, execution: &MultiChangeSet) -> AppResult<()> {
    let binding = execution
        .orchestration_binding
        .as_ref()
        .ok_or(AppError::InvalidOperation)?;
    validate_parent(run, binding.fleet_run_id, binding.fleet_run_version)?;
    if binding.graph_digest != run.graph_digest
        || binding.targets.len() != run.targets.len()
        || binding
            .targets
            .iter()
            .zip(&run.targets)
            .any(|(left, right)| {
                left.profile_id != right.profile_id
                    || left.session_id != right.session_id
                    || left.role != right.role
                    || left.ordinal != right.ordinal
            })
    {
        return Err(AppError::InvalidOperation);
    }
    let sessions = execution
        .targets
        .iter()
        .map(|target| target.target_id)
        .collect::<HashSet<_>>();
    if sessions.len() != run.targets.len()
        || run
            .targets
            .iter()
            .any(|target| !sessions.contains(&target.session_id))
    {
        return Err(AppError::InvalidOperation);
    }
    let verification_digest = hash(&VerificationMaterial {
        service: &execution.service_verification,
        cross_target: execution.cross_target_verification,
    })?;
    let expected = hash(&BindingMaterial {
        fleet_run_id: run.id,
        fleet_run_version: run.version,
        graph_digest: &run.graph_digest,
        targets: &binding.targets,
        execution_strategy: execution.execution_strategy,
        failure_policy: execution.failure_policy,
        batch_size: execution.batch_size,
        canary_count: execution.canary_count,
        production: execution.production,
        verification_contract_digest: &verification_digest,
    })?;
    if verification_digest != binding.verification_contract_digest
        || expected != binding.binding_digest
    {
        return Err(AppError::InvalidOperation);
    }
    Ok(())
}

fn hash(value: &impl Serialize) -> AppResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|_| AppError::Storage)?;
    Ok(digest(&SHA256, &bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn map_strategy(strategy: FleetChangeExecutionStrategy) -> ExecutionStrategy {
    match strategy {
        FleetChangeExecutionStrategy::Sequential => ExecutionStrategy::Sequential,
        FleetChangeExecutionStrategy::Canary => ExecutionStrategy::Canary,
        FleetChangeExecutionStrategy::RollingBatch => ExecutionStrategy::RollingBatch,
        FleetChangeExecutionStrategy::Parallel => ExecutionStrategy::Parallel,
    }
}

fn map_policy_strategy(strategy: ExecutionStrategy) -> PolicyExecutionStrategy {
    match strategy {
        ExecutionStrategy::Sequential => PolicyExecutionStrategy::Sequential,
        ExecutionStrategy::Canary => PolicyExecutionStrategy::Canary,
        ExecutionStrategy::RollingBatch => PolicyExecutionStrategy::Rolling,
        ExecutionStrategy::Parallel => PolicyExecutionStrategy::Parallel,
    }
}

fn map_failure(run: &FleetRunV2) -> FailurePolicy {
    use super::fleet_run::FleetFailurePolicyV2 as Source;
    match run.failure_policy {
        Source::Stop => FailurePolicy::Stop,
        Source::PauseForReview => FailurePolicy::PauseForReview,
        Source::Continue => FailurePolicy::Continue,
        Source::Rollback => FailurePolicy::Rollback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::fleet_run::{
        FleetExecutionStrategyV2, FleetFailurePolicyV2, FleetRunDraft, FleetStageDraft,
    };
    use crate::agent::fleet_target::FleetTargetBinding;

    fn run(production: bool) -> FleetRunV2 {
        let targets = vec![
            FleetTargetBinding {
                profile_id: Uuid::new_v4(),
                session_id: Uuid::new_v4(),
                role: Some("source".into()),
                ordinal: 0,
            },
            FleetTargetBinding {
                profile_id: Uuid::new_v4(),
                session_id: Uuid::new_v4(),
                role: Some("replica".into()),
                ordinal: 1,
            },
        ];
        let mut run = FleetRunV2::draft(
            FleetRunDraft {
                production,
                failure_policy: FleetFailurePolicyV2::PauseForReview,
                stages: vec![FleetStageDraft {
                    id: Uuid::new_v4(),
                    summary: "Reload nginx".into(),
                    target_ids: targets.iter().map(|target| target.profile_id).collect(),
                    depends_on: vec![],
                    execution_strategy: FleetExecutionStrategyV2::Sequential,
                    concurrency_limit: 1,
                }],
                targets,
            },
            10,
        )
        .expect("fleet");
        run.state = FleetRunStateV2::Approved;
        run
    }

    fn request(
        run: &FleetRunV2,
        strategy: FleetChangeExecutionStrategy,
    ) -> FleetChangeSetDraftRequest {
        FleetChangeSetDraftRequest {
            fleet_run_id: run.id,
            fleet_run_version: run.version,
            title: "Reload fleet".into(),
            targets: run
                .targets
                .iter()
                .map(|target| FleetTargetChangeDraft {
                    profile_id: target.profile_id,
                    title: format!("Reload {}", target.role.as_deref().unwrap_or("target")),
                    steps: vec![ChangeStepDraft::NginxReload],
                })
                .collect(),
            execution_strategy: strategy,
            batch_size: 1,
            canary_count: 1,
            service_verification: Some("nginx".into()),
            cross_target_verification: true,
        }
    }

    #[tokio::test]
    async fn drafts_independent_changesets_with_exact_orchestration_binding() {
        let run = run(true);
        let changes = ChangeSetService::default();
        let fleets = FleetExecutionService::default();
        let policies = AgentPolicyService::default();
        let review = FleetChangeSetReview::draft(
            &run,
            request(&run, FleetChangeExecutionStrategy::Sequential),
            &changes,
            &fleets,
            &policies,
        )
        .await
        .expect("review");

        assert_eq!(review.targets.len(), run.targets.len());
        assert!(review.targets.iter().all(|target| {
            target.change_set.steps.len() == 1
                && target.change_set.steps[0].tool_name == "nginx.reload"
                && !target.change_set.steps[0].rollback_capability.is_empty()
        }));
        let binding = review
            .execution
            .orchestration_binding
            .as_ref()
            .expect("binding");
        assert_eq!(binding.graph_digest, run.graph_digest);
        assert_eq!(binding.targets[0].role.as_deref(), Some("source"));

        let loaded = FleetChangeSetReview::latest_bound(&run, &changes, &fleets)
            .await
            .expect("load bound review")
            .expect("bound review exists");
        assert_eq!(loaded.execution.id, review.execution.id);
        assert_eq!(loaded.targets.len(), run.targets.len());

        let mut changed_run = run.clone();
        changed_run.version += 1;
        assert!(
            FleetChangeSetReview::latest_bound(&changed_run, &changes, &fleets)
                .await
                .expect("reject stale binding")
                .is_none()
        );
        assert_eq!(binding.binding_digest, review.orchestration_binding_digest);
        validate_execution_binding(&run, &review.execution).expect("exact binding");
    }

    #[tokio::test]
    async fn role_or_graph_change_invalidates_execution_binding() {
        let run = run(true);
        let review = FleetChangeSetReview::draft(
            &run,
            request(&run, FleetChangeExecutionStrategy::Sequential),
            &ChangeSetService::default(),
            &FleetExecutionService::default(),
            &AgentPolicyService::default(),
        )
        .await
        .expect("review");
        let mut changed = run.clone();
        changed.targets[0].role = Some("other".into());
        assert!(validate_execution_binding(&changed, &review.execution).is_err());
        changed = run.clone();
        changed.graph_digest = "0".repeat(64);
        assert!(validate_execution_binding(&changed, &review.execution).is_err());
    }

    #[tokio::test]
    async fn production_parallel_changes_are_rejected() {
        let run = run(true);
        let result = FleetChangeSetReview::draft(
            &run,
            request(&run, FleetChangeExecutionStrategy::Parallel),
            &ChangeSetService::default(),
            &FleetExecutionService::default(),
            &AgentPolicyService::default(),
        )
        .await;
        assert!(result.is_err());
    }
}
