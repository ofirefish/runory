//! Agent Runtime V2 (AR2-A…AR2-E: domain, loop, interrupt/resume, SQLite, IPC).

mod approval;
// Bounded artifact retrieval is implemented and tested, but its model-facing
// tool registration belongs to a later Runtime V2 integration step.
#[allow(dead_code)]
mod artifact;
mod broadcast;
mod changeset;
mod checkpoint;
mod command;
mod controller;
mod decision;
mod dispatch;
mod event;
mod facts;
mod gate;
mod metrics;
mod reasoner;
mod reasoner_planning;
mod regression;
mod repository;
mod routing;
mod run;
mod service;
mod session_dispatch;
mod sqlite;
mod state;
mod tool_error;

pub use approval::{
    validate_pending_approval, ApprovalInvalidationReason, ApprovalRequest, ApprovalRequestState,
    APPROVAL_REASON_ARGUMENTS_CHANGED, APPROVAL_REASON_POLICY_CHANGED,
    APPROVAL_REASON_TARGETS_CHANGED,
};
pub use checkpoint::{AgentCheckpoint, BudgetCheckpoint, PendingInterruptRef};
pub use command::{AgentCommandExecutionService, CommandOutcome};
pub use controller::{
    AgentController, AgentControllerConfig, AgentControllerError, AgentStores, RunBudget,
    RunOutcome, AGENT_BUDGET_EXCEEDED, AGENT_GOAL_INVALID, AGENT_INVALID_USER_INPUT,
    AGENT_NOT_AWAITING_APPROVAL, AGENT_NOT_AWAITING_USER, AGENT_TIME_BUDGET_EXCEEDED,
};
pub use decision::{
    validate_decision, AgentDecision, CommandMutability, CommandProposalRequest, CommandRisk,
    DecisionError, PreparedCommandProposal, PreparedToolCall, ToolCallRequest, ValidatedDecision,
    AGENT_DECISION_INVALID, MAX_TOOL_CALLS_PER_TURN,
};
pub use dispatch::{ToolDispatcher, ToolOutcome};
pub use event::{AgentEvent, AgentEventEnvelope};
pub use gate::{
    Authorization, AuthorizationGate, AutoAuthorizationGate, CommandAuthorization,
    FixedPolicyMatcher, FnAuthorizationGate, PolicySnapshotMatcher, AGENT_POLICY_DENIED,
};
pub use reasoner::{
    BudgetStatus, Observation, Reasoner, ReasonerError, ReasonerInput, AGENT_REASONER_FAILED,
};
pub(crate) use reasoner_planning::HostSessionContext;
pub use repository::{
    AgentEventRepository, AgentRunStore, AgentStoreError, ApprovalStore, CheckpointStore,
    InMemoryAgentEventRepository, InMemoryAgentRunStore, InMemoryApprovalStore,
    InMemoryCheckpointStore, InMemoryPendingToolCallStore, PendingToolCallStore,
    AGENT_APPROVAL_NOT_FOUND, AGENT_EVENT_SEQUENCE_INVALID, AGENT_PERSISTENCE_FAILED,
    AGENT_RUN_ALREADY_EXISTS, AGENT_RUN_NOT_FOUND, AGENT_STORE_CORRUPT,
    AGENT_STORE_MIGRATION_FAILED,
};
pub use run::AgentRun;
pub(crate) use service::AgentRuntimeV2Service;
pub(crate) use sqlite::{AgentHistoryDetail, AgentHistoryEntry};
pub use sqlite::{RecoveredRun, SqliteAgentDatabase, SCHEMA_VERSION};
pub use state::{AgentRunStateV2, AgentStateError, AGENT_INVALID_TRANSITION};
