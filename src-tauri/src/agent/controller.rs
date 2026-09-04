//! Runtime V2 `AgentController` — iterative Reason → Tool → Observation loop
//! with durable interrupt/resume (AR2-B read path, AR2-C approval/user input).
//!
//! Hard rules:
//! - Tool failure and rejected decisions become Observations; the run continues.
//! - `RequireApproval` pauses in `AwaitingApproval`; the pending action does not
//!   execute until binding validation passes on the same `run_id`.
//! - Rejection becomes an Observation; the run reasons again — it does not fail.
//! - `AskUser` parks in `AwaitingUser`; `resume_with_user_input` continues the
//!   same run. Approval never creates a new run.

use std::sync::Arc;
use std::time::Instant;

use thiserror::Error;
use tokio::sync::watch;
use uuid::Uuid;

use super::approval::{
    validate_pending_approval, validate_pending_command_approval, ApprovalInvalidationReason,
    ApprovalRequest, ApprovalRequestState,
};
#[cfg(test)]
use super::artifact::InMemoryArtifactStore;
use super::artifact::{
    artifact_reference, should_store_as_artifact, store_large_output, ArtifactStore,
};
use super::changeset::{
    ChangeSetExecutor, ChangeSetExecutorError, CHANGE_PROPOSAL_NOT_EVIDENCE_BOUND,
};
use super::checkpoint::{AgentCheckpoint, BudgetCheckpoint, PendingInterruptRef};
use super::command::CommandOutcome;
use super::decision::{
    validate_decision, CommandMutability, DecisionError, PreparedCommandProposal,
    ValidatedChangeProposal, ValidatedDecision, AGENT_DECISION_INVALID,
};
use super::dispatch::{ToolDispatcher, ToolOutcome};
use super::event::{AgentEvent, AgentEventEnvelope};
use super::facts::{extract_facts_from_result, WorkingFactSet};
use super::gate::{Authorization, AuthorizationGate, CommandAuthorization, PolicySnapshotMatcher};
use super::metrics::RunMetrics;
use super::reasoner::{BudgetStatus, Observation, Reasoner, ReasonerInput};
use super::repository::{
    AgentEventRepository, AgentRunStore, AgentStoreError, ApprovalStore, CheckpointStore,
    PendingToolCallStore,
};
use super::run::{now_epoch_ms, AgentRun};
use super::state::{AgentRunStateV2, AgentStateError};
use crate::agentic::context::{
    compact, redact_secrets, snapshot, user_context, AgentContextItem, ContextBudget, ContextFact,
    ContextFreshness, ContextSnapshot, ContextSource, ContextTrust,
};
use crate::agentic::{change_proposal_is_evidence_bound, Evidence, ExecutionState};
use crate::domain::SessionId;

/// Stable failure codes surfaced through `RunFailed` events.
pub const AGENT_BUDGET_EXCEEDED: &str = "AGENT_BUDGET_EXCEEDED";
pub const AGENT_TIME_BUDGET_EXCEEDED: &str = "AGENT_TIME_BUDGET_EXCEEDED";
pub const AGENT_GOAL_INVALID: &str = "AGENT_GOAL_INVALID";
pub const AGENT_NOT_AWAITING_USER: &str = "AGENT_NOT_AWAITING_USER";
pub const AGENT_NOT_AWAITING_APPROVAL: &str = "AGENT_NOT_AWAITING_APPROVAL";
pub const AGENT_INVALID_USER_INPUT: &str = "AGENT_INVALID_USER_INPUT";

const MAX_GOAL_BYTES: usize = 8 * 1024;
const OBSERVATION_CONTEXT_TTL_MS: u64 = 30_000;
const GENERIC_TOOL_FAILURE_CODE: &str = "TOOL_FAILED";
const COMMAND_EVENT_PREVIEW_CHARS: usize = 8 * 1024;

/// Budgets for one run. Deterministic Rust-side limits; the Reasoner sees
/// them but cannot change them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunBudget {
    pub max_reasoner_rounds: u32,
    pub max_tool_calls: u32,
    pub time_budget_ms: u64,
}

impl Default for RunBudget {
    fn default() -> Self {
        Self {
            max_reasoner_rounds: 50,
            max_tool_calls: 20,
            time_budget_ms: 5 * 60 * 1_000,
        }
    }
}

/// How a drive ended — terminal or a durable interrupt awaiting user action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    Completed {
        summary: String,
    },
    AwaitingUser {
        question: String,
    },
    AwaitingApproval {
        approval_id: Uuid,
        tool_call_id: Uuid,
    },
    Cancelled,
    Failed {
        error_code: &'static str,
    },
}

/// Shared persistence handles for one controller instance.
#[derive(Clone)]
pub struct AgentStores {
    pub runs: Arc<dyn AgentRunStore>,
    pub events: Arc<dyn AgentEventRepository>,
    pub approvals: Arc<dyn ApprovalStore>,
    pub checkpoints: Arc<dyn CheckpointStore>,
    pub pending_calls: Arc<dyn PendingToolCallStore>,
}

/// Runtime wiring for one controller (authorization, policy binding, budgets).
pub struct AgentControllerConfig {
    pub gate: Arc<dyn AuthorizationGate>,
    pub policy_matcher: Arc<dyn PolicySnapshotMatcher>,
    pub changesets: Arc<dyn ChangeSetExecutor>,
    pub artifacts: Arc<dyn ArtifactStore>,
    pub target_ids: Vec<Uuid>,
    pub session_id: Option<SessionId>,
    pub budget: RunBudget,
    pub cancellation: watch::Receiver<bool>,
}

/// Controller infrastructure error (store or state-machine violation).
/// Distinct from run failures, which are legitimate outcomes.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AgentControllerError {
    #[error(transparent)]
    Store(#[from] AgentStoreError),
    #[error(transparent)]
    State(#[from] AgentStateError),
    #[error("agent goal is empty or exceeds the allowed size")]
    InvalidGoal,
    #[error("command proposal requires a bound server session")]
    MissingSession,
    #[error("run is not awaiting user input")]
    NotAwaitingUser,
    #[error("run is not awaiting approval")]
    NotAwaitingApproval,
    #[error("user input is empty or exceeds the allowed size")]
    InvalidUserInput,
    #[error("approval not found")]
    ApprovalNotFound,
    #[error("run is already terminal")]
    AlreadyTerminal,
}

impl AgentControllerError {
    pub fn code(&self) -> &'static str {
        match self {
            AgentControllerError::Store(error) => error.code(),
            AgentControllerError::State(error) => error.code(),
            AgentControllerError::InvalidGoal => AGENT_GOAL_INVALID,
            AgentControllerError::MissingSession => "AGENT_SESSION_REQUIRED",
            AgentControllerError::NotAwaitingUser => AGENT_NOT_AWAITING_USER,
            AgentControllerError::NotAwaitingApproval => AGENT_NOT_AWAITING_APPROVAL,
            AgentControllerError::InvalidUserInput => AGENT_INVALID_USER_INPUT,
            AgentControllerError::ApprovalNotFound => super::repository::AGENT_APPROVAL_NOT_FOUND,
            AgentControllerError::AlreadyTerminal => "AGENT_ALREADY_TERMINAL",
        }
    }
}

/// Drives one `AgentRun` through the V2 loop until a terminal state or a
/// durable interrupt (`AwaitingUser` / `AwaitingApproval`).
pub struct AgentController<R: Reasoner, D: ToolDispatcher> {
    run: AgentRun,
    goal: String,
    reasoner: R,
    dispatcher: D,
    gate: Arc<dyn AuthorizationGate>,
    policy_matcher: Arc<dyn PolicySnapshotMatcher>,
    changesets: Arc<dyn ChangeSetExecutor>,
    artifacts: Arc<dyn ArtifactStore>,
    run_store: Arc<dyn AgentRunStore>,
    events: Arc<dyn AgentEventRepository>,
    approvals: Arc<dyn ApprovalStore>,
    checkpoints: Arc<dyn CheckpointStore>,
    pending_calls: Arc<dyn PendingToolCallStore>,
    target_ids: Vec<Uuid>,
    session_id: Option<SessionId>,
    budget: RunBudget,
    cancellation: watch::Receiver<bool>,
    observations: Vec<Observation>,
    evidence: Vec<Evidence>,
    working_facts: WorkingFactSet,
    artifact_refs: Vec<super::artifact::ArtifactReference>,
    metrics: RunMetrics,
    user_replies: Vec<String>,
    rounds_used: u32,
    tool_calls_used: u32,
    pending_command_verification: bool,
    pending_command_analysis: Option<Uuid>,
    loop_started_at: Option<Instant>,
}

impl<R: Reasoner, D: ToolDispatcher> AgentController<R, D> {
    /// Creates the run in `Created` state, persists it and emits
    /// `RunCreated` + `UserMessageAdded`. The goal is redacted before it
    /// enters events or model context.
    pub fn new(
        goal: &str,
        reasoner: R,
        dispatcher: D,
        stores: AgentStores,
        config: AgentControllerConfig,
    ) -> Result<Self, AgentControllerError> {
        let trimmed = goal.trim();
        if trimmed.is_empty() || trimmed.len() > MAX_GOAL_BYTES {
            return Err(AgentControllerError::InvalidGoal);
        }
        let (goal, _) = redact_secrets(trimmed);
        let run = AgentRun::new(Uuid::new_v4());
        let run_id = run.id();
        stores.runs.insert(run.clone())?;
        let mut controller = Self {
            run,
            goal,
            reasoner,
            dispatcher,
            gate: config.gate,
            policy_matcher: config.policy_matcher,
            changesets: config.changesets,
            artifacts: config.artifacts,
            run_store: stores.runs,
            events: stores.events,
            approvals: stores.approvals,
            checkpoints: stores.checkpoints,
            pending_calls: stores.pending_calls,
            target_ids: config.target_ids,
            session_id: config.session_id,
            budget: config.budget,
            cancellation: config.cancellation,
            observations: Vec::new(),
            evidence: Vec::new(),
            working_facts: WorkingFactSet::default(),
            artifact_refs: Vec::new(),
            metrics: RunMetrics::new(run_id),
            user_replies: Vec::new(),
            rounds_used: 0,
            tool_calls_used: 0,
            pending_command_verification: false,
            pending_command_analysis: None,
            loop_started_at: None,
        };
        controller.emit(AgentEvent::RunCreated)?;
        controller.emit(AgentEvent::UserMessageAdded {
            content: controller.goal.clone(),
        })?;
        Ok(controller)
    }

    /// Reattach to a persisted run without emitting lifecycle bootstrap events.
    pub fn attach(
        run: AgentRun,
        goal: String,
        reasoner: R,
        dispatcher: D,
        stores: AgentStores,
        config: AgentControllerConfig,
        observations: Vec<Observation>,
        user_replies: Vec<String>,
        rounds_used: u32,
        tool_calls_used: u32,
        elapsed_ms: u64,
    ) -> Result<Self, AgentControllerError> {
        let trimmed = goal.trim();
        if trimmed.is_empty() || trimmed.len() > MAX_GOAL_BYTES {
            return Err(AgentControllerError::InvalidGoal);
        }
        let (goal, _) = redact_secrets(trimmed);
        let started = Instant::now()
            .checked_sub(std::time::Duration::from_millis(elapsed_ms))
            .unwrap_or_else(Instant::now);
        let run_id = run.id();
        Ok(Self {
            run,
            goal,
            reasoner,
            dispatcher,
            gate: config.gate,
            policy_matcher: config.policy_matcher,
            changesets: config.changesets,
            artifacts: config.artifacts,
            run_store: stores.runs,
            events: stores.events,
            approvals: stores.approvals,
            checkpoints: stores.checkpoints,
            pending_calls: stores.pending_calls,
            target_ids: config.target_ids,
            session_id: config.session_id,
            budget: config.budget,
            cancellation: config.cancellation,
            observations,
            evidence: Vec::new(),
            working_facts: WorkingFactSet::default(),
            artifact_refs: Vec::new(),
            metrics: RunMetrics::new(run_id),
            user_replies,
            rounds_used,
            tool_calls_used,
            pending_command_verification: false,
            pending_command_analysis: None,
            loop_started_at: Some(started),
        })
    }

    /// Restores working facts from a checkpoint snapshot after attach.
    pub fn restore_facts_from_checkpoint(&mut self, checkpoint: &AgentCheckpoint) {
        if checkpoint.fact_snapshot.is_empty() || checkpoint.fact_snapshot == "{}" {
            return;
        }
        if let Ok(set) = WorkingFactSet::from_snapshot_json(&checkpoint.fact_snapshot) {
            self.working_facts = set;
        }
    }

    pub fn restore_pending_command_verification(&mut self, required: bool) {
        self.pending_command_verification = required;
    }

    #[cfg(test)]
    fn working_fact_count(&self) -> usize {
        self.working_facts.len()
    }

    #[cfg(test)]
    fn has_working_fact_key(&self, key: &str) -> bool {
        self.working_facts.all().iter().any(|fact| fact.key == key)
    }

    pub fn run_id(&self) -> Uuid {
        self.run.id()
    }

    pub fn state(&self) -> AgentRunStateV2 {
        self.run.state()
    }

    /// Content-free observability snapshot for this run.
    pub fn metrics(&self) -> &RunMetrics {
        &self.metrics
    }

    /// User-initiated pause from safe read/reason phases only.
    pub fn pause(&mut self) -> Result<(), AgentControllerError> {
        if !matches!(
            self.run.state(),
            AgentRunStateV2::Running
                | AgentRunStateV2::Reasoning
                | AgentRunStateV2::Acting
                | AgentRunStateV2::Observing
                | AgentRunStateV2::Diagnosed
                | AgentRunStateV2::PlanningChange
        ) {
            return Err(AgentControllerError::State(super::state::AgentStateError {
                from: self.run.state(),
                to: AgentRunStateV2::Paused,
            }));
        }
        self.transition(AgentRunStateV2::Paused)?;
        self.emit(AgentEvent::RunPaused)?;
        let started = self.loop_started_at.unwrap_or_else(Instant::now);
        self.save_checkpoint(None, started)?;
        Ok(())
    }

    /// Resume a user-paused or crash-recovered paused run on the same `run_id`.
    pub async fn resume(&mut self) -> Result<RunOutcome, AgentControllerError> {
        if self.run.state() != AgentRunStateV2::Paused {
            return Err(AgentControllerError::State(super::state::AgentStateError {
                from: self.run.state(),
                to: AgentRunStateV2::Running,
            }));
        }
        self.transition(AgentRunStateV2::Running)?;
        self.emit(AgentEvent::RunResumed)?;
        let started = self.loop_started_at.unwrap_or_else(Instant::now);
        self.drive_loop(started).await
    }

    /// Runs the loop until Completed / Failed / Cancelled or a durable
    /// interrupt. Resuming after an interrupt uses `resume_with_user_input`,
    /// `approve`, or `reject` on the same controller instance — never a new
    /// run id.
    pub async fn run_to_interrupt(&mut self) -> Result<RunOutcome, AgentControllerError> {
        if self.run.state() == AgentRunStateV2::Created {
            self.transition(AgentRunStateV2::Running)?;
            self.emit(AgentEvent::RunStarted)?;
        }
        let started = *self.loop_started_at.get_or_insert_with(Instant::now);
        self.drive_loop(started).await
    }

    /// Starts an explicitly requested turn in a finished conversation. Old
    /// approvals are invalidated; outstanding verification remains required.
    pub async fn continue_conversation(
        &mut self,
        answer: &str,
    ) -> Result<RunOutcome, AgentControllerError> {
        let trimmed = answer.trim();
        if trimmed.is_empty() || trimmed.len() > MAX_GOAL_BYTES {
            return Err(AgentControllerError::InvalidUserInput);
        }
        if !self.run.state().is_terminal() {
            return Err(AgentControllerError::NotAwaitingUser);
        }
        while let Some(mut approval) = self.approvals.pending_for_run(self.run.id())? {
            approval.decide(ApprovalRequestState::Invalidated);
            self.approvals.save(&approval)?;
            self.emit(AgentEvent::ApprovalInvalidated {
                approval_id: approval.id,
                reason_code: "APPROVAL_NEW_USER_TURN".into(),
            })?;
        }
        self.run.begin_user_turn()?;
        self.run_store.save(&self.run)?;
        self.rounds_used = 0;
        self.tool_calls_used = 0;
        let started = Instant::now();
        self.loop_started_at = Some(started);
        self.pending_command_analysis = None;
        for target in &self.target_ids {
            self.working_facts.invalidate_target(*target);
        }
        let (content, _) = redact_secrets(trimmed);
        self.emit(AgentEvent::UserMessageAdded {
            content: content.clone(),
        })?;
        self.user_replies.push(content.clone());
        self.observations.push(Observation {
            tool_call_id: None,
            tool_name: None,
            success: true,
            error_code: None,
            summary: format!("User replied: {content}"),
            detail: Some(content),
        });
        self.emit(AgentEvent::RunResumed)?;
        self.save_checkpoint(None, started)?;
        self.drive_loop(started).await
    }

    /// Continues a run parked in `AwaitingUser` after the user replies.
    pub async fn resume_with_user_input(
        &mut self,
        answer: &str,
    ) -> Result<RunOutcome, AgentControllerError> {
        if self.run.state() != AgentRunStateV2::AwaitingUser {
            return Err(AgentControllerError::NotAwaitingUser);
        }
        let trimmed = answer.trim();
        if trimmed.is_empty() || trimmed.len() > MAX_GOAL_BYTES {
            return Err(AgentControllerError::InvalidUserInput);
        }
        let (content, _) = redact_secrets(trimmed);
        self.emit(AgentEvent::UserMessageAdded {
            content: content.clone(),
        })?;
        self.emit(AgentEvent::UserInputReceived)?;
        self.user_replies.push(content.clone());
        self.observations.push(Observation {
            tool_call_id: None,
            tool_name: None,
            success: true,
            error_code: None,
            summary: format!("User replied: {content}"),
            detail: Some(content),
        });
        self.emit(AgentEvent::RunResumed)?;
        self.transition(AgentRunStateV2::Reasoning)?;
        let started = self.loop_started_at.expect("loop clock initialized");
        self.drive_loop(started).await
    }

    /// Grants a pending approval and executes the bound action on the same run.
    pub async fn approve(&mut self, approval_id: Uuid) -> Result<RunOutcome, AgentControllerError> {
        if self.run.state() != AgentRunStateV2::AwaitingApproval {
            return Err(AgentControllerError::NotAwaitingApproval);
        }
        let mut approval = self
            .approvals
            .get(approval_id)?
            .ok_or(AgentControllerError::ApprovalNotFound)?;
        if approval.is_change_set() {
            return self.approve_change_set(approval_id, &mut approval).await;
        }
        if approval.is_command() {
            return self.approve_command(approval_id, &mut approval).await;
        }
        let call = self
            .pending_calls
            .get(approval.tool_call_id)?
            .ok_or(AgentControllerError::ApprovalNotFound)?;
        let policy_matches = self
            .policy_matcher
            .matches(approval.policy_version, &approval.policy_hash)
            .await;
        if let Err(reason) =
            validate_pending_approval(&approval, &call, &self.target_ids, policy_matches)
        {
            return self
                .invalidate_approval_and_resume(&mut approval, reason, &call)
                .await;
        }
        approval.decide(ApprovalRequestState::Granted);
        self.approvals.save(&approval)?;
        self.emit(AgentEvent::ApprovalGranted { approval_id })?;
        self.pending_calls.remove(approval.tool_call_id)?;
        self.transition(AgentRunStateV2::Acting)?;
        self.emit(AgentEvent::ToolStarted {
            tool_call_id: call.tool_call_id,
        })?;
        let outcomes = self
            .dispatcher
            .execute_reads(self.run.id(), std::slice::from_ref(&call))
            .await;
        self.tool_calls_used = self.tool_calls_used.saturating_add(1);
        self.transition(AgentRunStateV2::Observing)?;
        for outcome in outcomes {
            self.record_outcome(outcome)?;
        }
        self.emit(AgentEvent::RunResumed)?;
        let started = self.loop_started_at.expect("loop clock initialized");
        self.drive_loop(started).await
    }

    /// Rejects a pending approval; rejection becomes an Observation and the
    /// run reasons again on the same `run_id`.
    pub async fn reject(&mut self, approval_id: Uuid) -> Result<RunOutcome, AgentControllerError> {
        if self.run.state() != AgentRunStateV2::AwaitingApproval {
            return Err(AgentControllerError::NotAwaitingApproval);
        }
        let mut approval = self
            .approvals
            .get(approval_id)?
            .ok_or(AgentControllerError::ApprovalNotFound)?;
        if approval.is_change_set() {
            return self.reject_change_set(approval_id, &mut approval).await;
        }
        if approval.is_command() {
            return self.reject_command(approval_id, &mut approval).await;
        }
        let call = self
            .pending_calls
            .get(approval.tool_call_id)?
            .ok_or(AgentControllerError::ApprovalNotFound)?;
        approval.decide(ApprovalRequestState::Rejected);
        self.approvals.save(&approval)?;
        self.pending_calls.remove(approval.tool_call_id)?;
        self.emit(AgentEvent::ApprovalRejected { approval_id })?;
        self.metrics.record_rejection();
        let summary = format!("User rejected {}", call.tool_name);
        self.emit(AgentEvent::ObservationAdded {
            summary: summary.clone(),
        })?;
        self.observations.push(Observation {
            tool_call_id: Some(call.tool_call_id),
            tool_name: Some(call.tool_name),
            success: false,
            error_code: None,
            summary,
            detail: None,
        });
        self.transition(AgentRunStateV2::Reasoning)?;
        self.emit(AgentEvent::RunResumed)?;
        let started = self.loop_started_at.expect("loop clock initialized");
        self.drive_loop(started).await
    }

    async fn request_command_approval(
        &mut self,
        command: PreparedCommandProposal,
        started: Instant,
    ) -> Result<Option<RunOutcome>, AgentControllerError> {
        self.emit(AgentEvent::ProgressUpdated {
            summary: command.reason_summary.clone(),
        })?;
        self.emit(AgentEvent::CommandProposed {
            command_id: command.command_id,
            command: command.command.clone(),
            reason: command.reason_summary.clone(),
            risk: command.risk.as_str().into(),
            mutability: command.mutability.as_str().into(),
        })?;

        let (policy_version, policy_hash) = match self.gate.authorize_command(&command).await {
            CommandAuthorization::RequireApproval {
                risk: _,
                policy_version,
                policy_hash,
            } => (policy_version, policy_hash),
            CommandAuthorization::Blocked { reason_code } => {
                let summary = format!("Command proposal blocked by policy ({reason_code})");
                self.emit(AgentEvent::ObservationAdded {
                    summary: summary.clone(),
                })?;
                self.observations.push(Observation {
                    tool_call_id: Some(command.command_id),
                    tool_name: Some("agent.command".into()),
                    success: false,
                    error_code: Some(reason_code),
                    summary,
                    detail: None,
                });
                self.trace_runtime(
                    "command_proposal",
                    "agent.command",
                    "deny",
                    "blocked",
                    AgentRunStateV2::Reasoning,
                );
                return Ok(None);
            }
        };

        let approval = ApprovalRequest::bind_command(
            self.run.id(),
            &command,
            self.target_ids.clone(),
            self.session_id
                .ok_or(AgentControllerError::MissingSession)?,
            policy_version,
            policy_hash,
        );
        let approval_id = approval.id;
        let command_id = command.command_id;
        self.pending_calls.put_command(command)?;
        self.approvals.insert(approval)?;
        self.emit(AgentEvent::CommandApprovalRequired {
            approval_id,
            command_id,
        })?;
        self.transition(AgentRunStateV2::AwaitingApproval)?;
        self.save_checkpoint(Some(PendingInterruptRef::Approval { approval_id }), started)?;
        self.trace_runtime(
            "command_proposal",
            "agent.command",
            "require_approval",
            "pending",
            AgentRunStateV2::AwaitingApproval,
        );
        Ok(Some(RunOutcome::AwaitingApproval {
            approval_id,
            tool_call_id: command_id,
        }))
    }

    async fn approve_command(
        &mut self,
        approval_id: Uuid,
        approval: &mut ApprovalRequest,
    ) -> Result<RunOutcome, AgentControllerError> {
        let command = self
            .pending_calls
            .get_command(approval.tool_call_id)?
            .ok_or(AgentControllerError::ApprovalNotFound)?;
        let policy_matches = self
            .policy_matcher
            .matches(approval.policy_version, &approval.policy_hash)
            .await;
        if let Err(reason) = validate_pending_command_approval(
            approval,
            &command,
            &self.target_ids,
            self.session_id
                .ok_or(AgentControllerError::MissingSession)?,
            policy_matches,
        ) {
            return self
                .invalidate_command_approval_and_resume(approval, reason, &command)
                .await;
        }

        approval.decide(ApprovalRequestState::Granted);
        self.approvals.save(approval)?;
        self.pending_calls.remove_command(command.command_id)?;
        self.emit(AgentEvent::ApprovalGranted { approval_id })?;
        self.transition(AgentRunStateV2::Acting)?;
        self.emit(AgentEvent::CommandStarted {
            command_id: command.command_id,
        })?;
        let outcome = self
            .dispatcher
            .execute_command(self.run.id(), &command)
            .await;
        self.tool_calls_used = self.tool_calls_used.saturating_add(1);
        self.transition(AgentRunStateV2::Observing)?;
        self.record_command_outcome(&command, outcome)?;
        self.emit(AgentEvent::RunResumed)?;
        let started = self.loop_started_at.expect("loop clock initialized");
        self.drive_loop(started).await
    }

    async fn reject_command(
        &mut self,
        approval_id: Uuid,
        approval: &mut ApprovalRequest,
    ) -> Result<RunOutcome, AgentControllerError> {
        let command = self
            .pending_calls
            .get_command(approval.tool_call_id)?
            .ok_or(AgentControllerError::ApprovalNotFound)?;
        approval.decide(ApprovalRequestState::Rejected);
        self.approvals.save(approval)?;
        self.pending_calls.remove_command(command.command_id)?;
        self.emit(AgentEvent::ApprovalRejected { approval_id })?;
        self.metrics.record_rejection();
        let summary = "User rejected the proposed command".to_owned();
        self.emit(AgentEvent::ObservationAdded {
            summary: summary.clone(),
        })?;
        self.observations.push(Observation {
            tool_call_id: Some(command.command_id),
            tool_name: Some("agent.command".into()),
            success: false,
            error_code: None,
            summary,
            detail: Some(format!("Rejected command: {}", command.command)),
        });
        self.transition(AgentRunStateV2::Reasoning)?;
        self.emit(AgentEvent::RunResumed)?;
        let started = self.loop_started_at.expect("loop clock initialized");
        self.drive_loop(started).await
    }

    async fn invalidate_command_approval_and_resume(
        &mut self,
        approval: &mut ApprovalRequest,
        reason: ApprovalInvalidationReason,
        command: &PreparedCommandProposal,
    ) -> Result<RunOutcome, AgentControllerError> {
        approval.decide(ApprovalRequestState::Invalidated);
        self.approvals.save(approval)?;
        self.pending_calls.remove_command(command.command_id)?;
        self.emit(AgentEvent::ApprovalInvalidated {
            approval_id: approval.id,
            reason_code: reason.reason_code().into(),
        })?;
        let summary = format!("Command approval invalidated ({})", reason.reason_code());
        self.emit(AgentEvent::ObservationAdded {
            summary: summary.clone(),
        })?;
        self.observations.push(Observation {
            tool_call_id: Some(command.command_id),
            tool_name: Some("agent.command".into()),
            success: false,
            error_code: Some(reason.reason_code().into()),
            summary,
            detail: None,
        });
        self.transition(AgentRunStateV2::Reasoning)?;
        self.emit(AgentEvent::RunResumed)?;
        let started = self.loop_started_at.expect("loop clock initialized");
        self.drive_loop(started).await
    }

    fn record_command_outcome(
        &mut self,
        command: &PreparedCommandProposal,
        outcome: CommandOutcome,
    ) -> Result<(), AgentControllerError> {
        match command.mutability {
            CommandMutability::Mutating | CommandMutability::Unknown => {
                self.pending_command_verification = true;
            }
            CommandMutability::ReadIntent if outcome.success => {
                self.pending_command_verification = false;
            }
            CommandMutability::ReadIntent => {}
        }
        let elapsed = self.loop_started_at.map(elapsed_ms).unwrap_or(0);
        if outcome.success {
            self.metrics.maybe_record_first_observation(elapsed);
            self.emit(AgentEvent::CommandCompleted {
                command_id: command.command_id,
                exit_code: outcome.exit_code,
                output_preview: command_output_preview(&outcome),
                duration_ms: outcome.duration_ms,
            })?;
        } else {
            self.metrics.record_tool_failure();
            self.metrics.record_recovery_attempt();
            self.emit(AgentEvent::CommandFailed {
                command_id: command.command_id,
                exit_code: outcome.exit_code,
                output_preview: command_output_preview(&outcome),
                error_code: outcome
                    .error_code
                    .clone()
                    .unwrap_or_else(|| "COMMAND_FAILED".into()),
                duration_ms: outcome.duration_ms,
            })?;
        }

        let exit = outcome
            .exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "unavailable".into());
        let summary = if outcome.success {
            format!("Command completed with exit code {exit}")
        } else {
            format!("Command failed with exit code {exit}")
        };
        let detail = format!(
            "Command: {}\nExit code: {exit}\nstdout:\n{}\nstderr:\n{}",
            command.command, outcome.stdout, outcome.stderr
        );
        self.observations.push(Observation {
            tool_call_id: Some(command.command_id),
            tool_name: Some("agent.command".into()),
            success: outcome.success,
            error_code: outcome.error_code.clone(),
            summary,
            detail: Some(detail),
        });
        self.pending_command_analysis = Some(command.command_id);
        self.trace_runtime(
            "command_result",
            "agent.command",
            "approved",
            if outcome.success { "success" } else { "failed" },
            AgentRunStateV2::Reasoning,
        );
        Ok(())
    }

    /// Cancels a non-terminal run, including while awaiting approval or user
    /// input. Does not create a new run.
    pub fn cancel(&mut self) -> Result<RunOutcome, AgentControllerError> {
        if self.run.state().is_terminal() {
            return Err(AgentControllerError::AlreadyTerminal);
        }
        self.finish_cancelled()
    }

    async fn drive_loop(&mut self, started: Instant) -> Result<RunOutcome, AgentControllerError> {
        loop {
            if self.is_cancelled() {
                return self.finish_cancelled();
            }
            if elapsed_ms(started) >= self.budget.time_budget_ms {
                return self.fail(AGENT_TIME_BUDGET_EXCEEDED);
            }
            if self.rounds_used >= self.budget.max_reasoner_rounds {
                return self.fail(AGENT_BUDGET_EXCEEDED);
            }

            if self.run.state() != AgentRunStateV2::Reasoning {
                self.transition(AgentRunStateV2::Reasoning)?;
            }
            self.emit(AgentEvent::ReasoningStarted)?;
            self.rounds_used += 1;

            let (decision, context_tokens) = {
                let now = now_epoch_ms();
                let context = self.build_context(now);
                let tokens = context.estimated_tokens;
                let input = ReasonerInput {
                    run_id: self.run.id(),
                    session_id: self.session_id,
                    target_ids: &self.target_ids,
                    goal: &self.goal,
                    round: self.rounds_used,
                    observations: &self.observations,
                    user_replies: &self.user_replies,
                    budget: BudgetStatus {
                        rounds_used: self.rounds_used,
                        max_rounds: self.budget.max_reasoner_rounds,
                        tool_calls_used: self.tool_calls_used,
                        max_tool_calls: self.budget.max_tool_calls,
                        elapsed_ms: elapsed_ms(started),
                        time_budget_ms: self.budget.time_budget_ms,
                    },
                    context: &context,
                };
                (self.reasoner.decide(&input).await, tokens)
            };
            self.metrics.record_reasoner_round(context_tokens);
            let decision = match decision {
                Ok(decision) => decision,
                Err(error) => return self.fail_reasoner(error.code),
            };

            self.trace_runtime_decision(&decision);

            let validated = match validate_decision(decision) {
                Ok(validated) => validated,
                Err(error) => {
                    self.record_rejected_decision(error)?;
                    continue;
                }
            };

            self.emit_pending_command_analysis(&validated)?;

            match validated {
                ValidatedDecision::Final { summary } => {
                    if self.pending_command_verification {
                        let verification_summary = "A mutating or unknown command requires a successful follow-up verification command before completion.".to_string();
                        self.emit(AgentEvent::ObservationAdded {
                            summary: verification_summary.clone(),
                        })?;
                        self.observations.push(Observation {
                            tool_call_id: None,
                            tool_name: Some("agent.command.verification".into()),
                            success: false,
                            error_code: Some("COMMAND_VERIFICATION_REQUIRED".into()),
                            summary: verification_summary,
                            detail: None,
                        });
                        continue;
                    }
                    self.emit(AgentEvent::AssistantMessageAdded {
                        content: summary.clone(),
                    })?;
                    self.metrics.record_resolution(elapsed_ms(started));
                    self.metrics.maybe_record_diagnosis(elapsed_ms(started));
                    self.transition(AgentRunStateV2::Completed)?;
                    self.emit(AgentEvent::RunCompleted)?;
                    return Ok(RunOutcome::Completed { summary });
                }
                ValidatedDecision::AskUser { question } => {
                    self.emit(AgentEvent::UserInputRequired {
                        question: question.clone(),
                    })?;
                    self.transition(AgentRunStateV2::AwaitingUser)?;
                    self.save_checkpoint(
                        Some(PendingInterruptRef::UserInput {
                            question: question.clone(),
                        }),
                        started,
                    )?;
                    return Ok(RunOutcome::AwaitingUser { question });
                }
                ValidatedDecision::ToolCalls(calls) => {
                    let requested = u32::try_from(calls.len()).unwrap_or(u32::MAX);
                    if self.tool_calls_used.saturating_add(requested) > self.budget.max_tool_calls {
                        return self.fail(AGENT_BUDGET_EXCEEDED);
                    }
                    if let Some(outcome) = self.execute_authorized_calls(calls, started).await? {
                        return Ok(outcome);
                    }
                }
                ValidatedDecision::CommandProposal(command) => {
                    if self.tool_calls_used.saturating_add(1) > self.budget.max_tool_calls {
                        return self.fail(AGENT_BUDGET_EXCEEDED);
                    }
                    if let Some(outcome) = self.request_command_approval(command, started).await? {
                        return Ok(outcome);
                    }
                }
                ValidatedDecision::ProposeChangeSet(proposal) => {
                    if let Some(outcome) = self.propose_change_set(proposal, started).await? {
                        return Ok(outcome);
                    }
                }
            }
        }
    }

    fn emit_pending_command_analysis(
        &mut self,
        decision: &ValidatedDecision,
    ) -> Result<(), AgentControllerError> {
        let Some(command_id) = self.pending_command_analysis else {
            return Ok(());
        };
        let summary = match decision {
            ValidatedDecision::CommandProposal(command) => command
                .observation_analysis
                .as_deref()
                .unwrap_or(&command.reason_summary),
            ValidatedDecision::Final { summary } => summary,
            ValidatedDecision::AskUser { question } => question,
            ValidatedDecision::ToolCalls(calls) => calls
                .first()
                .map(|call| call.reason_summary.as_str())
                .unwrap_or("Continuing the investigation based on the command result."),
            ValidatedDecision::ProposeChangeSet(proposal) => &proposal.summary,
        };
        self.emit(AgentEvent::CommandAnalysisUpdated {
            command_id,
            summary: summary.to_owned(),
        })?;
        self.pending_command_analysis = None;
        Ok(())
    }

    async fn propose_change_set(
        &mut self,
        proposal: ValidatedChangeProposal,
        started: Instant,
    ) -> Result<Option<RunOutcome>, AgentControllerError> {
        if !change_proposal_is_evidence_bound(
            &self.goal,
            &self.evidence,
            &proposal.evidence_ids,
            &proposal.steps,
        ) {
            self.record_change_rejection(CHANGE_PROPOSAL_NOT_EVIDENCE_BOUND)?;
            return Ok(None);
        }
        self.transition(AgentRunStateV2::PlanningChange)?;
        self.emit(AgentEvent::ProgressUpdated {
            summary: proposal.summary.clone(),
        })?;
        let change_set = match self
            .changesets
            .draft_and_check_policy(
                self.run.id(),
                proposal.title.clone(),
                proposal.steps.clone(),
            )
            .await
        {
            Ok(change_set) => change_set,
            Err(error) => {
                self.record_change_rejection(error.code())?;
                self.transition(AgentRunStateV2::Reasoning)?;
                return Ok(None);
            }
        };
        let policy = change_set
            .policy_evaluation
            .as_ref()
            .ok_or(AgentControllerError::Store(AgentStoreError::StoreCorrupt))?;
        self.emit(AgentEvent::ChangeSetProposed {
            change_set_id: change_set.id,
        })?;
        self.emit(AgentEvent::ToolApprovalRequired {
            tool_call_id: proposal.proposal_id,
        })?;
        let approval = ApprovalRequest::bind_change_set(
            self.run.id(),
            proposal.proposal_id,
            change_set.id,
            change_set.version,
            &proposal.title,
            self.target_ids.clone(),
            format!("{:?}", change_set.risk),
            policy.policy_version,
            policy.policy_hash.clone(),
            change_set
                .preconditions
                .first()
                .map(|item| item.state.to_owned()),
        );
        self.approvals.insert(approval.clone())?;
        self.metrics.record_approval_interrupt();
        self.transition(AgentRunStateV2::AwaitingApproval)?;
        self.save_checkpoint(
            Some(PendingInterruptRef::Approval {
                approval_id: approval.id,
            }),
            started,
        )?;
        Ok(Some(RunOutcome::AwaitingApproval {
            approval_id: approval.id,
            tool_call_id: proposal.proposal_id,
        }))
    }

    async fn approve_change_set(
        &mut self,
        approval_id: Uuid,
        approval: &mut ApprovalRequest,
    ) -> Result<RunOutcome, AgentControllerError> {
        let change_set_id = approval
            .change_set_id
            .ok_or(AgentControllerError::ApprovalNotFound)?;
        let version = approval
            .change_set_version
            .ok_or(AgentControllerError::ApprovalNotFound)?;
        let policy_matches = self
            .policy_matcher
            .matches(approval.policy_version, &approval.policy_hash)
            .await;
        if let Err(reason) = super::approval::validate_pending_change_set_approval(
            approval,
            change_set_id,
            version,
            &self.target_ids,
            policy_matches,
        ) {
            return self
                .invalidate_change_set_approval(approval, reason, approval_id)
                .await;
        }
        approval.decide(ApprovalRequestState::Granted);
        self.approvals.save(approval)?;
        self.emit(AgentEvent::ApprovalGranted { approval_id })?;
        self.emit(AgentEvent::ChangeSetApproved {
            change_set_id,
            version,
        })?;
        self.changesets
            .approve(change_set_id, version)
            .await
            .map_err(|_| AgentControllerError::ApprovalNotFound)?;
        self.transition(AgentRunStateV2::ExecutingChange)?;
        self.emit(AgentEvent::ChangeSetExecutionStarted { change_set_id })?;
        let executed = self
            .changesets
            .execute(change_set_id, version)
            .await
            .map_err(|_| AgentControllerError::ApprovalNotFound)?;
        let execution_success = executed.execution_state == ExecutionState::Committed;
        self.emit(AgentEvent::ChangeSetExecutionCompleted {
            change_set_id,
            success: execution_success,
        })?;
        self.transition(AgentRunStateV2::Verifying)?;
        self.emit(AgentEvent::VerificationStarted)?;
        let verified = if execution_success {
            self.changesets
                .verify(change_set_id, version)
                .await
                .unwrap_or(false)
        } else {
            false
        };
        self.emit(AgentEvent::VerificationCompleted { success: verified })?;
        self.metrics.record_verification(verified);
        if !verified && execution_success {
            self.transition(AgentRunStateV2::RollingBack)?;
            self.emit(AgentEvent::RollbackStarted)?;
            let rollback_ok = self
                .changesets
                .rollback(change_set_id, version)
                .await
                .map(|change_set| {
                    matches!(
                        change_set.execution_state,
                        ExecutionState::RolledBack | ExecutionState::Committed
                    )
                })
                .unwrap_or(false);
            self.emit(AgentEvent::RollbackCompleted {
                success: rollback_ok,
            })?;
            self.metrics.record_rollback(rollback_ok);
        }
        let summary = if verified {
            format!("ChangeSet {change_set_id} verified successfully")
        } else {
            format!("ChangeSet {change_set_id} failed verification")
        };
        // Writes invalidate cached target facts — Verification must re-observe.
        for target_id in &self.target_ids {
            self.working_facts.invalidate_target(*target_id);
        }
        self.emit(AgentEvent::ObservationAdded {
            summary: summary.clone(),
        })?;
        self.observations.push(Observation {
            tool_call_id: None,
            tool_name: Some("change_set".into()),
            success: verified,
            error_code: if verified {
                None
            } else {
                Some(ChangeSetExecutorError::ExecutionFailed.code().to_owned())
            },
            summary,
            detail: None,
        });
        self.transition(AgentRunStateV2::Reasoning)?;
        self.emit(AgentEvent::RunResumed)?;
        let started = self.loop_started_at.expect("loop clock initialized");
        self.drive_loop(started).await
    }

    async fn reject_change_set(
        &mut self,
        approval_id: Uuid,
        approval: &mut ApprovalRequest,
    ) -> Result<RunOutcome, AgentControllerError> {
        approval.decide(ApprovalRequestState::Rejected);
        self.approvals.save(approval)?;
        self.emit(AgentEvent::ApprovalRejected { approval_id })?;
        let summary = format!(
            "User rejected ChangeSet {}",
            approval.change_set_id.unwrap_or_default()
        );
        self.emit(AgentEvent::ObservationAdded {
            summary: summary.clone(),
        })?;
        self.observations.push(Observation {
            tool_call_id: Some(approval.tool_call_id),
            tool_name: Some(approval.tool_name.clone()),
            success: false,
            error_code: None,
            summary,
            detail: None,
        });
        self.transition(AgentRunStateV2::Reasoning)?;
        self.emit(AgentEvent::RunResumed)?;
        let started = self.loop_started_at.expect("loop clock initialized");
        self.drive_loop(started).await
    }

    async fn invalidate_change_set_approval(
        &mut self,
        approval: &mut ApprovalRequest,
        reason: ApprovalInvalidationReason,
        approval_id: Uuid,
    ) -> Result<RunOutcome, AgentControllerError> {
        approval.decide(ApprovalRequestState::Invalidated);
        self.approvals.save(approval)?;
        self.emit(AgentEvent::ApprovalInvalidated {
            approval_id,
            reason_code: reason.reason_code().into(),
        })?;
        let summary = format!("ChangeSet approval invalidated ({})", reason.reason_code());
        self.emit(AgentEvent::ObservationAdded {
            summary: summary.clone(),
        })?;
        self.observations.push(Observation {
            tool_call_id: Some(approval.tool_call_id),
            tool_name: Some(approval.tool_name.clone()),
            success: false,
            error_code: Some(reason.reason_code().into()),
            summary,
            detail: None,
        });
        self.transition(AgentRunStateV2::Reasoning)?;
        self.emit(AgentEvent::RunResumed)?;
        let started = self.loop_started_at.expect("loop clock initialized");
        self.drive_loop(started).await
    }

    fn record_change_rejection(&mut self, code: &str) -> Result<(), AgentControllerError> {
        let summary = format!("Change proposal rejected ({code})");
        self.emit(AgentEvent::ObservationAdded {
            summary: summary.clone(),
        })?;
        self.observations.push(Observation {
            tool_call_id: None,
            tool_name: None,
            success: false,
            error_code: Some(code.into()),
            summary,
            detail: None,
        });
        Ok(())
    }

    async fn execute_authorized_calls(
        &mut self,
        calls: Vec<super::decision::PreparedToolCall>,
        started: Instant,
    ) -> Result<Option<RunOutcome>, AgentControllerError> {
        self.transition(AgentRunStateV2::Acting)?;
        let mut auto_calls = Vec::new();
        for call in calls {
            self.emit(AgentEvent::ProgressUpdated {
                summary: call.reason_summary.clone(),
            })?;
            self.emit(AgentEvent::ToolRequested {
                tool_call_id: call.tool_call_id,
                tool_name: call.tool_name.clone(),
            })?;
            match self.gate.authorize(&call).await {
                Authorization::Auto => {
                    self.trace_runtime(
                        "tool_calls",
                        &call.tool_name,
                        "allow",
                        "pending",
                        AgentRunStateV2::Acting,
                    );
                    self.emit(AgentEvent::ToolAutoAuthorized {
                        tool_call_id: call.tool_call_id,
                    })?;
                    self.metrics.record_tool_call(true);
                    auto_calls.push(call);
                }
                Authorization::RequireApproval {
                    risk,
                    policy_version,
                    policy_hash,
                } => {
                    self.trace_runtime(
                        "tool_calls",
                        &call.tool_name,
                        "require_approval",
                        "pending",
                        AgentRunStateV2::AwaitingApproval,
                    );
                    return self
                        .interrupt_for_approval(call, risk, policy_version, policy_hash, started)
                        .await
                        .map(Some);
                }
                Authorization::Blocked { reason_code } => {
                    self.trace_runtime(
                        "tool_calls",
                        &call.tool_name,
                        "deny",
                        "blocked",
                        AgentRunStateV2::Reasoning,
                    );
                    self.record_blocked_call(&call, &reason_code)?;
                }
            }
        }
        if auto_calls.is_empty() {
            return Ok(None);
        }
        for call in &auto_calls {
            self.emit(AgentEvent::ToolStarted {
                tool_call_id: call.tool_call_id,
            })?;
        }
        let executed = u32::try_from(auto_calls.len()).unwrap_or(u32::MAX);
        let outcomes = self
            .dispatcher
            .execute_reads(self.run.id(), &auto_calls)
            .await;
        self.tool_calls_used = self.tool_calls_used.saturating_add(executed);
        self.transition(AgentRunStateV2::Observing)?;
        for outcome in outcomes {
            self.record_outcome(outcome)?;
        }
        Ok(None)
    }

    async fn interrupt_for_approval(
        &mut self,
        call: super::decision::PreparedToolCall,
        risk: String,
        policy_version: u64,
        policy_hash: String,
        started: Instant,
    ) -> Result<RunOutcome, AgentControllerError> {
        self.emit(AgentEvent::ToolApprovalRequired {
            tool_call_id: call.tool_call_id,
        })?;
        let approval = ApprovalRequest::bind(
            self.run.id(),
            &call,
            self.target_ids.clone(),
            risk,
            policy_version,
            policy_hash,
        );
        self.approvals.insert(approval.clone())?;
        self.pending_calls.put(call.clone())?;
        self.metrics.record_approval_interrupt();
        self.transition(AgentRunStateV2::AwaitingApproval)?;
        self.save_checkpoint(
            Some(PendingInterruptRef::Approval {
                approval_id: approval.id,
            }),
            started,
        )?;
        Ok(RunOutcome::AwaitingApproval {
            approval_id: approval.id,
            tool_call_id: call.tool_call_id,
        })
    }

    async fn invalidate_approval_and_resume(
        &mut self,
        approval: &mut ApprovalRequest,
        reason: ApprovalInvalidationReason,
        call: &super::decision::PreparedToolCall,
    ) -> Result<RunOutcome, AgentControllerError> {
        approval.decide(ApprovalRequestState::Invalidated);
        self.approvals.save(approval)?;
        self.pending_calls.remove(approval.tool_call_id)?;
        self.emit(AgentEvent::ApprovalInvalidated {
            approval_id: approval.id,
            reason_code: reason.reason_code().into(),
        })?;
        self.metrics.record_recovery_attempt();
        let summary = format!(
            "Approval invalidated ({}) for {}",
            reason.reason_code(),
            call.tool_name
        );
        self.emit(AgentEvent::ObservationAdded {
            summary: summary.clone(),
        })?;
        self.observations.push(Observation {
            tool_call_id: Some(call.tool_call_id),
            tool_name: Some(call.tool_name.clone()),
            success: false,
            error_code: Some(reason.reason_code().into()),
            summary,
            detail: None,
        });
        self.transition(AgentRunStateV2::Reasoning)?;
        self.emit(AgentEvent::RunResumed)?;
        let started = self.loop_started_at.expect("loop clock initialized");
        self.drive_loop(started).await
    }

    fn record_blocked_call(
        &mut self,
        call: &super::decision::PreparedToolCall,
        reason_code: &str,
    ) -> Result<(), AgentControllerError> {
        let summary = format!("{} blocked by policy", call.tool_name);
        self.emit(AgentEvent::ObservationAdded {
            summary: summary.clone(),
        })?;
        self.observations.push(Observation {
            tool_call_id: Some(call.tool_call_id),
            tool_name: Some(call.tool_name.clone()),
            success: false,
            error_code: Some(reason_code.into()),
            summary,
            detail: None,
        });
        Ok(())
    }

    fn save_checkpoint(
        &mut self,
        pending_interrupt: Option<PendingInterruptRef>,
        started: Instant,
    ) -> Result<(), AgentControllerError> {
        let event_cursor = self.run.peek_next_event_seq().saturating_sub(1);
        self.checkpoints.save(AgentCheckpoint {
            run_id: self.run.id(),
            state: self.run.state(),
            event_cursor,
            pending_interrupt,
            budget: BudgetCheckpoint {
                rounds_used: self.rounds_used,
                tool_calls_used: self.tool_calls_used,
                elapsed_ms: elapsed_ms(started),
            },
            target_ids: self.target_ids.clone(),
            fact_snapshot: self
                .working_facts
                .snapshot_json()
                .unwrap_or_else(|_| "[]".into()),
            created_at_epoch_ms: now_epoch_ms(),
        })?;
        Ok(())
    }

    fn record_outcome(&mut self, outcome: ToolOutcome) -> Result<(), AgentControllerError> {
        let elapsed = self.loop_started_at.map(elapsed_ms).unwrap_or(0);
        if outcome.success {
            self.metrics.maybe_record_first_observation(elapsed);
        } else {
            self.metrics.record_tool_failure();
            self.metrics.record_recovery_attempt();
        }
        if outcome.success {
            self.emit(AgentEvent::ToolCompleted {
                tool_call_id: outcome.tool_call_id,
            })?;
        } else {
            self.emit(AgentEvent::ToolFailed {
                tool_call_id: outcome.tool_call_id,
                error_code: outcome
                    .error_code
                    .clone()
                    .unwrap_or_else(|| GENERIC_TOOL_FAILURE_CODE.into()),
            })?;
        }
        let status = if outcome.success {
            "succeeded"
        } else {
            "failed"
        };
        let summary = format!("{} {status}: {}", outcome.tool_name, outcome.summary);
        self.emit(AgentEvent::ObservationAdded {
            summary: summary.clone(),
        })?;

        let detail = self.maybe_store_artifact(&outcome)?;
        self.observations.push(Observation {
            tool_call_id: Some(outcome.tool_call_id),
            tool_name: Some(outcome.tool_name.clone()),
            success: outcome.success,
            error_code: outcome.error_code.clone(),
            summary,
            detail,
        });
        self.trace_runtime(
            "observation",
            &outcome.tool_name,
            "allow",
            status,
            AgentRunStateV2::Reasoning,
        );
        if let Some(result) = outcome.tool_result {
            let source_seq = self.run.peek_next_event_seq().saturating_sub(1);
            let target_id = self.target_ids.first().copied();
            let extracted = extract_facts_from_result(
                &result,
                outcome.tool_call_id,
                target_id,
                now_epoch_ms(),
                Some(source_seq),
            );
            let changed = self.working_facts.upsert_many(extracted);
            if !changed.is_empty() {
                self.emit(AgentEvent::FactsUpdated { fact_keys: changed })?;
            }
            self.evidence.push(Evidence {
                id: outcome.tool_call_id,
                source: format!("tool.{}", result.tool_name.as_str()),
                invocation_id: result.invocation_id,
                trust: ContextTrust::UntrustedRemoteData,
                summary: result.summary,
                result,
            });
        }
        Ok(())
    }

    fn maybe_store_artifact(
        &mut self,
        outcome: &ToolOutcome,
    ) -> Result<Option<String>, AgentControllerError> {
        let Some(raw) = outcome.sanitized_data.as_deref() else {
            return Ok(None);
        };
        if !should_store_as_artifact(raw) {
            return Ok(Some(raw.to_owned()));
        }
        let artifact = store_large_output(
            self.artifacts.as_ref(),
            self.run.id(),
            outcome.tool_call_id,
            &outcome.tool_name,
            &outcome.summary,
            raw,
            now_epoch_ms(),
        )
        .map_err(|_| AgentControllerError::Store(AgentStoreError::PersistenceFailed))?;
        let reference = artifact_reference(&artifact);
        let detail = format!(
            "artifact:{} bytes={} summary={}",
            reference.artifact_id, reference.byte_len, reference.summary
        );
        self.artifact_refs.push(reference);
        self.metrics.record_artifact_bytes(artifact.byte_len as u64);
        Ok(Some(detail))
    }

    fn record_rejected_decision(
        &mut self,
        error: DecisionError,
    ) -> Result<(), AgentControllerError> {
        let summary = format!("Decision rejected: {error}");
        self.emit(AgentEvent::ObservationAdded {
            summary: summary.clone(),
        })?;
        self.observations.push(Observation {
            tool_call_id: None,
            tool_name: None,
            success: false,
            error_code: Some(AGENT_DECISION_INVALID.into()),
            summary,
            detail: None,
        });
        Ok(())
    }

    /// Builds the per-round model context through the Phase 10K context
    /// manager: redacted goal, fresh working facts, recent observations, and
    /// artifact references under the standard item/byte/token budget.
    /// Large raw tool output never enters this snapshot.
    fn build_context(&self, now_epoch_ms: u64) -> ContextSnapshot {
        let mut items = vec![user_context(&self.goal, self.run.created_at_epoch_ms())];
        // Keep only a bounded recent observation window; older rounds are
        // represented by compact working facts after compaction.
        const RECENT_OBSERVATIONS: usize = 8;
        let start = self.observations.len().saturating_sub(RECENT_OBSERVATIONS);
        for observation in &self.observations[start..] {
            items.push(observation_item(observation, now_epoch_ms));
        }
        let mut snap = snapshot(items, ContextBudget::default(), now_epoch_ms);
        let facts: Vec<ContextFact> = self
            .working_facts
            .fresh(now_epoch_ms)
            .into_iter()
            .map(|fact| fact.to_context_fact())
            .collect();
        if !facts.is_empty() {
            // Compaction drops NativeTools observation items in favor of facts.
            compact(&mut snap, facts);
        }
        // Artifact references are attached after compaction so they survive.
        for reference in &self.artifact_refs {
            let content = format!(
                "Artifact {} ({} bytes): {}",
                reference.artifact_id, reference.byte_len, reference.summary
            );
            let estimated_tokens = u32::try_from(content.len() / 4).unwrap_or(u32::MAX).max(1);
            snap.total_bytes = snap.total_bytes.saturating_add(content.len());
            snap.estimated_tokens = snap.estimated_tokens.saturating_add(estimated_tokens);
            snap.items.push(AgentContextItem {
                id: reference.artifact_id,
                source: format!("artifact:{}", reference.tool_name),
                source_kind: ContextSource::Logs,
                trust: ContextTrust::UntrustedRemoteData,
                redacted: true,
                content,
                freshness: ContextFreshness {
                    observed_at_epoch_ms: now_epoch_ms,
                    ttl_ms: OBSERVATION_CONTEXT_TTL_MS,
                },
                estimated_tokens,
            });
        }
        // Guardrail: never allow a single context item to hold multi-kiloline dumps.
        debug_assert!(snap.items.iter().all(|item| item.content.len() < 16 * 1024));
        snap
    }

    fn is_cancelled(&self) -> bool {
        *self.cancellation.borrow()
    }

    fn finish_cancelled(&mut self) -> Result<RunOutcome, AgentControllerError> {
        self.transition(AgentRunStateV2::Cancelled)?;
        self.emit(AgentEvent::RunCancelled)?;
        Ok(RunOutcome::Cancelled)
    }

    fn fail(&mut self, error_code: &'static str) -> Result<RunOutcome, AgentControllerError> {
        self.transition(AgentRunStateV2::Failed)?;
        self.emit(AgentEvent::RunFailed {
            error_code: error_code.into(),
        })?;
        Ok(RunOutcome::Failed { error_code })
    }

    fn fail_reasoner(&mut self, code: String) -> Result<RunOutcome, AgentControllerError> {
        self.transition(AgentRunStateV2::Failed)?;
        self.emit(AgentEvent::RunFailed { error_code: code })?;
        Ok(RunOutcome::Failed {
            error_code: super::reasoner::AGENT_REASONER_FAILED,
        })
    }

    fn transition(&mut self, next: AgentRunStateV2) -> Result<(), AgentControllerError> {
        self.run.transition_to(next)?;
        self.run_store.save(&self.run)?;
        if next.is_terminal() {
            self.persist_metrics()?;
        }
        Ok(())
    }

    fn persist_metrics(&self) -> Result<(), AgentControllerError> {
        let metrics_json = serde_json::to_string(&self.metrics)
            .map_err(|_| AgentControllerError::Store(AgentStoreError::PersistenceFailed))?;
        self.run_store
            .save_metrics(self.run.id(), &metrics_json)
            .map_err(AgentControllerError::Store)
    }

    fn emit(&mut self, event: AgentEvent) -> Result<(), AgentControllerError> {
        let seq = self.run.next_event_sequence();
        self.events.append(AgentEventEnvelope {
            run_id: self.run.id(),
            seq,
            timestamp_epoch_ms: now_epoch_ms(),
            event,
        })?;
        // Persist the sequence cursor together with every emission.
        self.run_store.save(&self.run)?;
        Ok(())
    }

    fn trace_runtime_decision(&self, decision: &super::decision::AgentDecision) {
        let (decision_type, requested_tools) = match decision {
            super::decision::AgentDecision::ToolCalls(calls) => (
                "tool_calls",
                calls
                    .iter()
                    .map(|call| call.tool_name.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            super::decision::AgentDecision::CommandProposal(_) => {
                ("command_proposal", "agent.command".into())
            }
            super::decision::AgentDecision::AskUser { .. } => ("ask_user", String::new()),
            super::decision::AgentDecision::Final { .. } => ("final", String::new()),
            super::decision::AgentDecision::ProposeChangeSet(_) => {
                ("propose_change_set", String::new())
            }
        };
        let next_state = match decision {
            super::decision::AgentDecision::ToolCalls(_) => AgentRunStateV2::Acting,
            super::decision::AgentDecision::CommandProposal(_) => AgentRunStateV2::AwaitingApproval,
            super::decision::AgentDecision::AskUser { .. } => AgentRunStateV2::AwaitingUser,
            super::decision::AgentDecision::Final { .. } => AgentRunStateV2::Completed,
            super::decision::AgentDecision::ProposeChangeSet(_) => AgentRunStateV2::PlanningChange,
        };
        self.trace_runtime(
            decision_type,
            &requested_tools,
            "pending",
            "pending",
            next_state,
        );
    }

    #[cfg(debug_assertions)]
    fn trace_runtime(
        &self,
        decision_type: &str,
        requested_tools: &str,
        policy_decision: &str,
        tool_result_status: &str,
        next_state: AgentRunStateV2,
    ) {
        tracing::debug!(
            run_id = %self.run.id(),
            round = self.rounds_used,
            target_id = ?self.target_ids.first(),
            session_id = ?self.session_id,
            available_tool_count = 0,
            decision_type,
            requested_tools,
            policy_decision,
            tool_result_status,
            observation_count = self.observations.len(),
            next_state = ?next_state,
            "agent_runtime_v2_trace"
        );
    }

    #[cfg(not(debug_assertions))]
    fn trace_runtime(
        &self,
        _decision_type: &str,
        _requested_tools: &str,
        _policy_decision: &str,
        _tool_result_status: &str,
        _next_state: AgentRunStateV2,
    ) {
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn command_output_preview(outcome: &CommandOutcome) -> String {
    let source = match (outcome.stdout.trim(), outcome.stderr.trim()) {
        ("", "") => String::new(),
        (stdout, "") => stdout.to_owned(),
        ("", stderr) => stderr.to_owned(),
        (stdout, stderr) => format!("{stdout}\n{stderr}"),
    };
    let mut preview = source
        .chars()
        .take(COMMAND_EVENT_PREVIEW_CHARS)
        .collect::<String>();
    if source.chars().count() > COMMAND_EVENT_PREVIEW_CHARS {
        preview.push_str("\n…");
    }
    preview
}

fn observation_item(observation: &Observation, now_epoch_ms: u64) -> AgentContextItem {
    let mut content = observation.summary.clone();
    if let Some(detail) = &observation.detail {
        // Large payloads are already replaced by artifact refs in record_outcome.
        // Bound residual inline detail so context stays compact.
        let bounded = if detail.len() > 2_048 {
            let mut end = 2_048;
            while end > 0 && !detail.is_char_boundary(end) {
                end -= 1;
            }
            &detail[..end]
        } else {
            detail.as_str()
        };
        content.push('\n');
        content.push_str(bounded);
    }
    let estimated_tokens = u32::try_from(content.len() / 4).unwrap_or(u32::MAX).max(1);
    AgentContextItem {
        id: Uuid::new_v4(),
        source: observation
            .tool_name
            .as_deref()
            .map(|name| format!("tool:{name}"))
            .unwrap_or_else(|| "runtime:decision".into()),
        source_kind: ContextSource::NativeTools,
        trust: ContextTrust::UntrustedRemoteData,
        redacted: true,
        content,
        freshness: ContextFreshness {
            observed_at_epoch_ms: now_epoch_ms,
            ttl_ms: OBSERVATION_CONTEXT_TTL_MS,
        },
        estimated_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::changeset::{ChangeSetExecutor, ChangeSetExecutorError};
    use crate::agent::decision::ValidatedDecision;
    use crate::agent::decision::{
        AgentDecision, ChangeProposalRequest, PreparedToolCall, ToolCallRequest,
    };
    use crate::agent::gate::{AutoAuthorizationGate, FixedPolicyMatcher, FnAuthorizationGate};
    use crate::agent::reasoner::{ReasonerError, AGENT_REASONER_FAILED};
    use crate::agent::repository::{
        InMemoryAgentEventRepository, InMemoryAgentRunStore, InMemoryApprovalStore,
        InMemoryCheckpointStore, InMemoryPendingToolCallStore,
    };
    use crate::agentic::{
        ApprovalState, ChangeSet, ChangeSetRecoveryState, ChangeStepDraft, ExecutionState,
    };
    use crate::policy::{PolicyDecision, PolicyEvaluation, PolicyScope};
    use crate::tools::{NativeToolName, ToolData, ToolResult};
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn command_event_preview_is_bounded_and_marks_truncation() {
        let outcome = CommandOutcome {
            command_id: Uuid::new_v4(),
            success: true,
            exit_code: None,
            stdout: "x".repeat(COMMAND_EVENT_PREVIEW_CHARS + 100),
            stderr: String::new(),
            error_code: None,
            duration_ms: 1,
            cancelled: false,
        };
        let preview = command_output_preview(&outcome);
        assert_eq!(
            preview
                .chars()
                .filter(|character| *character == 'x')
                .count(),
            COMMAND_EVENT_PREVIEW_CHARS
        );
        assert!(preview.ends_with("\n…"));
    }

    struct FnReasoner<F>(F);

    #[async_trait]
    impl<F> Reasoner for FnReasoner<F>
    where
        F: for<'a> Fn(&ReasonerInput<'a>) -> Result<AgentDecision, ReasonerError> + Send + Sync,
    {
        async fn decide(&self, input: &ReasonerInput<'_>) -> Result<AgentDecision, ReasonerError> {
            (self.0)(input)
        }
    }

    struct FnDispatcher<F>(F);

    #[async_trait]
    impl<F> ToolDispatcher for FnDispatcher<F>
    where
        F: Fn(&PreparedToolCall) -> ToolOutcome + Send + Sync,
    {
        async fn execute_reads(
            &self,
            _run_id: Uuid,
            calls: &[PreparedToolCall],
        ) -> Vec<ToolOutcome> {
            calls.iter().map(&self.0).collect()
        }
    }

    fn tool_calls(requests: &[(&str, serde_json::Value)]) -> AgentDecision {
        AgentDecision::ToolCalls(
            requests
                .iter()
                .map(|(tool_name, arguments)| ToolCallRequest {
                    tool_name: (*tool_name).into(),
                    arguments: arguments.clone(),
                    reason_summary: format!("Running {tool_name}"),
                })
                .collect(),
        )
    }

    fn final_answer(summary: &str) -> AgentDecision {
        AgentDecision::Final {
            summary: summary.into(),
        }
    }

    fn success_outcome(call: &PreparedToolCall, detail: &str) -> ToolOutcome {
        ToolOutcome {
            tool_call_id: call.tool_call_id,
            tool_name: call.tool_name.clone(),
            success: true,
            summary: "collected".into(),
            error_code: None,
            duration_ms: 3,
            cancelled: false,
            from_cache: false,
            untrusted_remote_data: true,
            sanitized_data: Some(detail.into()),
            tool_result: None,
        }
    }

    fn failure_outcome(call: &PreparedToolCall, error_code: &str) -> ToolOutcome {
        ToolOutcome {
            tool_call_id: call.tool_call_id,
            tool_name: call.tool_name.clone(),
            success: false,
            summary: "execution failed".into(),
            error_code: Some(error_code.into()),
            duration_ms: 3,
            cancelled: false,
            from_cache: false,
            untrusted_remote_data: false,
            sanitized_data: None,
            tool_result: None,
        }
    }

    fn nginx_evidence_outcome(call: &PreparedToolCall) -> ToolOutcome {
        let result = ToolResult {
            invocation_id: call.tool_call_id,
            tool_name: NativeToolName::NginxTest,
            success: true,
            summary: "nginx config valid",
            data: Some(ToolData::NginxTest(crate::tools::NginxTestData {
                valid: true,
                config_file: Some("/etc/nginx/nginx.conf".into()),
                error_file: None,
                error_line: None,
                error_message: None,
                raw_summary: "syntax ok".into(),
            })),
            error_code: None,
            warnings: Vec::new(),
            started_at_epoch_ms: 0,
            duration_ms: 3,
            truncated: false,
            cancelled: false,
            untrusted_remote_data: true,
        };
        ToolOutcome {
            tool_call_id: call.tool_call_id,
            tool_name: call.tool_name.clone(),
            success: true,
            summary: "nginx config valid".into(),
            error_code: None,
            duration_ms: 3,
            cancelled: false,
            from_cache: false,
            untrusted_remote_data: true,
            sanitized_data: Some("valid".into()),
            tool_result: Some(result),
        }
    }

    #[derive(Clone)]
    struct MockChangeSetExecutor {
        verify_ok: bool,
        execution_state: ExecutionState,
    }

    impl MockChangeSetExecutor {
        fn verified() -> Self {
            Self {
                verify_ok: true,
                execution_state: ExecutionState::Committed,
            }
        }

        fn verification_failed() -> Self {
            Self {
                verify_ok: false,
                execution_state: ExecutionState::Committed,
            }
        }
    }

    fn mock_change_set(run_id: Uuid, title: &str) -> ChangeSet {
        ChangeSet {
            id: Uuid::new_v4(),
            agent_run_id: run_id,
            session_id: Uuid::new_v4(),
            title: title.into(),
            version: 1,
            risk: crate::tools::RiskLevel::R3,
            steps: Vec::new(),
            approval_state: ApprovalState::Draft,
            approved_version: None,
            execution_state: ExecutionState::NotStarted,
            recovery_state: ChangeSetRecoveryState::Live,
            preconditions: Vec::new(),
            policy_evaluation: Some(PolicyEvaluation {
                decision: PolicyDecision::RequireApproval,
                matched_rules: Vec::new(),
                reason: "test".into(),
                scope: PolicyScope::Global,
                policy_version: 1,
                policy_hash: "test-policy".into(),
            }),
            policy_snapshot: None,
            approved_step_ids: Vec::new(),
        }
    }

    #[async_trait]
    impl ChangeSetExecutor for MockChangeSetExecutor {
        async fn draft_and_check_policy(
            &self,
            run_id: Uuid,
            title: String,
            _steps: Vec<ChangeStepDraft>,
        ) -> Result<ChangeSet, ChangeSetExecutorError> {
            Ok(mock_change_set(run_id, &title))
        }

        async fn approve(
            &self,
            change_set_id: Uuid,
            version: u64,
        ) -> Result<ChangeSet, ChangeSetExecutorError> {
            let mut change_set = mock_change_set(Uuid::new_v4(), "approved");
            change_set.id = change_set_id;
            change_set.version = version;
            change_set.approval_state = ApprovalState::Approved;
            Ok(change_set)
        }

        async fn execute(
            &self,
            change_set_id: Uuid,
            version: u64,
        ) -> Result<ChangeSet, ChangeSetExecutorError> {
            let mut change_set = mock_change_set(Uuid::new_v4(), "executed");
            change_set.id = change_set_id;
            change_set.version = version;
            change_set.execution_state = self.execution_state;
            Ok(change_set)
        }

        async fn verify(
            &self,
            _change_set_id: Uuid,
            _version: u64,
        ) -> Result<bool, ChangeSetExecutorError> {
            Ok(self.verify_ok)
        }

        async fn rollback(
            &self,
            change_set_id: Uuid,
            version: u64,
        ) -> Result<ChangeSet, ChangeSetExecutorError> {
            let mut change_set = mock_change_set(Uuid::new_v4(), "rolled back");
            change_set.id = change_set_id;
            change_set.version = version;
            change_set.execution_state = ExecutionState::RolledBack;
            Ok(change_set)
        }
    }

    struct NoopChangeSetExecutor;

    #[async_trait]
    impl ChangeSetExecutor for NoopChangeSetExecutor {
        async fn draft_and_check_policy(
            &self,
            _: Uuid,
            _: String,
            _: Vec<ChangeStepDraft>,
        ) -> Result<ChangeSet, ChangeSetExecutorError> {
            Err(ChangeSetExecutorError::InvalidOperation)
        }

        async fn approve(&self, _: Uuid, _: u64) -> Result<ChangeSet, ChangeSetExecutorError> {
            Err(ChangeSetExecutorError::InvalidOperation)
        }

        async fn execute(&self, _: Uuid, _: u64) -> Result<ChangeSet, ChangeSetExecutorError> {
            Err(ChangeSetExecutorError::InvalidOperation)
        }

        async fn verify(&self, _: Uuid, _: u64) -> Result<bool, ChangeSetExecutorError> {
            Ok(false)
        }

        async fn rollback(&self, _: Uuid, _: u64) -> Result<ChangeSet, ChangeSetExecutorError> {
            Err(ChangeSetExecutorError::InvalidOperation)
        }
    }

    struct Harness {
        run_store: Arc<InMemoryAgentRunStore>,
        events: Arc<InMemoryAgentEventRepository>,
        approvals: Arc<InMemoryApprovalStore>,
        checkpoints: Arc<InMemoryCheckpointStore>,
        pending_calls: Arc<InMemoryPendingToolCallStore>,
        cancel_tx: watch::Sender<bool>,
        cancel_rx: watch::Receiver<bool>,
        target_id: Uuid,
    }

    impl Harness {
        fn new() -> Self {
            let (cancel_tx, cancel_rx) = watch::channel(false);
            Self {
                run_store: Arc::new(InMemoryAgentRunStore::default()),
                events: Arc::new(InMemoryAgentEventRepository::default()),
                approvals: Arc::new(InMemoryApprovalStore::default()),
                checkpoints: Arc::new(InMemoryCheckpointStore::default()),
                pending_calls: Arc::new(InMemoryPendingToolCallStore::default()),
                cancel_tx,
                cancel_rx,
                target_id: Uuid::new_v4(),
            }
        }

        fn stores(&self) -> AgentStores {
            AgentStores {
                runs: self.run_store.clone(),
                events: self.events.clone(),
                approvals: self.approvals.clone(),
                checkpoints: self.checkpoints.clone(),
                pending_calls: self.pending_calls.clone(),
            }
        }

        fn config(
            &self,
            gate: Arc<dyn AuthorizationGate>,
            budget: RunBudget,
            changesets: Arc<dyn ChangeSetExecutor>,
        ) -> AgentControllerConfig {
            AgentControllerConfig {
                gate,
                policy_matcher: Arc::new(FixedPolicyMatcher { matches: true }),
                changesets,
                artifacts: Arc::new(InMemoryArtifactStore::default()),
                target_ids: vec![self.target_id],
                session_id: None,
                budget,
                cancellation: self.cancel_rx.clone(),
            }
        }

        fn controller_with_goal<R: Reasoner, D: ToolDispatcher>(
            &self,
            goal: &str,
            reasoner: R,
            dispatcher: D,
            gate: Arc<dyn AuthorizationGate>,
            budget: RunBudget,
            changesets: Arc<dyn ChangeSetExecutor>,
        ) -> AgentController<R, D> {
            AgentController::new(
                goal,
                reasoner,
                dispatcher,
                self.stores(),
                self.config(gate, budget, changesets),
            )
            .expect("controller builds")
        }

        fn controller<R: Reasoner, D: ToolDispatcher>(
            &self,
            reasoner: R,
            dispatcher: D,
            gate: Arc<dyn AuthorizationGate>,
            budget: RunBudget,
        ) -> AgentController<R, D> {
            self.controller_with_goal(
                "Diagnose why the website is slow",
                reasoner,
                dispatcher,
                gate,
                budget,
                Arc::new(NoopChangeSetExecutor),
            )
        }

        fn auto_controller<R: Reasoner, D: ToolDispatcher>(
            &self,
            reasoner: R,
            dispatcher: D,
            budget: RunBudget,
        ) -> AgentController<R, D> {
            self.controller(
                reasoner,
                dispatcher,
                Arc::new(AutoAuthorizationGate),
                budget,
            )
        }

        fn event_types(&self, run_id: Uuid) -> Vec<&'static str> {
            self.events
                .all_events(run_id)
                .expect("events load")
                .iter()
                .map(|envelope| envelope.event.event_type())
                .collect()
        }

        fn requested_tools(&self, run_id: Uuid) -> Vec<String> {
            self.events
                .all_events(run_id)
                .expect("events load")
                .iter()
                .filter_map(|envelope| match &envelope.event {
                    AgentEvent::ToolRequested { tool_name, .. } => Some(tool_name.clone()),
                    _ => None,
                })
                .collect()
        }
    }

    /// A reasoner mimicking a diagnosis: check disk; if usage is high dig
    /// into /var; otherwise finish immediately.
    fn diagnosis_reasoner(
    ) -> impl for<'a> Fn(&ReasonerInput<'a>) -> Result<AgentDecision, ReasonerError> {
        |input: &ReasonerInput<'_>| {
            if input.observations.is_empty() {
                return Ok(tool_calls(&[("system.disk_usage", json!({}))]));
            }
            let disk_is_full = input.observations.iter().any(|observation| {
                observation
                    .detail
                    .as_deref()
                    .is_some_and(|d| d.contains("94%"))
            });
            let dug_deeper = input.observations.iter().any(|observation| {
                observation.tool_name.as_deref() == Some("system.directory_usage")
            });
            if disk_is_full && !dug_deeper {
                return Ok(tool_calls(&[(
                    "system.directory_usage",
                    json!({"path": "/var"}),
                )]));
            }
            if disk_is_full {
                Ok(final_answer("Root filesystem is filled by /var."))
            } else {
                Ok(final_answer("Disk usage is healthy."))
            }
        }
    }

    #[tokio::test]
    async fn two_round_diagnosis_completes_through_the_loop() {
        let harness = Harness::new();
        let mut controller = harness.auto_controller(
            FnReasoner(diagnosis_reasoner()),
            FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "usage 94% on /")),
            RunBudget::default(),
        );
        let outcome = controller.run_to_interrupt().await.expect("run drives");

        assert_eq!(
            outcome,
            RunOutcome::Completed {
                summary: "Root filesystem is filled by /var.".into()
            }
        );
        assert_eq!(controller.state(), AgentRunStateV2::Completed);
        assert_eq!(
            harness.requested_tools(controller.run_id()),
            vec![
                "system.disk_usage".to_owned(),
                "system.directory_usage".to_owned()
            ]
        );
        let types = harness.event_types(controller.run_id());
        assert_eq!(types.first(), Some(&"run_created"));
        assert_eq!(types.last(), Some(&"run_completed"));
        assert_eq!(
            types.iter().filter(|t| **t == "reasoning_started").count(),
            3
        );
    }

    /// AR2-B gate: a real ToolResult changes the Agent's next action.
    #[tokio::test]
    async fn tool_result_changes_the_next_action() {
        // Same reasoner, different first observation → different second step.
        let full = Harness::new();
        let mut full_disk = full.auto_controller(
            FnReasoner(diagnosis_reasoner()),
            FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "usage 94% on /")),
            RunBudget::default(),
        );
        full_disk.run_to_interrupt().await.expect("run drives");
        assert_eq!(
            full.requested_tools(full_disk.run_id()).len(),
            2,
            "high usage must trigger a follow-up read"
        );

        let healthy = Harness::new();
        let mut healthy_disk = healthy.auto_controller(
            FnReasoner(diagnosis_reasoner()),
            FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "usage 12% on /")),
            RunBudget::default(),
        );
        let outcome = healthy_disk.run_to_interrupt().await.expect("run drives");
        assert_eq!(
            healthy.requested_tools(healthy_disk.run_id()),
            vec!["system.disk_usage".to_owned()],
            "healthy usage must not trigger the follow-up read"
        );
        assert_eq!(
            outcome,
            RunOutcome::Completed {
                summary: "Disk usage is healthy.".into()
            }
        );
    }

    #[tokio::test]
    async fn tool_failure_becomes_an_observation_and_an_alternate_tool_runs() {
        let harness = Harness::new();
        let reasoner = FnReasoner(|input: &ReasonerInput<'_>| {
            if input.observations.is_empty() {
                return Ok(tool_calls(&[(
                    "service.logs",
                    json!({"service": "nginx", "lines": 100}),
                )]));
            }
            let logs_failed = input.observations.iter().any(|observation| {
                observation.tool_name.as_deref() == Some("service.logs") && !observation.success
            });
            let docker_succeeded = input.observations.iter().any(|observation| {
                observation.tool_name.as_deref() == Some("docker.logs") && observation.success
            });
            if logs_failed && !docker_succeeded {
                return Ok(tool_calls(&[(
                    "docker.logs",
                    json!({"container": "web", "lines": 100}),
                )]));
            }
            Ok(final_answer("Investigated via container logs."))
        });
        let dispatcher = FnDispatcher(|call: &PreparedToolCall| {
            if call.tool_name == "service.logs" {
                failure_outcome(call, "EXEC_TIMED_OUT")
            } else {
                success_outcome(call, "container log tail")
            }
        });
        let mut controller = harness.auto_controller(reasoner, dispatcher, RunBudget::default());
        let outcome = controller.run_to_interrupt().await.expect("run drives");

        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        assert_eq!(
            controller.state(),
            AgentRunStateV2::Completed,
            "tool failure must not fail the run"
        );
        let types = harness.event_types(controller.run_id());
        assert!(types.contains(&"tool_failed"));
        assert!(types.contains(&"tool_completed"));
    }

    #[tokio::test]
    async fn ask_user_parks_the_run_in_a_resumable_interrupt() {
        let harness = Harness::new();
        let mut controller = harness.auto_controller(
            FnReasoner(|_: &ReasonerInput<'_>| {
                Ok(AgentDecision::AskUser {
                    question: "Which site should I diagnose?".into(),
                })
            }),
            FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "")),
            RunBudget::default(),
        );
        let outcome = controller.run_to_interrupt().await.expect("run drives");

        assert_eq!(
            outcome,
            RunOutcome::AwaitingUser {
                question: "Which site should I diagnose?".into()
            }
        );
        assert_eq!(controller.state(), AgentRunStateV2::AwaitingUser);
        assert!(controller.state().is_interrupt());
        assert!(controller.state().can_resume());
        // The interrupt state is persisted, ready for the AR2-C resume engine.
        let stored = harness
            .run_store
            .get(controller.run_id())
            .expect("store reads")
            .expect("run persisted");
        assert_eq!(stored.state(), AgentRunStateV2::AwaitingUser);
        assert!(harness
            .event_types(controller.run_id())
            .contains(&"user_input_required"));
    }

    #[tokio::test]
    async fn immediate_final_answer_completes_the_run() {
        let harness = Harness::new();
        let mut controller = harness.auto_controller(
            FnReasoner(|_: &ReasonerInput<'_>| Ok(final_answer("All services are healthy."))),
            FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "")),
            RunBudget::default(),
        );
        let outcome = controller.run_to_interrupt().await.expect("run drives");

        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        let types = harness.event_types(controller.run_id());
        assert!(types.contains(&"assistant_message_added"));
        assert_eq!(types.last(), Some(&"run_completed"));
    }

    #[tokio::test]
    async fn cancellation_ends_the_run_as_cancelled() {
        let harness = Harness::new();
        harness.cancel_tx.send(true).expect("cancel signal");
        let mut controller = harness.auto_controller(
            FnReasoner(|_: &ReasonerInput<'_>| Ok(final_answer("never reached"))),
            FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "")),
            RunBudget::default(),
        );
        let outcome = controller.run_to_interrupt().await.expect("run drives");

        assert_eq!(outcome, RunOutcome::Cancelled);
        assert_eq!(controller.state(), AgentRunStateV2::Cancelled);
        assert!(harness
            .event_types(controller.run_id())
            .contains(&"run_cancelled"));
    }

    #[tokio::test]
    async fn round_budget_exhaustion_fails_with_a_stable_code() {
        let harness = Harness::new();
        let mut controller = harness.auto_controller(
            FnReasoner(|_: &ReasonerInput<'_>| Ok(tool_calls(&[("system.disk_usage", json!({}))]))),
            FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "usage 50%")),
            RunBudget {
                max_reasoner_rounds: 2,
                max_tool_calls: 20,
                time_budget_ms: 60_000,
            },
        );
        let outcome = controller.run_to_interrupt().await.expect("run drives");

        assert_eq!(
            outcome,
            RunOutcome::Failed {
                error_code: AGENT_BUDGET_EXCEEDED
            }
        );
        assert_eq!(controller.state(), AgentRunStateV2::Failed);
    }

    #[tokio::test]
    async fn tool_call_budget_exhaustion_fails_with_a_stable_code() {
        let harness = Harness::new();
        let mut controller = harness.auto_controller(
            FnReasoner(|_: &ReasonerInput<'_>| {
                Ok(tool_calls(&[
                    ("system.disk_usage", json!({})),
                    ("system.info", json!({})),
                ]))
            }),
            FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "")),
            RunBudget {
                max_reasoner_rounds: 8,
                max_tool_calls: 1,
                time_budget_ms: 60_000,
            },
        );
        let outcome = controller.run_to_interrupt().await.expect("run drives");
        assert_eq!(
            outcome,
            RunOutcome::Failed {
                error_code: AGENT_BUDGET_EXCEEDED
            }
        );
    }

    #[tokio::test]
    async fn time_budget_exhaustion_fails_with_a_stable_code() {
        let harness = Harness::new();
        let mut controller = harness.auto_controller(
            FnReasoner(|_: &ReasonerInput<'_>| Ok(final_answer("never reached"))),
            FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "")),
            RunBudget {
                max_reasoner_rounds: 8,
                max_tool_calls: 20,
                time_budget_ms: 0,
            },
        );
        let outcome = controller.run_to_interrupt().await.expect("run drives");
        assert_eq!(
            outcome,
            RunOutcome::Failed {
                error_code: AGENT_TIME_BUDGET_EXCEEDED
            }
        );
        assert!(harness
            .event_types(controller.run_id())
            .contains(&"run_failed"));
    }

    #[tokio::test]
    async fn invalid_decisions_become_observations_and_the_run_recovers() {
        let harness = Harness::new();
        let reasoner = FnReasoner(|input: &ReasonerInput<'_>| {
            if input.observations.is_empty() {
                // The model tries a write tool; Rust must reject it.
                Ok(tool_calls(&[(
                    "service.restart",
                    json!({"service": "nginx"}),
                )]))
            } else {
                Ok(final_answer("Answered without the write tool."))
            }
        });
        let dispatcher = FnDispatcher(|_: &PreparedToolCall| -> ToolOutcome {
            panic!("rejected decisions must never reach the dispatcher")
        });
        let mut controller = harness.auto_controller(reasoner, dispatcher, RunBudget::default());
        let outcome = controller.run_to_interrupt().await.expect("run drives");

        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        let types = harness.event_types(controller.run_id());
        assert!(types.contains(&"observation_added"));
        assert!(
            !types.contains(&"tool_started"),
            "no tool may start from a rejected decision"
        );
    }

    #[tokio::test]
    async fn reasoner_infrastructure_failure_fails_the_run() {
        let harness = Harness::new();
        let mut controller = harness.auto_controller(
            FnReasoner(|_: &ReasonerInput<'_>| Err(ReasonerError::unavailable())),
            FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "")),
            RunBudget::default(),
        );
        let outcome = controller.run_to_interrupt().await.expect("run drives");
        assert_eq!(
            outcome,
            RunOutcome::Failed {
                error_code: AGENT_REASONER_FAILED
            }
        );
    }

    #[tokio::test]
    async fn reasoner_receives_the_phase_10k_context_snapshot() {
        let harness = Harness::new();
        let reasoner = FnReasoner(|input: &ReasonerInput<'_>| {
            if input.observations.is_empty() {
                assert_eq!(
                    input.context.items.len(),
                    1,
                    "first round context is the redacted goal only"
                );
                assert!(input.context.items[0].content.contains("website is slow"));
                Ok(tool_calls(&[("system.disk_usage", json!({}))]))
            } else {
                assert!(
                    input.context.items.len() >= 2,
                    "later rounds add sanitized observations"
                );
                assert!(
                    input
                        .context
                        .items
                        .iter()
                        .any(|item| item.content.contains("usage 94% on /")),
                    "tool observations must reach the model context"
                );
                Ok(final_answer("Context carried the observation."))
            }
        });
        let mut controller = harness.auto_controller(
            reasoner,
            FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "usage 94% on /")),
            RunBudget::default(),
        );
        let outcome = controller.run_to_interrupt().await.expect("run drives");
        assert!(matches!(outcome, RunOutcome::Completed { .. }));
    }

    #[tokio::test]
    async fn events_are_persisted_with_contiguous_sequences() {
        let harness = Harness::new();
        let mut controller = harness.auto_controller(
            FnReasoner(diagnosis_reasoner()),
            FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "usage 94% on /")),
            RunBudget::default(),
        );
        controller.run_to_interrupt().await.expect("run drives");

        let events = harness
            .events
            .all_events(controller.run_id())
            .expect("events load");
        let sequences: Vec<u64> = events.iter().map(|envelope| envelope.seq).collect();
        let expected: Vec<u64> = (1..=u64::try_from(events.len()).expect("fits")).collect();
        assert_eq!(sequences, expected, "event log must be gapless from 1");
    }

    #[test]
    fn empty_goal_is_rejected_before_a_run_exists() {
        let harness = Harness::new();
        let result = AgentController::new(
            "   ",
            FnReasoner(|_: &ReasonerInput<'_>| Ok(final_answer("x"))),
            FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "")),
            harness.stores(),
            harness.config(
                Arc::new(AutoAuthorizationGate),
                RunBudget::default(),
                Arc::new(NoopChangeSetExecutor),
            ),
        );
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("blank goal must be rejected"),
        };
        assert_eq!(error.code(), AGENT_GOAL_INVALID);
    }

    fn approval_gate() -> Arc<dyn AuthorizationGate> {
        Arc::new(FnAuthorizationGate(|call: &PreparedToolCall| {
            if call.tool_name == "service.logs" {
                Authorization::RequireApproval {
                    risk: "R1".into(),
                    policy_version: 3,
                    policy_hash: "policy-v3".into(),
                }
            } else {
                Authorization::Auto
            }
        }))
    }

    #[tokio::test]
    async fn approval_interrupt_parks_the_run_without_executing() {
        let harness = Harness::new();
        let mut controller = harness.controller(
            FnReasoner(|_: &ReasonerInput<'_>| {
                Ok(tool_calls(&[(
                    "service.logs",
                    json!({"service": "nginx", "lines": 100}),
                )]))
            }),
            FnDispatcher(|_: &PreparedToolCall| panic!("must not execute before approval")),
            approval_gate(),
            RunBudget::default(),
        );
        let outcome = controller.run_to_interrupt().await.expect("run drives");
        let RunOutcome::AwaitingApproval {
            approval_id,
            tool_call_id,
        } = outcome
        else {
            panic!("expected awaiting approval interrupt");
        };
        assert_eq!(controller.run_id(), controller.run_id());
        assert_eq!(controller.state(), AgentRunStateV2::AwaitingApproval);
        assert!(harness
            .event_types(controller.run_id())
            .contains(&"tool_approval_required"));
        assert!(harness.approvals.get(approval_id).expect("get").is_some());
        assert!(harness
            .pending_calls
            .get(tool_call_id)
            .expect("get")
            .is_some());
    }

    #[tokio::test]
    async fn approve_resumes_the_same_run_id_and_executes() {
        let harness = Harness::new();
        let run_id;
        let approval_id;
        {
            let mut controller = harness.controller(
                FnReasoner(|input: &ReasonerInput<'_>| {
                    if input.observations.is_empty() {
                        Ok(tool_calls(&[(
                            "service.logs",
                            json!({"service": "nginx", "lines": 100}),
                        )]))
                    } else {
                        Ok(final_answer("Logs reviewed after approval."))
                    }
                }),
                FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "log tail")),
                approval_gate(),
                RunBudget::default(),
            );
            run_id = controller.run_id();
            let RunOutcome::AwaitingApproval {
                approval_id: id, ..
            } = controller.run_to_interrupt().await.expect("interrupt")
            else {
                panic!("expected approval interrupt");
            };
            approval_id = id;
            let outcome = controller.approve(approval_id).await.expect("approve");
            assert!(matches!(outcome, RunOutcome::Completed { .. }));
            assert_eq!(controller.run_id(), run_id);
        }
        assert_eq!(
            harness
                .run_store
                .get(run_id)
                .expect("get")
                .expect("run")
                .state(),
            AgentRunStateV2::Completed
        );
        let types = harness.event_types(run_id);
        assert!(types.contains(&"approval_granted"));
        assert!(types.contains(&"tool_completed"));
    }

    #[tokio::test]
    async fn reject_becomes_an_observation_and_reasons_again() {
        let harness = Harness::new();
        let mut controller = harness.controller(
            FnReasoner(|input: &ReasonerInput<'_>| {
                let rejected = input
                    .observations
                    .iter()
                    .any(|observation| observation.summary.contains("User rejected service.logs"));
                if rejected {
                    return Ok(final_answer("Found an alternative after rejection."));
                }
                Ok(tool_calls(&[(
                    "service.logs",
                    json!({"service": "nginx", "lines": 100}),
                )]))
            }),
            FnDispatcher(|_: &PreparedToolCall| panic!("rejected call must not execute")),
            approval_gate(),
            RunBudget::default(),
        );
        let run_id = controller.run_id();
        let RunOutcome::AwaitingApproval { approval_id, .. } =
            controller.run_to_interrupt().await.expect("interrupt")
        else {
            panic!("expected approval interrupt");
        };
        let outcome = controller.reject(approval_id).await.expect("reject");
        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        assert_eq!(controller.run_id(), run_id);
        let types = harness.event_types(run_id);
        assert!(types.contains(&"approval_rejected"));
        assert!(types.contains(&"observation_added"));
        assert!(types.contains(&"run_completed"));
    }

    #[tokio::test]
    async fn ask_user_resume_continues_the_same_run() {
        let harness = Harness::new();
        let mut controller = harness.auto_controller(
            FnReasoner(|input: &ReasonerInput<'_>| {
                if input
                    .observations
                    .iter()
                    .any(|observation| observation.summary.contains("User replied"))
                {
                    return Ok(final_answer("Thanks for the site name."));
                }
                Ok(AgentDecision::AskUser {
                    question: "Which site?".into(),
                })
            }),
            FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "")),
            RunBudget::default(),
        );
        let run_id = controller.run_id();
        controller.run_to_interrupt().await.expect("interrupt");
        let outcome = controller
            .resume_with_user_input("example.com")
            .await
            .expect("resume");
        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        assert_eq!(controller.run_id(), run_id);
        assert!(harness.event_types(run_id).contains(&"user_input_received"));
    }

    #[tokio::test]
    async fn completed_conversation_accepts_explicit_turn_with_same_id_and_context() {
        let harness = Harness::new();
        let mut controller = harness.auto_controller(
            FnReasoner(|input: &ReasonerInput<'_>| {
                if input.user_replies.is_empty() {
                    Ok(final_answer("First answer"))
                } else {
                    assert!(input.user_replies.iter().any(|reply| reply == "Follow up"));
                    Ok(final_answer("Second answer"))
                }
            }),
            FnDispatcher(|_: &PreparedToolCall| panic!("no command requested")),
            RunBudget {
                max_reasoner_rounds: 1,
                ..RunBudget::default()
            },
        );
        let run_id = controller.run_id();
        assert!(matches!(
            controller.run_to_interrupt().await.expect("first"),
            RunOutcome::Completed { .. }
        ));
        assert!(controller.continue_conversation(" ").await.is_err());
        assert!(matches!(
            controller
                .continue_conversation("Follow up")
                .await
                .expect("second"),
            RunOutcome::Completed { .. }
        ));
        assert_eq!(controller.run_id(), run_id);
        let events = harness.event_types(run_id);
        assert_eq!(
            events
                .iter()
                .filter(|kind| **kind == "run_completed")
                .count(),
            2
        );
        assert_eq!(
            events
                .iter()
                .filter(|kind| **kind == "user_message_added")
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn mutated_arguments_invalidate_approval_on_resume() {
        let harness = Harness::new();
        let mut controller = harness.controller(
            FnReasoner(|input: &ReasonerInput<'_>| {
                if input.observations.iter().any(|observation| {
                    observation
                        .error_code
                        .as_deref()
                        .is_some_and(|code| code.contains("ARGUMENTS_CHANGED"))
                }) {
                    return Ok(final_answer("Replanned after invalidation."));
                }
                Ok(tool_calls(&[(
                    "service.logs",
                    json!({"service": "nginx", "lines": 100}),
                )]))
            }),
            FnDispatcher(|_: &PreparedToolCall| panic!("invalidated call must not execute")),
            approval_gate(),
            RunBudget::default(),
        );
        let RunOutcome::AwaitingApproval {
            approval_id,
            tool_call_id,
            ..
        } = controller.run_to_interrupt().await.expect("interrupt")
        else {
            panic!("expected approval interrupt");
        };
        // Simulate argument drift: replace the pending call with different lines.
        harness.pending_calls.remove(tool_call_id).expect("remove");
        let mut swapped = validate_decision(tool_calls(&[(
            "service.logs",
            json!({"service": "nginx", "lines": 200}),
        )]))
        .expect("validates");
        let ValidatedDecision::ToolCalls(ref mut calls) = swapped else {
            panic!("tool calls");
        };
        calls[0].tool_call_id = tool_call_id;
        harness
            .pending_calls
            .put(calls[0].clone())
            .expect("put swapped call");
        let outcome = controller
            .approve(approval_id)
            .await
            .expect("approve attempt");
        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        assert!(harness
            .event_types(controller.run_id())
            .contains(&"approval_invalidated"));
    }

    #[tokio::test]
    async fn changed_targets_invalidate_approval_on_resume() {
        let harness = Harness::new();
        let mut controller = harness.controller(
            FnReasoner(|input: &ReasonerInput<'_>| {
                if input.observations.iter().any(|observation| {
                    observation
                        .error_code
                        .as_deref()
                        .is_some_and(|code| code.contains("TARGETS_CHANGED"))
                }) {
                    return Ok(final_answer("Replanned after target change."));
                }
                Ok(tool_calls(&[(
                    "service.logs",
                    json!({"service": "nginx", "lines": 100}),
                )]))
            }),
            FnDispatcher(|_: &PreparedToolCall| panic!("invalidated call must not execute")),
            approval_gate(),
            RunBudget::default(),
        );
        let RunOutcome::AwaitingApproval { approval_id, .. } =
            controller.run_to_interrupt().await.expect("interrupt")
        else {
            panic!("expected approval interrupt");
        };
        let mut approval = harness
            .approvals
            .get(approval_id)
            .expect("get")
            .expect("approval exists");
        approval.target_ids.push(Uuid::new_v4());
        harness.approvals.insert(approval).expect("update approval");
        let outcome = controller
            .approve(approval_id)
            .await
            .expect("approve attempt");
        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        assert!(harness
            .event_types(controller.run_id())
            .contains(&"approval_invalidated"));
    }

    #[tokio::test]
    async fn cancel_while_awaiting_approval_ends_the_same_run() {
        let harness = Harness::new();
        let mut controller = harness.controller(
            FnReasoner(|_: &ReasonerInput<'_>| {
                Ok(tool_calls(&[(
                    "service.logs",
                    json!({"service": "nginx", "lines": 100}),
                )]))
            }),
            FnDispatcher(|_: &PreparedToolCall| panic!("must not execute")),
            approval_gate(),
            RunBudget::default(),
        );
        let run_id = controller.run_id();
        assert!(matches!(
            controller.run_to_interrupt().await.expect("interrupt"),
            RunOutcome::AwaitingApproval { .. }
        ));
        let outcome = controller.cancel().expect("cancel");
        assert_eq!(outcome, RunOutcome::Cancelled);
        assert_eq!(controller.run_id(), run_id);
        assert_eq!(controller.state(), AgentRunStateV2::Cancelled);
    }

    #[tokio::test]
    async fn change_set_proposal_parks_for_approval_and_executes_after_grant() {
        let harness = Harness::new();
        let executor = Arc::new(MockChangeSetExecutor::verified());
        let mut controller = harness.controller_with_goal(
            "Fix nginx configuration and reload",
            FnReasoner(|input: &ReasonerInput<'_>| {
                if input.observations.is_empty() {
                    return Ok(tool_calls(&[("nginx.test", json!({}))]));
                }
                let nginx_evidence = input.observations.iter().find(|observation| {
                    observation.tool_name.as_deref() == Some("nginx.test") && observation.success
                });
                let changeset_observed = input
                    .observations
                    .iter()
                    .any(|observation| observation.tool_name.as_deref() == Some("change_set"));
                if let Some(observation) = nginx_evidence {
                    if !changeset_observed {
                        let Some(evidence_id) = observation.tool_call_id else {
                            return Err(ReasonerError::unavailable());
                        };
                        return Ok(AgentDecision::ProposeChangeSet(ChangeProposalRequest {
                            title: "Reload nginx".into(),
                            summary: "Reload nginx after config validation".into(),
                            evidence_ids: vec![evidence_id],
                            steps: vec![ChangeStepDraft::NginxReload],
                        }));
                    }
                }
                Ok(final_answer("Nginx reload verified."))
            }),
            FnDispatcher(|call: &PreparedToolCall| {
                if call.tool_name == "nginx.test" {
                    nginx_evidence_outcome(call)
                } else {
                    success_outcome(call, "")
                }
            }),
            Arc::new(AutoAuthorizationGate),
            RunBudget::default(),
            executor,
        );
        let run_id = controller.run_id();
        let RunOutcome::AwaitingApproval { approval_id, .. } =
            controller.run_to_interrupt().await.expect("interrupt")
        else {
            panic!("expected change set approval interrupt");
        };
        let types = harness.event_types(run_id);
        assert!(types.contains(&"change_set_proposed"));
        assert!(types.contains(&"tool_approval_required"));
        let outcome = controller.approve(approval_id).await.expect("approve");
        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        let final_types = harness.event_types(run_id);
        assert!(final_types.contains(&"change_set_execution_started"));
        assert!(final_types.contains(&"verification_completed"));
        assert!(final_types.contains(&"run_completed"));
    }

    #[tokio::test]
    async fn change_set_verification_failure_triggers_rollback_and_continues() {
        let harness = Harness::new();
        let executor = Arc::new(MockChangeSetExecutor::verification_failed());
        let mut controller = harness.controller_with_goal(
            "Fix nginx and reload the service",
            FnReasoner(|input: &ReasonerInput<'_>| {
                if input.observations.is_empty() {
                    return Ok(tool_calls(&[("nginx.test", json!({}))]));
                }
                let nginx_evidence = input.observations.iter().find(|observation| {
                    observation.tool_name.as_deref() == Some("nginx.test") && observation.success
                });
                let changeset_observed = input
                    .observations
                    .iter()
                    .any(|observation| observation.tool_name.as_deref() == Some("change_set"));
                if let Some(observation) = nginx_evidence {
                    if !changeset_observed {
                        let Some(evidence_id) = observation.tool_call_id else {
                            return Err(ReasonerError::unavailable());
                        };
                        return Ok(AgentDecision::ProposeChangeSet(ChangeProposalRequest {
                            title: "Reload nginx".into(),
                            summary: "Reload nginx after config validation".into(),
                            evidence_ids: vec![evidence_id],
                            steps: vec![ChangeStepDraft::NginxReload],
                        }));
                    }
                }
                if changeset_observed {
                    return Ok(final_answer("Rollback completed; investigation continues."));
                }
                Ok(final_answer("unexpected branch"))
            }),
            FnDispatcher(|call: &PreparedToolCall| {
                if call.tool_name == "nginx.test" {
                    nginx_evidence_outcome(call)
                } else {
                    success_outcome(call, "")
                }
            }),
            Arc::new(AutoAuthorizationGate),
            RunBudget::default(),
            executor,
        );
        let RunOutcome::AwaitingApproval { approval_id, .. } =
            controller.run_to_interrupt().await.expect("interrupt")
        else {
            panic!("expected approval interrupt");
        };
        let outcome = controller.approve(approval_id).await.expect("approve");
        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        let types = harness.event_types(controller.run_id());
        assert!(types.contains(&"rollback_started"));
        assert!(types.contains(&"rollback_completed"));
        assert!(types.contains(&"observation_added"));
    }

    #[tokio::test]
    async fn large_tool_output_becomes_artifact_not_full_model_context() {
        let harness = Harness::new();
        let huge = "line nginx error\n".repeat(5_000);
        assert!(huge.len() > 4_096);
        let mut controller = harness.auto_controller(
            FnReasoner(|input: &ReasonerInput<'_>| {
                if input.observations.is_empty() {
                    return Ok(tool_calls(&[(
                        "service.logs",
                        json!({"service": "nginx", "lines": 100}),
                    )]));
                }
                // Model context must not contain the full multi-kiloline dump.
                assert!(
                    input.context.items.iter().all(|item| {
                        !item.content.contains("line nginx error\nline nginx error")
                            || item.content.len() < 4_096
                    }),
                    "full log dump must not enter model context"
                );
                assert!(
                    input.context.items.iter().any(|item| {
                        item.source.starts_with("artifact:") || item.content.contains("artifact:")
                    }),
                    "context should carry an artifact reference"
                );
                Ok(final_answer("Reviewed bounded artifact reference."))
            }),
            FnDispatcher(move |call: &PreparedToolCall| {
                let mut outcome = success_outcome(call, &huge);
                outcome.tool_result = Some(ToolResult {
                    invocation_id: call.tool_call_id,
                    tool_name: NativeToolName::ServiceLogs,
                    success: true,
                    summary: "log tail",
                    data: Some(ToolData::ServiceLogs(crate::tools::ServiceLogsData {
                        service: "nginx".into(),
                        entries: (0..5_000)
                            .map(|i| format!("line {i} nginx error"))
                            .collect(),
                    })),
                    error_code: None,
                    warnings: Vec::new(),
                    started_at_epoch_ms: 0,
                    duration_ms: 3,
                    truncated: false,
                    cancelled: false,
                    untrusted_remote_data: true,
                });
                outcome
            }),
            RunBudget::default(),
        );
        let outcome = controller.run_to_interrupt().await.expect("run");
        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        assert!(harness
            .event_types(controller.run_id())
            .contains(&"facts_updated"));
        assert!(controller.working_fact_count() > 0);
    }

    #[tokio::test]
    async fn write_invalidates_working_facts_for_target() {
        let harness = Harness::new();
        let executor = Arc::new(MockChangeSetExecutor::verified());
        let mut controller = harness.controller_with_goal(
            "Fix nginx and reload",
            FnReasoner(|input: &ReasonerInput<'_>| {
                if input.observations.is_empty() {
                    return Ok(tool_calls(&[("nginx.test", json!({}))]));
                }
                let nginx_evidence = input.observations.iter().find(|observation| {
                    observation.tool_name.as_deref() == Some("nginx.test") && observation.success
                });
                let changeset_observed = input
                    .observations
                    .iter()
                    .any(|observation| observation.tool_name.as_deref() == Some("change_set"));
                if let Some(observation) = nginx_evidence {
                    if !changeset_observed {
                        return Ok(AgentDecision::ProposeChangeSet(ChangeProposalRequest {
                            title: "Reload nginx".into(),
                            summary: "Reload nginx after config validation".into(),
                            evidence_ids: vec![observation.tool_call_id.unwrap()],
                            steps: vec![ChangeStepDraft::NginxReload],
                        }));
                    }
                }
                Ok(final_answer("Done after write invalidation."))
            }),
            FnDispatcher(|call: &PreparedToolCall| {
                if call.tool_name == "nginx.test" {
                    nginx_evidence_outcome(call)
                } else {
                    success_outcome(call, "")
                }
            }),
            Arc::new(AutoAuthorizationGate),
            RunBudget::default(),
            executor,
        );
        let RunOutcome::AwaitingApproval { approval_id, .. } =
            controller.run_to_interrupt().await.expect("interrupt")
        else {
            panic!("expected approval");
        };
        assert!(
            controller.has_working_fact_key("nginx.config_valid"),
            "facts should exist before write"
        );
        let _ = controller.approve(approval_id).await.expect("approve");
        assert_eq!(
            controller.working_fact_count(),
            0,
            "successful write must invalidate target facts"
        );
    }
}
