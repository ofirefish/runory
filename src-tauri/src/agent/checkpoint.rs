//! Runtime V2 `AgentCheckpoint` model (AR2-C defines, AR2-D persists).
//!
//! A checkpoint captures the minimum needed to resume a run after restart:
//! where the event log ends, which interrupt is pending, and how much budget
//! is consumed. It deliberately contains no credentials, no raw tool
//! arguments and no model reasoning — the pending approval is referenced by
//! id and the exact action binding lives in the `ApprovalRequest`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::state::AgentRunStateV2;

/// Reference to the pending interrupt, checkpoint-safe by construction.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum PendingInterruptRef {
    Approval { approval_id: Uuid },
    UserInput { question: String },
}

/// Consumed budget at checkpoint time.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BudgetCheckpoint {
    pub rounds_used: u32,
    pub tool_calls_used: u32,
    pub elapsed_ms: u64,
}

/// Serializable resume point for one run.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AgentCheckpoint {
    pub run_id: Uuid,
    pub state: AgentRunStateV2,
    /// Sequence of the last emitted event; replay resumes after this.
    pub event_cursor: u64,
    pub pending_interrupt: Option<PendingInterruptRef>,
    pub budget: BudgetCheckpoint,
    pub target_ids: Vec<Uuid>,
    /// Compact working-fact snapshot (JSON array). No secrets, no raw logs.
    #[serde(default)]
    pub fact_snapshot: String,
    pub created_at_epoch_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkpoint() -> AgentCheckpoint {
        AgentCheckpoint {
            run_id: Uuid::new_v4(),
            state: AgentRunStateV2::AwaitingApproval,
            event_cursor: 12,
            pending_interrupt: Some(PendingInterruptRef::Approval {
                approval_id: Uuid::new_v4(),
            }),
            budget: BudgetCheckpoint {
                rounds_used: 3,
                tool_calls_used: 5,
                elapsed_ms: 42_000,
            },
            target_ids: vec![Uuid::new_v4()],
            fact_snapshot: "[]".into(),
            created_at_epoch_ms: 1_725_000_000_000,
        }
    }

    #[test]
    fn checkpoint_round_trips_with_stable_names() {
        let original = checkpoint();
        let json = serde_json::to_string(&original).expect("checkpoint serializes");
        let back: AgentCheckpoint = serde_json::from_str(&json).expect("checkpoint deserializes");
        assert_eq!(back, original);
        assert!(json.contains("\"state\":\"awaiting_approval\""));
        assert!(json.contains("\"type\":\"approval\""));
    }

    #[test]
    fn checkpoint_never_contains_secret_or_reasoning_fields() {
        let json = serde_json::to_string(&checkpoint()).expect("checkpoint serializes");
        for forbidden in [
            "password",
            "passphrase",
            "private_key",
            "vault",
            "chain_of_thought",
            "arguments\":",
        ] {
            assert!(
                !json.contains(forbidden),
                "checkpoint must not carry {forbidden}"
            );
        }
    }
}
