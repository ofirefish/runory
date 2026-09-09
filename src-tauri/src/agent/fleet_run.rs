use std::collections::{HashMap, HashSet, VecDeque};

use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::fleet_target::{FleetTargetBinding, MAX_FLEET_TARGETS};
use super::repository::AgentStoreError;
use crate::agentic::context::redact_secrets;

pub const MAX_FLEET_STAGES: usize = 64;
const MAX_STAGE_SUMMARY_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FleetRunStateV2 {
    Draft,
    ValidatingTargets,
    Investigating,
    Planning,
    AwaitingApproval,
    Executing,
    Verifying,
    PausedForReview,
    Succeeded,
    Failed,
    RollingBack,
    RolledBack,
    RollbackFailed,
    Interrupted,
    Cancelled,
}

// M3's scheduler will call these transitions. They are fully exercised now so
// the future coordinator cannot invent ad-hoc state changes.
#[allow(dead_code)]
impl FleetRunStateV2 {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded
                | Self::Failed
                | Self::RolledBack
                | Self::RollbackFailed
                | Self::Interrupted
                | Self::Cancelled
        )
    }

    pub fn is_in_flight(self) -> bool {
        matches!(
            self,
            Self::ValidatingTargets
                | Self::Investigating
                | Self::Planning
                | Self::Executing
                | Self::Verifying
                | Self::RollingBack
        )
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        if self.is_terminal() {
            return false;
        }
        if matches!(next, Self::Cancelled | Self::Failed | Self::Interrupted) {
            return true;
        }
        match self {
            Self::Draft => matches!(next, Self::ValidatingTargets),
            Self::ValidatingTargets => matches!(next, Self::Investigating),
            Self::Investigating => matches!(next, Self::Planning | Self::PausedForReview),
            Self::Planning => matches!(next, Self::AwaitingApproval | Self::PausedForReview),
            Self::AwaitingApproval => matches!(next, Self::Executing | Self::Planning),
            Self::Executing => matches!(
                next,
                Self::Verifying | Self::PausedForReview | Self::RollingBack
            ),
            Self::Verifying => matches!(
                next,
                Self::Succeeded | Self::PausedForReview | Self::RollingBack
            ),
            Self::PausedForReview => matches!(
                next,
                Self::Investigating | Self::Planning | Self::Executing | Self::RollingBack
            ),
            Self::RollingBack => matches!(next, Self::RolledBack | Self::RollbackFailed),
            Self::Succeeded
            | Self::Failed
            | Self::RolledBack
            | Self::RollbackFailed
            | Self::Interrupted
            | Self::Cancelled => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FleetStageStateV2 {
    Pending,
    Ready,
    Running,
    AwaitingApproval,
    Verifying,
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
    RollbackPending,
    RolledBack,
    RollbackFailed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FleetExecutionStrategyV2 {
    Sequential,
    Canary,
    RollingBatch,
    Parallel,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FleetFailurePolicyV2 {
    Stop,
    PauseForReview,
    Continue,
    Rollback,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FleetRecoveryStateV2 {
    Live,
    MetadataOnly,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetStageDraft {
    pub id: Uuid,
    pub summary: String,
    pub target_ids: Vec<Uuid>,
    pub depends_on: Vec<Uuid>,
    pub execution_strategy: FleetExecutionStrategyV2,
    pub concurrency_limit: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetStage {
    pub id: Uuid,
    pub summary: String,
    pub target_ids: Vec<Uuid>,
    pub depends_on: Vec<Uuid>,
    pub execution_strategy: FleetExecutionStrategyV2,
    pub concurrency_limit: usize,
    pub state: FleetStageStateV2,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetChildRun {
    pub stage_id: Uuid,
    pub target_id: Uuid,
    pub agent_run_id: Option<Uuid>,
    pub attempt: u32,
    pub state: FleetStageStateV2,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetRunV2 {
    pub id: Uuid,
    pub version: u64,
    pub state: FleetRunStateV2,
    pub production: bool,
    pub failure_policy: FleetFailurePolicyV2,
    pub targets: Vec<FleetTargetBinding>,
    pub stages: Vec<FleetStage>,
    pub children: Vec<FleetChildRun>,
    pub graph_digest: String,
    pub recovery_state: FleetRecoveryStateV2,
    pub created_at_epoch_ms: u64,
    pub updated_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetRunDraft {
    pub production: bool,
    pub failure_policy: FleetFailurePolicyV2,
    pub targets: Vec<FleetTargetBinding>,
    pub stages: Vec<FleetStageDraft>,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum FleetPlanError {
    #[error("fleet target set is invalid")]
    InvalidTargets,
    #[error("fleet stage set is invalid")]
    InvalidStages,
    #[error("fleet stage references an unknown target")]
    UnknownTarget,
    #[error("fleet stage dependency is invalid")]
    InvalidDependency,
    #[error("fleet stage graph contains a cycle")]
    DependencyCycle,
    #[error("production parallel execution is prohibited")]
    ProductionParallel,
    #[error("fleet state transition is invalid")]
    InvalidTransition,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphDigestInput<'a> {
    production: bool,
    failure_policy: FleetFailurePolicyV2,
    targets: &'a [FleetTargetBinding],
    stages: &'a [FleetStage],
}

impl FleetRunV2 {
    pub fn draft(request: FleetRunDraft, now_epoch_ms: u64) -> Result<Self, FleetPlanError> {
        validate_targets(&request.targets)?;
        let stages = validate_stages(request.production, &request.targets, request.stages)?;
        let graph_digest = graph_digest(
            request.production,
            request.failure_policy,
            &request.targets,
            &stages,
        )?;
        Ok(Self {
            id: Uuid::new_v4(),
            version: 1,
            state: FleetRunStateV2::Draft,
            production: request.production,
            failure_policy: request.failure_policy,
            targets: request.targets,
            stages,
            children: Vec::new(),
            graph_digest,
            recovery_state: FleetRecoveryStateV2::Live,
            created_at_epoch_ms: now_epoch_ms,
            updated_at_epoch_ms: now_epoch_ms,
        })
    }

    #[allow(dead_code)]
    pub fn transition_to(
        &mut self,
        next: FleetRunStateV2,
        now_epoch_ms: u64,
    ) -> Result<(), FleetPlanError> {
        if self.recovery_state != FleetRecoveryStateV2::Live || !self.state.can_transition_to(next)
        {
            return Err(FleetPlanError::InvalidTransition);
        }
        self.state = next;
        self.updated_at_epoch_ms = now_epoch_ms;
        Ok(())
    }

    pub(crate) fn from_metadata(
        id: Uuid,
        version: u64,
        state: FleetRunStateV2,
        production: bool,
        failure_policy: FleetFailurePolicyV2,
        targets: Vec<FleetTargetBinding>,
        stages: Vec<FleetStage>,
        children: Vec<FleetChildRun>,
        graph_digest: String,
        recovery_state: FleetRecoveryStateV2,
        created_at_epoch_ms: u64,
        updated_at_epoch_ms: u64,
    ) -> Self {
        Self {
            id,
            version,
            state,
            production,
            failure_policy,
            targets,
            stages,
            children,
            graph_digest,
            recovery_state,
            created_at_epoch_ms,
            updated_at_epoch_ms,
        }
    }

    pub(crate) fn validate_metadata(&self) -> Result<(), FleetPlanError> {
        validate_targets(&self.targets)?;
        if self.stages.is_empty() || self.stages.len() > MAX_FLEET_STAGES {
            return Err(FleetPlanError::InvalidStages);
        }
        let target_ids = self
            .targets
            .iter()
            .map(|target| target.profile_id)
            .collect::<HashSet<_>>();
        let stage_ids = self
            .stages
            .iter()
            .map(|stage| stage.id)
            .collect::<HashSet<_>>();
        if stage_ids.len() != self.stages.len()
            || self.graph_digest.len() != 64
            || !self
                .graph_digest
                .bytes()
                .all(|value| value.is_ascii_hexdigit())
        {
            return Err(FleetPlanError::InvalidStages);
        }
        for stage in &self.stages {
            let selected = stage.target_ids.iter().copied().collect::<HashSet<_>>();
            let dependencies = stage.depends_on.iter().copied().collect::<HashSet<_>>();
            if selected.is_empty()
                || selected.len() != stage.target_ids.len()
                || !selected.is_subset(&target_ids)
                || dependencies.len() != stage.depends_on.len()
                || dependencies.contains(&stage.id)
                || !dependencies.is_subset(&stage_ids)
                || stage.concurrency_limit == 0
                || stage.concurrency_limit > stage.target_ids.len()
                || (stage.execution_strategy == FleetExecutionStrategyV2::Sequential
                    && stage.concurrency_limit != 1)
            {
                return Err(FleetPlanError::InvalidStages);
            }
            if self.production && stage.execution_strategy == FleetExecutionStrategyV2::Parallel {
                return Err(FleetPlanError::ProductionParallel);
            }
        }
        validate_acyclic(&self.stages)?;
        let mut child_keys = HashSet::with_capacity(self.children.len());
        for child in &self.children {
            if !stage_ids.contains(&child.stage_id)
                || !target_ids.contains(&child.target_id)
                || child.attempt == 0
                || !child_keys.insert((child.stage_id, child.target_id, child.attempt))
                || child.error_code.as_ref().is_some_and(|code| {
                    code.is_empty()
                        || code.len() > 96
                        || !code
                            .bytes()
                            .all(|value| value.is_ascii_uppercase() || value == b'_')
                })
            {
                return Err(FleetPlanError::InvalidStages);
            }
        }
        Ok(())
    }
}

pub trait FleetRunStore: Send + Sync {
    fn insert_fleet_run(&self, run: &FleetRunV2) -> Result<(), AgentStoreError>;
    #[allow(dead_code)]
    fn save_fleet_run(&self, run: &FleetRunV2) -> Result<(), AgentStoreError>;
    fn get_fleet_run(&self, id: Uuid) -> Result<Option<FleetRunV2>, AgentStoreError>;
    fn list_fleet_runs(&self) -> Result<Vec<FleetRunV2>, AgentStoreError>;
    fn recover_fleet_runs(&self) -> Result<usize, AgentStoreError>;
}

fn validate_targets(targets: &[FleetTargetBinding]) -> Result<(), FleetPlanError> {
    if !(2..=MAX_FLEET_TARGETS).contains(&targets.len()) {
        return Err(FleetPlanError::InvalidTargets);
    }
    let mut profiles = HashSet::with_capacity(targets.len());
    let mut sessions = HashSet::with_capacity(targets.len());
    for (ordinal, target) in targets.iter().enumerate() {
        if target.ordinal != ordinal
            || !profiles.insert(target.profile_id)
            || !sessions.insert(target.session_id)
        {
            return Err(FleetPlanError::InvalidTargets);
        }
    }
    Ok(())
}

fn validate_stages(
    production: bool,
    targets: &[FleetTargetBinding],
    drafts: Vec<FleetStageDraft>,
) -> Result<Vec<FleetStage>, FleetPlanError> {
    if drafts.is_empty() || drafts.len() > MAX_FLEET_STAGES {
        return Err(FleetPlanError::InvalidStages);
    }
    let known_targets = targets
        .iter()
        .map(|target| target.profile_id)
        .collect::<HashSet<_>>();
    let known_stages = drafts.iter().map(|stage| stage.id).collect::<HashSet<_>>();
    if known_stages.len() != drafts.len() {
        return Err(FleetPlanError::InvalidStages);
    }

    let mut stages = Vec::with_capacity(drafts.len());
    for draft in drafts {
        let summary = draft.summary.trim();
        let (summary, _) = redact_secrets(summary);
        if summary.is_empty() || summary.len() > MAX_STAGE_SUMMARY_BYTES {
            return Err(FleetPlanError::InvalidStages);
        }
        let unique_targets = draft.target_ids.iter().copied().collect::<HashSet<_>>();
        if draft.target_ids.is_empty() || unique_targets.len() != draft.target_ids.len() {
            return Err(FleetPlanError::InvalidStages);
        }
        if !unique_targets.is_subset(&known_targets) {
            return Err(FleetPlanError::UnknownTarget);
        }
        let unique_dependencies = draft.depends_on.iter().copied().collect::<HashSet<_>>();
        if unique_dependencies.len() != draft.depends_on.len()
            || unique_dependencies.contains(&draft.id)
            || !unique_dependencies.is_subset(&known_stages)
        {
            return Err(FleetPlanError::InvalidDependency);
        }
        if draft.concurrency_limit == 0 || draft.concurrency_limit > draft.target_ids.len() {
            return Err(FleetPlanError::InvalidStages);
        }
        if draft.execution_strategy == FleetExecutionStrategyV2::Sequential
            && draft.concurrency_limit != 1
        {
            return Err(FleetPlanError::InvalidStages);
        }
        if production && draft.execution_strategy == FleetExecutionStrategyV2::Parallel {
            return Err(FleetPlanError::ProductionParallel);
        }
        stages.push(FleetStage {
            id: draft.id,
            summary,
            target_ids: draft.target_ids,
            depends_on: draft.depends_on,
            execution_strategy: draft.execution_strategy,
            concurrency_limit: draft.concurrency_limit,
            state: FleetStageStateV2::Pending,
        });
    }
    validate_acyclic(&stages)?;
    Ok(stages)
}

fn validate_acyclic(stages: &[FleetStage]) -> Result<(), FleetPlanError> {
    let mut incoming = stages
        .iter()
        .map(|stage| (stage.id, stage.depends_on.len()))
        .collect::<HashMap<_, _>>();
    let mut dependants: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for stage in stages {
        for dependency in &stage.depends_on {
            dependants.entry(*dependency).or_default().push(stage.id);
        }
    }
    let mut ready = incoming
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id))
        .collect::<VecDeque<_>>();
    let mut visited = 0;
    while let Some(id) = ready.pop_front() {
        visited += 1;
        for dependant in dependants.get(&id).into_iter().flatten() {
            let count = incoming
                .get_mut(dependant)
                .ok_or(FleetPlanError::InvalidDependency)?;
            *count -= 1;
            if *count == 0 {
                ready.push_back(*dependant);
            }
        }
    }
    if visited == stages.len() {
        Ok(())
    } else {
        Err(FleetPlanError::DependencyCycle)
    }
}

fn graph_digest(
    production: bool,
    failure_policy: FleetFailurePolicyV2,
    targets: &[FleetTargetBinding],
    stages: &[FleetStage],
) -> Result<String, FleetPlanError> {
    let encoded = serde_json::to_vec(&GraphDigestInput {
        production,
        failure_policy,
        targets,
        stages,
    })
    .map_err(|_| FleetPlanError::InvalidStages)?;
    Ok(digest(&SHA256, &encoded)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(ordinal: usize, role: &str) -> FleetTargetBinding {
        FleetTargetBinding {
            profile_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            role: Some(role.into()),
            ordinal,
        }
    }

    fn draft(targets: Vec<FleetTargetBinding>, stages: Vec<FleetStageDraft>) -> FleetRunDraft {
        FleetRunDraft {
            production: true,
            failure_policy: FleetFailurePolicyV2::PauseForReview,
            targets,
            stages,
        }
    }

    #[test]
    fn creates_an_exact_acyclic_goal_graph_and_digest() {
        let targets = vec![target(0, "source"), target(1, "replica")];
        let prepare = Uuid::new_v4();
        let verify = Uuid::new_v4();
        let run = FleetRunV2::draft(
            draft(
                targets.clone(),
                vec![
                    FleetStageDraft {
                        id: prepare,
                        summary: "Prepare source".into(),
                        target_ids: vec![targets[0].profile_id],
                        depends_on: vec![],
                        execution_strategy: FleetExecutionStrategyV2::Sequential,
                        concurrency_limit: 1,
                    },
                    FleetStageDraft {
                        id: verify,
                        summary: "Join replica".into(),
                        target_ids: vec![targets[1].profile_id],
                        depends_on: vec![prepare],
                        execution_strategy: FleetExecutionStrategyV2::Sequential,
                        concurrency_limit: 1,
                    },
                ],
            ),
            100,
        )
        .expect("valid fleet run");

        assert_eq!(run.version, 1);
        assert_eq!(run.graph_digest.len(), 64);
        assert_eq!(run.state, FleetRunStateV2::Draft);
        assert_eq!(run.stages[1].depends_on, vec![prepare]);
    }

    #[test]
    fn rejects_cycles_unknown_targets_and_production_parallelism() {
        let targets = vec![target(0, "source"), target(1, "replica")];
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let cyclic = FleetRunV2::draft(
            draft(
                targets.clone(),
                vec![
                    FleetStageDraft {
                        id: first,
                        summary: "First".into(),
                        target_ids: vec![targets[0].profile_id],
                        depends_on: vec![second],
                        execution_strategy: FleetExecutionStrategyV2::Sequential,
                        concurrency_limit: 1,
                    },
                    FleetStageDraft {
                        id: second,
                        summary: "Second".into(),
                        target_ids: vec![targets[1].profile_id],
                        depends_on: vec![first],
                        execution_strategy: FleetExecutionStrategyV2::Sequential,
                        concurrency_limit: 1,
                    },
                ],
            ),
            100,
        );
        assert!(matches!(cyclic, Err(FleetPlanError::DependencyCycle)));

        let unknown = FleetRunV2::draft(
            draft(
                targets.clone(),
                vec![FleetStageDraft {
                    id: first,
                    summary: "Unknown target".into(),
                    target_ids: vec![Uuid::new_v4()],
                    depends_on: vec![],
                    execution_strategy: FleetExecutionStrategyV2::Sequential,
                    concurrency_limit: 1,
                }],
            ),
            100,
        );
        assert!(matches!(unknown, Err(FleetPlanError::UnknownTarget)));

        let parallel = FleetRunV2::draft(
            draft(
                targets.clone(),
                vec![FleetStageDraft {
                    id: first,
                    summary: "Parallel".into(),
                    target_ids: targets.iter().map(|target| target.profile_id).collect(),
                    depends_on: vec![],
                    execution_strategy: FleetExecutionStrategyV2::Parallel,
                    concurrency_limit: 2,
                }],
            ),
            100,
        );
        assert!(matches!(parallel, Err(FleetPlanError::ProductionParallel)));
    }

    #[test]
    fn recovered_metadata_cannot_transition_or_execute() {
        let targets = vec![target(0, "source"), target(1, "replica")];
        let stage_id = Uuid::new_v4();
        let mut run = FleetRunV2::draft(
            draft(
                targets.clone(),
                vec![FleetStageDraft {
                    id: stage_id,
                    summary: "Inspect".into(),
                    target_ids: vec![targets[0].profile_id],
                    depends_on: vec![],
                    execution_strategy: FleetExecutionStrategyV2::Sequential,
                    concurrency_limit: 1,
                }],
            ),
            100,
        )
        .expect("valid fleet run");
        run.recovery_state = FleetRecoveryStateV2::MetadataOnly;

        assert!(matches!(
            run.transition_to(FleetRunStateV2::ValidatingTargets, 200),
            Err(FleetPlanError::InvalidTransition)
        ));
    }

    #[test]
    fn transition_table_blocks_terminal_resumption() {
        assert!(FleetRunStateV2::Draft.can_transition_to(FleetRunStateV2::ValidatingTargets));
        assert!(!FleetRunStateV2::Draft.can_transition_to(FleetRunStateV2::Executing));
        for terminal in [
            FleetRunStateV2::Succeeded,
            FleetRunStateV2::Failed,
            FleetRunStateV2::Interrupted,
            FleetRunStateV2::Cancelled,
        ] {
            assert!(!terminal.can_transition_to(FleetRunStateV2::Executing));
        }
    }
}
