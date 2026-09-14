//! Runtime V2 Reasoner abstraction (AR2-B).
//!
//! A `Reasoner` receives the current run context and proposes an untrusted
//! `AgentDecision`; the controller validates and executes it. AR2-B ships the
//! trait plus test implementations only. The production `ModelGateway`
//! adapter is deliberately deferred: the current remote `decide()` protocol
//! forbids tool calls in its system prompt, and upgrading it requires
//! touching `agentic/model_gateway.rs`, which carries frozen uncommitted WIP.
//! The adapter lands together with the V2 model protocol stage.

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use super::decision::AgentDecision;
use crate::agentic::context::ContextSnapshot;
use crate::domain::SessionId;

/// Stable error code for Reasoner infrastructure failures (model unreachable,
/// malformed transport, …). Protocol-level bad decisions are not errors —
/// they surface as rejected decisions and become observations.
pub const AGENT_REASONER_FAILED: &str = "AGENT_REASONER_FAILED";

/// One sanitized observation the loop feeds back into reasoning.
///
/// `summary` and `detail` pass `redact_secrets` before they get here and are
/// treated as untrusted remote data in the model context.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    pub tool_call_id: Option<Uuid>,
    pub tool_name: Option<String>,
    pub success: bool,
    pub error_code: Option<String>,
    pub summary: String,
    pub detail: Option<String>,
}

/// Budget snapshot exposed to the Reasoner so it can wrap up before limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudgetStatus {
    pub rounds_used: u32,
    pub max_rounds: u32,
    pub tool_calls_used: u32,
    pub max_tool_calls: u32,
    pub elapsed_ms: u64,
    pub time_budget_ms: u64,
}

/// Per-round Reasoner input. The context snapshot comes from the Phase 10K
/// context manager (`agentic::context::snapshot`) — V2 does not build a
/// second context system.
pub struct ReasonerInput<'a> {
    pub run_id: Uuid,
    /// Bound live SSH session. Production runs always set this; isolated
    /// controller tests may leave it unset.
    pub session_id: Option<SessionId>,
    pub target_ids: &'a [Uuid],
    pub goal: &'a str,
    /// 1-based reasoning round.
    pub round: u32,
    pub observations: &'a [Observation],
    /// Redacted user replies collected through `AwaitingUser` resumes.
    pub user_replies: &'a [String],
    pub budget: BudgetStatus,
    /// When true, the previous mutating/unknown command still needs a
    /// successful approved ReadIntent command before `Final` is allowed.
    pub verification_required: bool,
    // Read by unit tests today; the production reader is the ModelGateway
    // adapter, which lands with the V2 model protocol stage.
    #[allow(dead_code)]
    pub(crate) context: &'a ContextSnapshot,
}

/// Reasoner infrastructure failure with a stable code.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("reasoner failed: {code}")]
pub struct ReasonerError {
    pub code: String,
}

impl ReasonerError {
    pub fn unavailable() -> Self {
        Self {
            code: AGENT_REASONER_FAILED.into(),
        }
    }
}

/// Proposes the next decision for a run. Output is untrusted and always
/// passes `validate_decision` before the controller acts on it.
#[async_trait]
pub trait Reasoner: Send + Sync {
    async fn decide(&self, input: &ReasonerInput<'_>) -> Result<AgentDecision, ReasonerError>;
}
