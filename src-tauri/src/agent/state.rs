//! Runtime V2 authoritative run states and explicit transition rules.
//!
//! The legacy `agentic::state::AgentRunState` (doctor runtime) is intentionally
//! left untouched; `AgentRunStateV2` exists independently until a migration
//! adapter maps the old states (`NeedsInput` → `AwaitingUser`,
//! `BudgetExceeded` → `Failed(error_code)`, …) in a later AR2 stage.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable error code for rejected state transitions (IPC/persistence safe).
pub const AGENT_INVALID_TRANSITION: &str = "AGENT_INVALID_TRANSITION";

/// Authoritative Agent Runtime V2 run state (16 states).
///
/// Serialization names are the stable snake_case protocol used by future
/// SQLite persistence, checkpoints and IPC event streaming; never rely on the
/// Rust `Debug` representation for storage.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunStateV2 {
    Created,
    Running,
    Reasoning,
    Acting,
    Observing,
    AwaitingApproval,
    AwaitingUser,
    Diagnosed,
    PlanningChange,
    ExecutingChange,
    Verifying,
    RollingBack,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl AgentRunStateV2 {
    /// Every state, for exhaustive tests and future migrations.
    pub const ALL: [AgentRunStateV2; 16] = [
        AgentRunStateV2::Created,
        AgentRunStateV2::Running,
        AgentRunStateV2::Reasoning,
        AgentRunStateV2::Acting,
        AgentRunStateV2::Observing,
        AgentRunStateV2::AwaitingApproval,
        AgentRunStateV2::AwaitingUser,
        AgentRunStateV2::Diagnosed,
        AgentRunStateV2::PlanningChange,
        AgentRunStateV2::ExecutingChange,
        AgentRunStateV2::Verifying,
        AgentRunStateV2::RollingBack,
        AgentRunStateV2::Paused,
        AgentRunStateV2::Completed,
        AgentRunStateV2::Failed,
        AgentRunStateV2::Cancelled,
    ];

    /// Terminal states never accept another transition.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            AgentRunStateV2::Completed | AgentRunStateV2::Failed | AgentRunStateV2::Cancelled
        )
    }

    /// Durable interrupt states: the run is alive but waits for the user.
    pub fn is_interrupt(self) -> bool {
        matches!(
            self,
            AgentRunStateV2::AwaitingApproval
                | AgentRunStateV2::AwaitingUser
                | AgentRunStateV2::Paused
        )
    }

    /// States in which the runtime itself is progressing the run.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            AgentRunStateV2::Running
                | AgentRunStateV2::Reasoning
                | AgentRunStateV2::Acting
                | AgentRunStateV2::Observing
                | AgentRunStateV2::Diagnosed
                | AgentRunStateV2::PlanningChange
                | AgentRunStateV2::ExecutingChange
                | AgentRunStateV2::Verifying
                | AgentRunStateV2::RollingBack
        )
    }

    /// Only durable interrupt states may resume the same run.
    pub fn can_resume(self) -> bool {
        self.is_interrupt()
    }

    /// Explicit transition rule. Anything not listed here is invalid.
    ///
    /// Decisions beyond the literal minimum in the AR2-A specification (all
    /// chosen to be stricter or safer, and recorded in
    /// `docs/agent-runtime-v2-audit.md`):
    /// - `Cancelled` and `Failed` are uniformly reachable from every
    ///   non-terminal state (user Stop / unrecoverable runtime error).
    /// - `Paused` is only reachable from read/reason phases
    ///   (`Running`, `Reasoning`, `Acting`, `Observing`, `Diagnosed`,
    ///   `PlanningChange`); a write, verification or rollback in flight must
    ///   finish or fail, never silently pause mid side effect.
    /// - `AwaitingApproval → ExecutingChange` is allowed so an approved
    ///   ChangeSet resumes directly into execution (audit §8/§12 flow).
    /// - Terminal states have no outgoing transitions at all.
    pub fn can_transition_to(self, next: AgentRunStateV2) -> bool {
        use AgentRunStateV2 as S;

        if self.is_terminal() {
            return false;
        }
        // Cancellation and hard failure are valid from every non-terminal state.
        if matches!(next, S::Cancelled | S::Failed) {
            return true;
        }
        match self {
            S::Created => matches!(next, S::Running),
            S::Running => matches!(next, S::Reasoning | S::Paused),
            S::Reasoning => matches!(
                next,
                S::Acting
                    | S::AwaitingApproval
                    | S::AwaitingUser
                    | S::PlanningChange
                    | S::Diagnosed
                    | S::Completed
                    | S::Paused
            ),
            S::Acting => matches!(next, S::Observing | S::AwaitingApproval | S::Paused),
            S::Observing => matches!(next, S::Reasoning | S::Diagnosed | S::Paused),
            S::AwaitingApproval => matches!(next, S::Acting | S::ExecutingChange | S::Reasoning),
            S::AwaitingUser => matches!(next, S::Reasoning),
            S::Diagnosed => matches!(
                next,
                S::PlanningChange | S::Reasoning | S::Completed | S::Paused
            ),
            S::PlanningChange => matches!(
                next,
                S::AwaitingApproval | S::ExecutingChange | S::Reasoning | S::Paused
            ),
            S::ExecutingChange => matches!(next, S::Verifying | S::RollingBack | S::Reasoning),
            S::Verifying => matches!(next, S::Completed | S::Reasoning | S::RollingBack),
            S::RollingBack => matches!(next, S::Reasoning | S::Completed),
            S::Paused => matches!(next, S::Running | S::Reasoning),
            S::Completed | S::Failed | S::Cancelled => false,
        }
    }

    /// Validated transition. Invalid moves return a stable typed error and
    /// never panic.
    pub fn transition_to(self, next: AgentRunStateV2) -> Result<AgentRunStateV2, AgentStateError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(AgentStateError {
                from: self,
                to: next,
            })
        }
    }
}

/// Rejected state transition. Carries only the two states involved; state
/// names contain no user or server data, so the error is safe to log and to
/// return across IPC.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("invalid agent run state transition: {from:?} -> {to:?}")]
pub struct AgentStateError {
    pub from: AgentRunStateV2,
    pub to: AgentRunStateV2,
}

impl AgentStateError {
    /// Stable machine-readable code, aligned with the project error pattern.
    pub fn code(&self) -> &'static str {
        AGENT_INVALID_TRANSITION
    }
}

#[cfg(test)]
mod tests {
    use super::AgentRunStateV2 as S;
    use super::*;

    #[test]
    fn authoritative_state_count_is_sixteen() {
        assert_eq!(S::ALL.len(), 16);
    }

    #[test]
    fn every_state_has_exactly_one_classification() {
        for state in S::ALL {
            let classes = [
                state.is_terminal(),
                state.is_interrupt(),
                state.is_active(),
                state == S::Created,
            ];
            let count = classes.iter().filter(|is_in| **is_in).count();
            assert_eq!(count, 1, "state {state:?} must be in exactly one class");
        }
    }

    #[test]
    fn only_interrupt_states_can_resume() {
        for state in S::ALL {
            assert_eq!(state.can_resume(), state.is_interrupt());
        }
    }

    #[test]
    fn created_running_reasoning_is_the_valid_start_path() {
        assert_eq!(S::Created.transition_to(S::Running), Ok(S::Running));
        assert_eq!(S::Running.transition_to(S::Reasoning), Ok(S::Reasoning));
    }

    #[test]
    fn created_cannot_jump_to_completed() {
        let error = S::Created
            .transition_to(S::Completed)
            .expect_err("Created -> Completed must be rejected");
        assert_eq!(error.code(), AGENT_INVALID_TRANSITION);
        assert_eq!(error.from, S::Created);
        assert_eq!(error.to, S::Completed);
    }

    #[test]
    fn terminal_states_reject_every_outgoing_transition() {
        for terminal in [S::Completed, S::Failed, S::Cancelled] {
            for next in S::ALL {
                assert!(
                    terminal.transition_to(next).is_err(),
                    "{terminal:?} -> {next:?} must be rejected"
                );
            }
        }
    }

    #[test]
    fn cancel_and_fail_are_reachable_from_every_non_terminal_state() {
        for state in S::ALL {
            if state.is_terminal() {
                continue;
            }
            assert!(
                state.can_transition_to(S::Cancelled),
                "{state:?} -> Cancelled"
            );
            assert!(state.can_transition_to(S::Failed), "{state:?} -> Failed");
        }
    }

    #[test]
    fn approval_interrupt_round_trip_is_valid() {
        assert!(S::Acting.can_transition_to(S::AwaitingApproval));
        assert!(S::AwaitingApproval.can_transition_to(S::Acting));
        // Approved ChangeSet resumes directly into execution.
        assert!(S::AwaitingApproval.can_transition_to(S::ExecutingChange));
        // Rejection becomes an observation; the run reasons again.
        assert!(S::AwaitingApproval.can_transition_to(S::Reasoning));
    }

    #[test]
    fn awaiting_user_round_trip_is_valid() {
        assert!(S::Reasoning.can_transition_to(S::AwaitingUser));
        assert!(S::AwaitingUser.can_transition_to(S::Reasoning));
        // AwaitingUser must not skip reasoning straight into action.
        assert!(!S::AwaitingUser.can_transition_to(S::Acting));
    }

    #[test]
    fn tool_failure_recovery_path_is_expressible() {
        // ToolCall failure is an observation, not a run failure:
        // Acting -> Observing -> Reasoning must be a legal loop.
        assert_eq!(S::Acting.transition_to(S::Observing), Ok(S::Observing));
        assert_eq!(S::Observing.transition_to(S::Reasoning), Ok(S::Reasoning));
    }

    #[test]
    fn change_execution_flow_is_expressible() {
        assert!(S::Reasoning.can_transition_to(S::PlanningChange));
        assert!(S::PlanningChange.can_transition_to(S::AwaitingApproval));
        assert!(S::AwaitingApproval.can_transition_to(S::ExecutingChange));
        assert!(S::ExecutingChange.can_transition_to(S::Verifying));
        assert!(S::Verifying.can_transition_to(S::Completed));
        assert!(S::Verifying.can_transition_to(S::RollingBack));
        assert!(S::RollingBack.can_transition_to(S::Reasoning));
    }

    #[test]
    fn pause_is_rejected_while_side_effects_are_in_flight() {
        for state in [S::ExecutingChange, S::Verifying, S::RollingBack] {
            assert!(
                !state.can_transition_to(S::Paused),
                "{state:?} -> Paused must be rejected"
            );
        }
        for state in [S::Running, S::Reasoning, S::Acting, S::Observing] {
            assert!(state.can_transition_to(S::Paused), "{state:?} -> Paused");
        }
    }

    #[test]
    fn paused_resumes_only_into_running_reasoning_or_terminates() {
        assert!(S::Paused.can_transition_to(S::Running));
        assert!(S::Paused.can_transition_to(S::Reasoning));
        assert!(S::Paused.can_transition_to(S::Cancelled));
        assert!(!S::Paused.can_transition_to(S::Acting));
        assert!(!S::Paused.can_transition_to(S::Completed));
    }

    #[test]
    fn serialization_uses_stable_snake_case_names() {
        let json = |state: S| serde_json::to_string(&state).expect("state serializes");
        assert_eq!(json(S::AwaitingApproval), "\"awaiting_approval\"");
        assert_eq!(json(S::AwaitingUser), "\"awaiting_user\"");
        assert_eq!(json(S::PlanningChange), "\"planning_change\"");
        assert_eq!(json(S::ExecutingChange), "\"executing_change\"");
        assert_eq!(json(S::RollingBack), "\"rolling_back\"");
    }

    #[test]
    fn serialization_round_trips_every_state() {
        for state in S::ALL {
            let json = serde_json::to_string(&state).expect("state serializes");
            let back: S = serde_json::from_str(&json).expect("state deserializes");
            assert_eq!(back, state);
        }
    }
}
