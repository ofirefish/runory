use std::collections::HashMap;
use std::sync::Arc;

use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::domain::{AppError, AppResult};
use crate::policy::{
    AgentPolicyService, PolicyDecision, PolicyEvaluation, PolicyEvaluationRequest,
    PolicyExecutionStrategy, PolicyInvocationSource, PolicyMatchedRule, PolicySnapshot,
    PolicyTarget,
};
use crate::ssh::ServerSessionManager;
use crate::storage::JsonRepository;
use crate::tools::{
    NativeToolExecutionService, NativeToolInvocation, NativeToolRequest, RiskLevel,
    ToolExecutionAuthority,
};

const MAX_STEPS: usize = 12;
const MAX_PATCH_BYTES: usize = 512 * 1024;
const MAX_PERSISTED_CHANGE_SETS: usize = 2_000;
const CHANGE_SET_SCHEMA_VERSION: u8 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "tool", deny_unknown_fields)]
pub(crate) enum ChangeStepDraft {
    #[serde(rename = "file.patch")]
    FilePatch {
        path: String,
        expected: String,
        replacement: String,
    },
    #[serde(rename = "service.restart")]
    ServiceRestart { service: String },
    #[serde(rename = "service.reload")]
    ServiceReload { service: String },
    #[serde(rename = "nginx.reload")]
    NginxReload,
    #[serde(rename = "docker.restart")]
    DockerRestart { container: String },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangeSetDraftRequest {
    pub agent_run_id: Uuid,
    pub session_id: Uuid,
    pub title: String,
    pub steps: Vec<ChangeStepDraft>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ApprovalState {
    Draft,
    Approved,
    Rejected,
    Invalidated,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ExecutionState {
    NotStarted,
    Executing,
    Committed,
    Failed,
    RolledBack,
    RollbackFailed,
    Interrupted,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ChangeStepState {
    Pending,
    Succeeded,
    Failed,
    RolledBack,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ChangeSetRecoveryState {
    Live,
    MetadataOnly,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangeStep {
    pub id: Uuid,
    pub order: u32,
    pub tool_name: &'static str,
    pub risk: RiskLevel,
    pub preview: String,
    pub verification_plan_code: &'static str,
    pub rollback_capability: &'static str,
    pub state: ChangeStepState,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangeSet {
    pub id: Uuid,
    pub agent_run_id: Uuid,
    pub session_id: Uuid,
    pub title: String,
    pub version: u64,
    pub risk: RiskLevel,
    pub steps: Vec<ChangeStep>,
    pub approval_state: ApprovalState,
    pub approved_version: Option<u64>,
    pub execution_state: ExecutionState,
    pub recovery_state: ChangeSetRecoveryState,
    pub preconditions: Vec<ChangePrecondition>,
    pub policy_evaluation: Option<PolicyEvaluation>,
    pub policy_snapshot: Option<PolicySnapshot>,
    pub approved_step_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangePrecondition {
    pub step_id: Uuid,
    pub target_id: Uuid,
    pub check_tool: &'static str,
    pub observed_at_epoch_ms: u64,
    pub state: &'static str,
}

#[derive(Clone)]
struct PreconditionBinding {
    invocation: NativeToolInvocation,
    digest: [u8; 32],
}

#[derive(Clone)]
struct StoredChangeSet {
    public: ChangeSet,
    payloads: Vec<ChangeStepDraft>,
    precondition_bindings: Vec<PreconditionBinding>,
    policy_binding: Option<ChangePolicyBinding>,
}

#[derive(Clone)]
struct ChangePolicyBinding {
    target: PolicyTarget,
    target_count: usize,
    execution_strategy: PolicyExecutionStrategy,
    source: PolicyInvocationSource,
}

#[derive(Clone)]
pub(crate) struct PolicyCheckContext {
    pub target: PolicyTarget,
    pub target_count: usize,
    pub execution_strategy: PolicyExecutionStrategy,
    pub source: PolicyInvocationSource,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedChangeStep {
    id: Uuid,
    order: u32,
    tool_name: String,
    risk: RiskLevel,
    verification_plan_code: String,
    rollback_capability: String,
    state: ChangeStepState,
    error_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedChangeSet {
    schema_version: u8,
    id: Uuid,
    agent_run_id: Uuid,
    session_id: Uuid,
    version: u64,
    risk: RiskLevel,
    steps: Vec<PersistedChangeStep>,
    execution_state: ExecutionState,
    #[serde(default)]
    policy_snapshot: Option<PolicySnapshot>,
    #[serde(default)]
    approved_step_ids: Vec<Uuid>,
}

#[derive(Clone)]
pub(crate) struct ChangeSetService {
    items: Arc<Mutex<HashMap<Uuid, StoredChangeSet>>>,
    repository: Option<JsonRepository<Vec<PersistedChangeSet>>>,
}

impl Default for ChangeSetService {
    fn default() -> Self {
        Self {
            items: Arc::new(Mutex::new(HashMap::new())),
            repository: None,
        }
    }
}

impl ChangeSetService {
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
        let persisted = repository.load_or_default().await?;
        let mut items = self.items.lock().await;
        items.clear();
        for record in persisted
            .into_iter()
            .filter(|record| record.schema_version == CHANGE_SET_SCHEMA_VERSION)
            .take(MAX_PERSISTED_CHANGE_SETS)
        {
            let public = recovered_change_set(record)?;
            items.insert(
                public.id,
                StoredChangeSet {
                    public,
                    // Write payloads may contain file contents and intentionally never cross a
                    // process boundary. Recovery is metadata-only and cannot be resumed.
                    payloads: Vec::new(),
                    precondition_bindings: Vec::new(),
                    policy_binding: None,
                },
            );
        }
        self.persist_locked(&items).await
    }

    pub(crate) async fn draft(&self, request: ChangeSetDraftRequest) -> AppResult<ChangeSet> {
        validate_draft(&request)?;
        let id = Uuid::new_v4();
        let steps = request
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| public_step(index, step))
            .collect::<Vec<_>>();
        let risk = steps
            .iter()
            .map(|step| step.risk)
            .max()
            .unwrap_or(RiskLevel::R2);
        let change_set = ChangeSet {
            id,
            agent_run_id: request.agent_run_id,
            session_id: request.session_id,
            title: request.title,
            version: 1,
            risk,
            steps,
            approval_state: ApprovalState::Draft,
            approved_version: None,
            execution_state: ExecutionState::NotStarted,
            recovery_state: ChangeSetRecoveryState::Live,
            preconditions: Vec::new(),
            policy_evaluation: None,
            policy_snapshot: None,
            approved_step_ids: Vec::new(),
        };
        self.items.lock().await.insert(
            id,
            StoredChangeSet {
                public: change_set.clone(),
                payloads: request.steps,
                precondition_bindings: Vec::new(),
                policy_binding: None,
            },
        );
        let items = self.items.lock().await;
        if let Err(error) = self.persist_locked(&items).await {
            drop(items);
            self.items.lock().await.remove(&id);
            return Err(error);
        }
        Ok(change_set)
    }

    pub(crate) async fn get(&self, id: Uuid) -> AppResult<ChangeSet> {
        self.items
            .lock()
            .await
            .get(&id)
            .map(|item| item.public.clone())
            .ok_or(AppError::InvalidOperation)
    }

    pub(crate) async fn list(&self) -> AppResult<Vec<ChangeSet>> {
        let mut items = self
            .items
            .lock()
            .await
            .values()
            .map(|item| item.public.clone())
            .collect::<Vec<_>>();
        items.sort_by_key(|item| item.id);
        Ok(items)
    }

    pub(crate) async fn revise(
        &self,
        id: Uuid,
        request: ChangeSetDraftRequest,
    ) -> AppResult<ChangeSet> {
        validate_draft(&request)?;
        let mut items = self.items.lock().await;
        let stored = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        let previous = stored.clone();
        if stored.public.execution_state != ExecutionState::NotStarted
            || stored.public.recovery_state != ChangeSetRecoveryState::Live
        {
            return Err(AppError::InvalidOperation);
        }
        stored.public.version = stored.public.version.saturating_add(1);
        stored.public.agent_run_id = request.agent_run_id;
        stored.public.session_id = request.session_id;
        stored.public.title = request.title;
        stored.public.steps = request
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| public_step(index, step))
            .collect();
        stored.public.risk = stored
            .public
            .steps
            .iter()
            .map(|step| step.risk)
            .max()
            .unwrap_or(RiskLevel::R2);
        stored.public.approval_state = ApprovalState::Invalidated;
        stored.public.approved_version = None;
        stored.payloads = request.steps;
        stored.public.preconditions.clear();
        stored.precondition_bindings.clear();
        stored.public.policy_evaluation = None;
        stored.public.policy_snapshot = None;
        stored.public.approved_step_ids.clear();
        stored.policy_binding = None;
        let output = stored.public.clone();
        if let Err(error) = self.persist_locked(&items).await {
            items.insert(id, previous);
            return Err(error);
        }
        Ok(output)
    }

    pub(crate) async fn approve(&self, id: Uuid, version: u64) -> AppResult<ChangeSet> {
        let mut items = self.items.lock().await;
        let stored = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        let previous = stored.clone();
        if stored.public.version != version
            || stored.public.execution_state != ExecutionState::NotStarted
            || stored.public.recovery_state != ChangeSetRecoveryState::Live
            || (!cfg!(test)
                && (stored
                    .public
                    .policy_evaluation
                    .as_ref()
                    .is_none_or(|evaluation| !evaluation.permits_execution())
                    || stored.public.policy_snapshot.is_none()))
            || stored
                .public
                .policy_evaluation
                .as_ref()
                .is_some_and(|evaluation| {
                    evaluation.decision == PolicyDecision::RequireStepApproval
                })
        {
            return Err(AppError::InvalidOperation);
        }
        stored.public.approval_state = ApprovalState::Approved;
        stored.public.approved_version = Some(version);
        let output = stored.public.clone();
        if let Err(error) = self.persist_locked(&items).await {
            items.insert(id, previous);
            return Err(error);
        }
        Ok(output)
    }

    pub(crate) async fn approve_step(
        &self,
        id: Uuid,
        version: u64,
        step_id: Uuid,
    ) -> AppResult<ChangeSet> {
        let mut items = self.items.lock().await;
        let stored = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        let previous = stored.clone();
        if stored.public.version != version
            || stored.public.execution_state != ExecutionState::NotStarted
            || stored.public.recovery_state != ChangeSetRecoveryState::Live
            || stored.public.policy_snapshot.is_none()
            || stored
                .public
                .policy_evaluation
                .as_ref()
                .is_none_or(|evaluation| evaluation.decision != PolicyDecision::RequireStepApproval)
            || !stored.public.steps.iter().any(|step| step.id == step_id)
        {
            return Err(AppError::InvalidOperation);
        }
        if !stored.public.approved_step_ids.contains(&step_id) {
            stored.public.approved_step_ids.push(step_id);
            stored.public.approved_step_ids.sort();
        }
        if stored.public.approved_step_ids.len() == stored.public.steps.len() {
            stored.public.approval_state = ApprovalState::Approved;
            stored.public.approved_version = Some(version);
        }
        let output = stored.public.clone();
        if let Err(error) = self.persist_locked(&items).await {
            items.insert(id, previous);
            return Err(error);
        }
        Ok(output)
    }

    pub(crate) async fn check_policy(
        &self,
        id: Uuid,
        version: u64,
        policies: &AgentPolicyService,
        context: PolicyCheckContext,
    ) -> AppResult<ChangeSet> {
        let (public, payloads) = {
            let items = self.items.lock().await;
            let stored = items.get(&id).ok_or(AppError::InvalidOperation)?;
            if stored.public.version != version
                || stored.public.recovery_state != ChangeSetRecoveryState::Live
                || context.target_count == 0
            {
                return Err(AppError::InvalidOperation);
            }
            (stored.public.clone(), stored.payloads.clone())
        };
        let mut evaluations = Vec::with_capacity(payloads.len());
        for (index, payload) in payloads.iter().cloned().enumerate() {
            let tool = invocation(payload).name();
            evaluations.push(
                policies
                    .evaluate(&PolicyEvaluationRequest {
                        target: context.target.clone(),
                        tool,
                        risk_level: public.steps[index].risk,
                        resource_impact: crate::tools::resource_impact_for(tool),
                        target_count: context.target_count,
                        execution_strategy: context.execution_strategy,
                        source: context.source,
                    })
                    .await?,
            );
        }
        let evaluation = combine_policy_evaluations(evaluations)?;
        let snapshot = PolicySnapshot::from(&evaluation);
        let mut items = self.items.lock().await;
        let stored = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        if stored.public.version != version {
            return Err(AppError::InvalidOperation);
        }
        stored.public.policy_evaluation = Some(evaluation);
        stored.public.policy_snapshot = Some(snapshot);
        stored.policy_binding = Some(ChangePolicyBinding {
            target: context.target,
            target_count: context.target_count,
            execution_strategy: context.execution_strategy,
            source: context.source,
        });
        if stored
            .public
            .policy_evaluation
            .as_ref()
            .is_some_and(|evaluation| !evaluation.permits_execution())
        {
            stored.public.approval_state = ApprovalState::Invalidated;
            stored.public.approved_version = None;
        }
        let output = stored.public.clone();
        self.persist_locked(&items).await?;
        Ok(output)
    }

    pub(crate) async fn approve_with_preconditions(
        &self,
        id: Uuid,
        version: u64,
        sessions: &ServerSessionManager,
        tools: &NativeToolExecutionService,
    ) -> AppResult<ChangeSet> {
        self.capture_preconditions(id, version, sessions, tools)
            .await?;
        self.approve(id, version).await
    }

    pub(crate) async fn capture_preconditions(
        &self,
        id: Uuid,
        version: u64,
        sessions: &ServerSessionManager,
        tools: &NativeToolExecutionService,
    ) -> AppResult<ChangeSet> {
        let (public, payloads) = {
            let items = self.items.lock().await;
            let stored = items.get(&id).ok_or(AppError::InvalidOperation)?;
            if stored.public.version != version
                || stored.public.execution_state != ExecutionState::NotStarted
                || stored.public.recovery_state != ChangeSetRecoveryState::Live
            {
                return Err(AppError::InvalidOperation);
            }
            (stored.public.clone(), stored.payloads.clone())
        };
        let mut bindings = Vec::with_capacity(payloads.len());
        let mut visible = Vec::with_capacity(payloads.len());
        for (index, payload) in payloads.iter().enumerate() {
            let check = precondition_invocation(payload);
            let result = tools
                .execute(
                    sessions,
                    NativeToolRequest::new(public.session_id, check.clone()),
                )
                .await?;
            if !result.success {
                return Err(AppError::InvalidOperation);
            }
            bindings.push(PreconditionBinding {
                invocation: check.clone(),
                digest: result_digest(&result)?,
            });
            visible.push(ChangePrecondition {
                step_id: public.steps[index].id,
                target_id: public.session_id,
                check_tool: check.name().as_str(),
                observed_at_epoch_ms: result.started_at_epoch_ms,
                state: "captured",
            });
        }
        let output = {
            let mut items = self.items.lock().await;
            let stored = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
            if stored.public.version != version
                || stored.public.execution_state != ExecutionState::NotStarted
            {
                return Err(AppError::InvalidOperation);
            }
            stored.precondition_bindings = bindings;
            stored.public.preconditions = visible;
            let output = stored.public.clone();
            self.persist_locked(&items).await?;
            output
        };
        Ok(output)
    }

    pub(crate) async fn reject(&self, id: Uuid, version: u64) -> AppResult<ChangeSet> {
        let mut items = self.items.lock().await;
        let stored = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        let previous = stored.clone();
        if stored.public.version != version
            || stored.public.execution_state != ExecutionState::NotStarted
            || stored.public.recovery_state != ChangeSetRecoveryState::Live
        {
            return Err(AppError::InvalidOperation);
        }
        stored.public.approval_state = ApprovalState::Rejected;
        stored.public.approved_version = None;
        let output = stored.public.clone();
        if let Err(error) = self.persist_locked(&items).await {
            items.insert(id, previous);
            return Err(error);
        }
        Ok(output)
    }

    #[cfg(test)]
    pub(crate) async fn execute(
        &self,
        id: Uuid,
        version: u64,
        sessions: &ServerSessionManager,
        tools: &NativeToolExecutionService,
    ) -> AppResult<ChangeSet> {
        let policies = AgentPolicyService::default();
        let current = self.get(id).await?;
        if current.policy_snapshot.is_none() {
            self.check_policy(
                id,
                version,
                &policies,
                PolicyCheckContext {
                    target: PolicyTarget {
                        server_id: current.session_id,
                        group_id: None,
                        environment: None,
                    },
                    target_count: 1,
                    execution_strategy: PolicyExecutionStrategy::Sequential,
                    source: PolicyInvocationSource::NativeTool,
                },
            )
            .await?;
        }
        self.execute_with_policy(id, version, sessions, tools, &policies)
            .await
    }

    pub(crate) async fn execute_with_policy(
        &self,
        id: Uuid,
        version: u64,
        sessions: &ServerSessionManager,
        tools: &NativeToolExecutionService,
        policies: &AgentPolicyService,
    ) -> AppResult<ChangeSet> {
        let (snapshot, binding) = {
            let items = self.items.lock().await;
            let stored = items.get(&id).ok_or(AppError::InvalidOperation)?;
            (
                stored.public.policy_snapshot.clone(),
                stored.policy_binding.clone(),
            )
        };
        let (Some(snapshot), Some(binding)) = (snapshot, binding) else {
            return Err(AppError::InvalidOperation);
        };
        if !policies.current_matches(&snapshot).await {
            self.invalidate_for_policy_change(id, version).await?;
            return Err(AppError::InvalidOperation);
        }
        let checked = self
            .check_policy(
                id,
                version,
                policies,
                PolicyCheckContext {
                    target: binding.target,
                    target_count: binding.target_count,
                    execution_strategy: binding.execution_strategy,
                    source: binding.source,
                },
            )
            .await?;
        if !checked
            .policy_evaluation
            .as_ref()
            .is_some_and(PolicyEvaluation::permits_execution)
        {
            return Err(AppError::InvalidOperation);
        }
        let (precondition_public, precondition_bindings) = {
            let items = self.items.lock().await;
            let stored = items.get(&id).ok_or(AppError::InvalidOperation)?;
            if !cfg!(test) && stored.precondition_bindings.len() != stored.payloads.len() {
                return Err(AppError::InvalidOperation);
            }
            (stored.public.clone(), stored.precondition_bindings.clone())
        };
        for binding in &precondition_bindings {
            let result = tools
                .execute(
                    sessions,
                    NativeToolRequest::new(
                        precondition_public.session_id,
                        binding.invocation.clone(),
                    ),
                )
                .await?;
            if !result.success || result_digest(&result)? != binding.digest {
                self.invalidate_for_precondition_change(id, version).await?;
                return Err(AppError::InvalidOperation);
            }
        }
        let (mut public, payloads) = {
            let mut items = self.items.lock().await;
            let stored = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
            let previous = stored.clone();
            if stored.public.version != version
                || stored.public.approved_version != Some(version)
                || stored.public.approval_state != ApprovalState::Approved
                || stored.public.execution_state != ExecutionState::NotStarted
                || stored.public.recovery_state != ChangeSetRecoveryState::Live
                || stored.payloads.len() != stored.public.steps.len()
            {
                return Err(AppError::InvalidOperation);
            }
            stored.public.execution_state = ExecutionState::Executing;
            // Durably claim execution before the first remote side effect. A second caller now
            // fails closed, and a process restart can report an interrupted transaction.
            let output = (stored.public.clone(), stored.payloads.clone());
            if let Err(error) = self.persist_locked(&items).await {
                items.insert(id, previous);
                return Err(error);
            }
            output
        };
        let authority = ToolExecutionAuthority {
            agent_run_id: public.agent_run_id,
            change_set_id: public.id,
            change_set_version: public.version,
            rollback: false,
        };
        let mut completed = Vec::<(usize, ChangeStepDraft)>::new();
        for (index, payload) in payloads.iter().cloned().enumerate() {
            let result = match tools
                .execute(
                    sessions,
                    NativeToolRequest::approved(
                        public.session_id,
                        invocation(payload.clone()),
                        authority,
                    ),
                )
                .await
            {
                Ok(result) => result,
                Err(error) => {
                    public.steps[index].state = ChangeStepState::Failed;
                    public.steps[index].error_code = Some(error.code().to_owned());
                    public.execution_state = ExecutionState::Failed;
                    rollback_files(&mut public, completed, sessions, tools, authority).await;
                    self.store_execution_snapshot(id, &public).await?;
                    break;
                }
            };
            if result.success {
                public.steps[index].state = ChangeStepState::Succeeded;
                completed.push((index, payload));
            } else {
                public.steps[index].state = ChangeStepState::Failed;
                public.steps[index].error_code = result.error_code.map(str::to_owned);
                public.execution_state = ExecutionState::Failed;
                rollback_files(&mut public, completed, sessions, tools, authority).await;
                break;
            }
            self.store_execution_snapshot(id, &public).await?;
        }
        if public.execution_state == ExecutionState::Executing {
            public.execution_state = ExecutionState::Committed;
        }
        self.store_execution_snapshot(id, &public).await?;
        Ok(public)
    }

    pub(crate) async fn verify_execution(
        &self,
        id: Uuid,
        version: u64,
        sessions: &ServerSessionManager,
        tools: &NativeToolExecutionService,
    ) -> AppResult<bool> {
        let (public, payloads) = {
            let items = self.items.lock().await;
            let stored = items.get(&id).ok_or(AppError::InvalidOperation)?;
            if stored.public.version != version {
                return Err(AppError::InvalidOperation);
            }
            (stored.public.clone(), stored.payloads.clone())
        };
        if public.execution_state != ExecutionState::Committed {
            return Ok(false);
        }
        for (index, step) in public.steps.iter().enumerate() {
            if step.state != ChangeStepState::Succeeded {
                return Ok(false);
            }
            let payload = payloads.get(index).ok_or(AppError::InvalidOperation)?;
            let result = tools
                .execute(
                    sessions,
                    NativeToolRequest::new(public.session_id, verification_invocation(payload)),
                )
                .await?;
            if !result.success
                || !verification_plan_satisfied(step.verification_plan_code, &result)?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn invalidate_for_precondition_change(&self, id: Uuid, version: u64) -> AppResult<()> {
        let mut items = self.items.lock().await;
        let stored = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        if stored.public.version != version {
            return Err(AppError::InvalidOperation);
        }
        stored.public.approval_state = ApprovalState::Invalidated;
        stored.public.approved_version = None;
        for precondition in &mut stored.public.preconditions {
            precondition.state = "changed";
        }
        self.persist_locked(&items).await
    }

    async fn invalidate_for_policy_change(&self, id: Uuid, version: u64) -> AppResult<()> {
        let mut items = self.items.lock().await;
        let stored = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        if stored.public.version != version {
            return Err(AppError::InvalidOperation);
        }
        stored.public.approval_state = ApprovalState::Invalidated;
        stored.public.approved_version = None;
        self.persist_locked(&items).await
    }

    pub(crate) async fn rollback(
        &self,
        id: Uuid,
        version: u64,
        sessions: &ServerSessionManager,
        tools: &NativeToolExecutionService,
    ) -> AppResult<ChangeSet> {
        let (mut public, payloads) = {
            let items = self.items.lock().await;
            let stored = items.get(&id).ok_or(AppError::InvalidOperation)?;
            if stored.public.version != version
                || stored.public.recovery_state != ChangeSetRecoveryState::Live
                || !matches!(
                    stored.public.execution_state,
                    ExecutionState::Committed | ExecutionState::Failed
                )
                || stored.payloads.len() != stored.public.steps.len()
            {
                return Err(AppError::InvalidOperation);
            }
            (stored.public.clone(), stored.payloads.clone())
        };
        let authority = ToolExecutionAuthority {
            agent_run_id: public.agent_run_id,
            change_set_id: public.id,
            change_set_version: public.version,
            rollback: true,
        };
        let completed = payloads
            .into_iter()
            .enumerate()
            .filter(|(index, _)| public.steps[*index].state == ChangeStepState::Succeeded)
            .collect::<Vec<_>>();
        rollback_files(&mut public, completed, sessions, tools, authority).await;
        if !matches!(
            public.execution_state,
            ExecutionState::RolledBack | ExecutionState::RollbackFailed
        ) {
            public.execution_state = ExecutionState::RollbackFailed;
        }
        self.store_execution_snapshot(id, &public).await?;
        Ok(public)
    }

    async fn store_execution_snapshot(&self, id: Uuid, public: &ChangeSet) -> AppResult<()> {
        let mut items = self.items.lock().await;
        items.get_mut(&id).ok_or(AppError::InvalidOperation)?.public = public.clone();
        self.persist_locked(&items).await
    }

    async fn persist_locked(&self, items: &HashMap<Uuid, StoredChangeSet>) -> AppResult<()> {
        let Some(repository) = &self.repository else {
            return Ok(());
        };
        let mut records = items.values().map(persisted_change_set).collect::<Vec<_>>();
        records.sort_by_key(|record| record.id);
        if records.len() > MAX_PERSISTED_CHANGE_SETS {
            records.drain(..records.len() - MAX_PERSISTED_CHANGE_SETS);
        }
        repository.save_atomic(&records).await
    }
}

fn persisted_change_set(stored: &StoredChangeSet) -> PersistedChangeSet {
    PersistedChangeSet {
        schema_version: CHANGE_SET_SCHEMA_VERSION,
        id: stored.public.id,
        agent_run_id: stored.public.agent_run_id,
        session_id: stored.public.session_id,
        version: stored.public.version,
        risk: stored.public.risk,
        steps: stored
            .public
            .steps
            .iter()
            .map(|step| PersistedChangeStep {
                id: step.id,
                order: step.order,
                tool_name: step.tool_name.to_owned(),
                risk: step.risk,
                verification_plan_code: step.verification_plan_code.to_owned(),
                rollback_capability: step.rollback_capability.to_owned(),
                state: step.state,
                error_code: step.error_code.clone(),
            })
            .collect(),
        execution_state: stored.public.execution_state,
        policy_snapshot: stored.public.policy_snapshot.clone(),
        approved_step_ids: stored.public.approved_step_ids.clone(),
    }
}

fn recovered_change_set(record: PersistedChangeSet) -> AppResult<ChangeSet> {
    let execution_state = if record.execution_state == ExecutionState::Executing {
        ExecutionState::Interrupted
    } else {
        record.execution_state
    };
    let steps = record
        .steps
        .into_iter()
        .map(|step| {
            Ok(ChangeStep {
                id: step.id,
                order: step.order,
                tool_name: persisted_tool_name(&step.tool_name)?,
                risk: step.risk,
                preview: "recovery-payload-unavailable".into(),
                verification_plan_code: persisted_verification_code(&step.verification_plan_code)?,
                rollback_capability: persisted_rollback_code(&step.rollback_capability)?,
                state: step.state,
                error_code: step.error_code.filter(|value| valid_stable_code(value)),
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(ChangeSet {
        id: record.id,
        agent_run_id: record.agent_run_id,
        session_id: record.session_id,
        title: "recovered-change-set".into(),
        version: record.version,
        risk: record.risk,
        steps,
        approval_state: ApprovalState::Invalidated,
        approved_version: None,
        execution_state,
        recovery_state: ChangeSetRecoveryState::MetadataOnly,
        preconditions: Vec::new(),
        policy_evaluation: None,
        policy_snapshot: record.policy_snapshot,
        approved_step_ids: record.approved_step_ids,
    })
}

fn combine_policy_evaluations(evaluations: Vec<PolicyEvaluation>) -> AppResult<PolicyEvaluation> {
    let first = evaluations.first().ok_or(AppError::InvalidOperation)?;
    if evaluations.iter().any(|evaluation| {
        evaluation.policy_version != first.policy_version
            || evaluation.policy_hash != first.policy_hash
    }) {
        return Err(AppError::InvalidOperation);
    }
    let decisive = evaluations
        .iter()
        .max_by_key(|evaluation| evaluation.decision.precedence())
        .ok_or(AppError::InvalidOperation)?;
    let mut matched_rules = evaluations
        .iter()
        .flat_map(|evaluation| evaluation.matched_rules.clone())
        .collect::<Vec<PolicyMatchedRule>>();
    matched_rules.sort_by(|left, right| left.rule_id.cmp(&right.rule_id));
    matched_rules.dedup_by(|left, right| left.rule_id == right.rule_id);
    Ok(PolicyEvaluation {
        decision: decisive.decision,
        matched_rules,
        reason: decisive.reason.clone(),
        scope: decisive.scope.clone(),
        policy_version: first.policy_version,
        policy_hash: first.policy_hash.clone(),
    })
}

fn persisted_tool_name(value: &str) -> AppResult<&'static str> {
    match value {
        "file.patch" => Ok("file.patch"),
        "service.restart" => Ok("service.restart"),
        "service.reload" => Ok("service.reload"),
        "nginx.reload" => Ok("nginx.reload"),
        _ => Err(AppError::Storage),
    }
}

fn persisted_verification_code(value: &str) -> AppResult<&'static str> {
    match value {
        "read-back-exact-content" => Ok("read-back-exact-content"),
        "service-active" => Ok("service-active"),
        "nginx-test-before-and-after" => Ok("nginx-test-before-and-after"),
        _ => Err(AppError::Storage),
    }
}

fn persisted_rollback_code(value: &str) -> AppResult<&'static str> {
    match value {
        "snapshot-and-reverse-patch" => Ok("snapshot-and-reverse-patch"),
        "not-supported" => Ok("not-supported"),
        _ => Err(AppError::Storage),
    }
}

fn valid_stable_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

async fn rollback_files(
    public: &mut ChangeSet,
    completed: Vec<(usize, ChangeStepDraft)>,
    sessions: &ServerSessionManager,
    tools: &NativeToolExecutionService,
    authority: ToolExecutionAuthority,
) {
    let mut attempted = false;
    let mut failed = completed
        .iter()
        .any(|(_, step)| !matches!(step, ChangeStepDraft::FilePatch { .. }));
    for (index, step) in completed.into_iter().rev() {
        if let ChangeStepDraft::FilePatch {
            path,
            expected,
            replacement,
        } = step
        {
            attempted = true;
            let rollback = NativeToolInvocation::FilePatch {
                path,
                expected: replacement,
                replacement: expected,
            };
            match tools
                .execute(
                    sessions,
                    NativeToolRequest::approved_rollback(public.session_id, rollback, authority),
                )
                .await
            {
                Ok(result) if result.success => {
                    public.steps[index].state = ChangeStepState::RolledBack
                }
                _ => failed = true,
            }
        }
    }
    if attempted {
        public.execution_state = if failed {
            ExecutionState::RollbackFailed
        } else {
            ExecutionState::RolledBack
        };
    }
}

fn invocation(step: ChangeStepDraft) -> NativeToolInvocation {
    match step {
        ChangeStepDraft::FilePatch {
            path,
            expected,
            replacement,
        } => NativeToolInvocation::FilePatch {
            path,
            expected,
            replacement,
        },
        ChangeStepDraft::ServiceRestart { service } => {
            NativeToolInvocation::ServiceRestart { service }
        }
        ChangeStepDraft::ServiceReload { service } => {
            NativeToolInvocation::ServiceReload { service }
        }
        ChangeStepDraft::NginxReload => NativeToolInvocation::NginxReload,
        ChangeStepDraft::DockerRestart { container } => {
            NativeToolInvocation::DockerRestart { container }
        }
    }
}

fn precondition_invocation(step: &ChangeStepDraft) -> NativeToolInvocation {
    match step {
        ChangeStepDraft::FilePatch { path, .. } => {
            NativeToolInvocation::FileInspect { path: path.clone() }
        }
        ChangeStepDraft::ServiceRestart { service }
        | ChangeStepDraft::ServiceReload { service } => NativeToolInvocation::ServiceStatus {
            service: service.clone(),
        },
        ChangeStepDraft::NginxReload => NativeToolInvocation::NginxTest,
        ChangeStepDraft::DockerRestart { container } => NativeToolInvocation::DockerInspect {
            container: container.clone(),
        },
    }
}

fn verification_invocation(step: &ChangeStepDraft) -> NativeToolInvocation {
    precondition_invocation(step)
}

fn verification_plan_satisfied(plan: &str, result: &crate::tools::ToolResult) -> AppResult<bool> {
    use crate::domain::ServiceStatus;
    use crate::tools::ToolData;
    Ok(match plan {
        "service-active" => matches!(
            result.data.as_ref(),
            Some(ToolData::ServiceStatus(data))
                if data.service.status == ServiceStatus::Active
        ),
        "nginx-test-before-and-after" => matches!(
            result.data.as_ref(),
            Some(ToolData::NginxTest(data)) if data.valid
        ),
        "container-running" => matches!(
            result.data.as_ref(),
            Some(ToolData::Diagnostic(data))
                if data.category == "docker-inspect"
                    && data.fields.get("running").and_then(|value| value.as_bool()) == Some(true)
        ),
        "read-back-exact-content" => result.success,
        _ => result.success,
    })
}

fn result_digest(result: &crate::tools::ToolResult) -> AppResult<[u8; 32]> {
    let stable = serde_json::json!({
        "tool": result.tool_name,
        "success": result.success,
        "data": &result.data,
        "errorCode": result.error_code,
    });
    let bytes = serde_json::to_vec(&stable).map_err(|_| AppError::InvalidOperation)?;
    let value = digest(&SHA256, &bytes);
    let mut output = [0u8; 32];
    output.copy_from_slice(value.as_ref());
    Ok(output)
}

fn public_step(index: usize, step: &ChangeStepDraft) -> ChangeStep {
    let (tool_name, risk, preview, verification, rollback) = match step {
        ChangeStepDraft::FilePatch {
            path,
            expected,
            replacement,
        } => (
            "file.patch",
            RiskLevel::R3,
            format!(
                "--- {path}\n+++ {path}\n-{}\n+{}",
                preview_line(expected),
                preview_line(replacement)
            ),
            "read-back-exact-content",
            "snapshot-and-reverse-patch",
        ),
        ChangeStepDraft::ServiceRestart { service } => (
            "service.restart",
            RiskLevel::R3,
            format!("restart {service}"),
            "service-active",
            "not-supported",
        ),
        ChangeStepDraft::ServiceReload { service } => (
            "service.reload",
            RiskLevel::R2,
            format!("reload {service}"),
            "service-active",
            "not-supported",
        ),
        ChangeStepDraft::NginxReload => (
            "nginx.reload",
            RiskLevel::R2,
            "reload nginx".into(),
            "nginx-test-before-and-after",
            "not-supported",
        ),
        ChangeStepDraft::DockerRestart { container } => (
            "docker.restart",
            RiskLevel::R3,
            format!("restart docker container {container}"),
            "container-running",
            "not-supported",
        ),
    };
    ChangeStep {
        id: Uuid::new_v4(),
        order: index as u32 + 1,
        tool_name,
        risk,
        preview,
        verification_plan_code: verification,
        rollback_capability: rollback,
        state: ChangeStepState::Pending,
        error_code: None,
    }
}

fn preview_line(value: &str) -> String {
    let first = value.lines().next().unwrap_or_default();
    if first.len() <= 160 {
        return first.into();
    }
    let mut boundary = 160;
    while !first.is_char_boundary(boundary) {
        boundary -= 1;
    }
    format!("{}…", &first[..boundary])
}

fn validate_draft(request: &ChangeSetDraftRequest) -> AppResult<()> {
    if request.title.trim().is_empty()
        || request.title.len() > 256
        || request.steps.is_empty()
        || request.steps.len() > MAX_STEPS
    {
        return Err(AppError::InvalidOperation);
    }
    for step in &request.steps {
        match step {
            ChangeStepDraft::FilePatch {
                path,
                expected,
                replacement,
            } if !path.is_empty()
                && path.len() <= 4096
                && !expected.is_empty()
                && expected != replacement
                && expected.len() <= MAX_PATCH_BYTES
                && replacement.len() <= MAX_PATCH_BYTES => {}
            ChangeStepDraft::ServiceRestart { service }
            | ChangeStepDraft::ServiceReload { service }
                if valid_service(service) => {}
            ChangeStepDraft::NginxReload => {}
            ChangeStepDraft::DockerRestart { container } if valid_service(container) => {}
            _ => return Err(AppError::InvalidOperation),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn revision_invalidates_approval() {
        let service = ChangeSetService::default();
        let request = ChangeSetDraftRequest {
            agent_run_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            title: "repair".into(),
            steps: vec![ChangeStepDraft::NginxReload],
        };
        let draft = service.draft(request).await.expect("draft");
        let approved = service.approve(draft.id, 1).await.expect("approve");
        assert_eq!(approved.approved_version, Some(1));
        let revised = service
            .revise(
                draft.id,
                ChangeSetDraftRequest {
                    agent_run_id: draft.agent_run_id,
                    session_id: draft.session_id,
                    title: "revised".into(),
                    steps: vec![ChangeStepDraft::NginxReload],
                },
            )
            .await
            .expect("revise");
        assert_eq!(revised.version, 2);
        assert_eq!(revised.approved_version, None);
        assert_eq!(revised.approval_state, ApprovalState::Invalidated);
    }

    #[tokio::test]
    async fn recovery_persists_only_metadata_and_invalidates_approval() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("change-sets.json");
        let service = ChangeSetService::at_path(path.clone());
        let secret_expected = "DATABASE_PASSWORD=before";
        let secret_replacement = "DATABASE_PASSWORD=after";
        let draft = service
            .draft(ChangeSetDraftRequest {
                agent_run_id: Uuid::new_v4(),
                session_id: Uuid::new_v4(),
                title: "contains sensitive operator context".into(),
                steps: vec![ChangeStepDraft::FilePatch {
                    path: "/etc/example.conf".into(),
                    expected: secret_expected.into(),
                    replacement: secret_replacement.into(),
                }],
            })
            .await
            .expect("draft");
        service
            .approve(draft.id, draft.version)
            .await
            .expect("approve");

        let persisted = tokio::fs::read_to_string(&path).await.expect("metadata");
        assert!(!persisted.contains(secret_expected));
        assert!(!persisted.contains(secret_replacement));
        assert!(!persisted.contains("sensitive operator context"));

        let recovered = ChangeSetService::at_path(path);
        recovered.load().await.expect("recover");
        let item = recovered.get(draft.id).await.expect("recovered item");
        assert_eq!(item.approval_state, ApprovalState::Invalidated);
        assert_eq!(item.approved_version, None);
        assert_eq!(item.recovery_state, ChangeSetRecoveryState::MetadataOnly);
        assert!(recovered.approve(item.id, item.version).await.is_err());
    }

    #[tokio::test]
    async fn recovery_marks_in_flight_execution_interrupted() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("change-sets.json");
        let service = ChangeSetService::at_path(path.clone());
        let draft = service
            .draft(ChangeSetDraftRequest {
                agent_run_id: Uuid::new_v4(),
                session_id: Uuid::new_v4(),
                title: "reload".into(),
                steps: vec![ChangeStepDraft::NginxReload],
            })
            .await
            .expect("draft");
        {
            let mut items = service.items.lock().await;
            items
                .get_mut(&draft.id)
                .expect("stored")
                .public
                .execution_state = ExecutionState::Executing;
            service.persist_locked(&items).await.expect("persist");
        }

        let recovered = ChangeSetService::at_path(path);
        recovered.load().await.expect("recover");
        let item = recovered.get(draft.id).await.expect("recovered item");
        assert_eq!(item.execution_state, ExecutionState::Interrupted);
        assert_eq!(item.recovery_state, ChangeSetRecoveryState::MetadataOnly);
    }

    #[tokio::test]
    async fn failed_metadata_write_does_not_leave_executable_memory_state() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let blocking_parent = directory.path().join("not-a-directory");
        tokio::fs::write(&blocking_parent, b"file")
            .await
            .expect("blocking file");
        let service = ChangeSetService::at_path(blocking_parent.join("change-sets.json"));
        let result = service
            .draft(ChangeSetDraftRequest {
                agent_run_id: Uuid::new_v4(),
                session_id: Uuid::new_v4(),
                title: "reload".into(),
                steps: vec![ChangeStepDraft::NginxReload],
            })
            .await;
        assert!(result.is_err());
        assert!(service.list().await.expect("list").is_empty());
    }

    #[test]
    fn precondition_digest_ignores_runtime_ids_but_detects_state_changes() {
        let make = |success| crate::tools::ToolResult {
            invocation_id: Uuid::new_v4(),
            tool_name: crate::tools::NativeToolName::NginxTest,
            success,
            summary: "nginx-tested",
            data: None,
            error_code: (!success).then_some("EXEC_FAILED"),
            warnings: Vec::new(),
            started_at_epoch_ms: Uuid::new_v4().as_u128() as u64,
            duration_ms: 4,
            truncated: false,
            cancelled: false,
            untrusted_remote_data: true,
        };
        let first = make(true);
        let second = make(true);
        let changed = make(false);
        assert_eq!(
            result_digest(&first).expect("digest"),
            result_digest(&second).expect("digest")
        );
        assert_ne!(
            result_digest(&first).expect("digest"),
            result_digest(&changed).expect("digest")
        );
    }
}
