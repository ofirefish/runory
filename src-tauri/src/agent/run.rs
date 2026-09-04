//! Runtime V2 `AgentRun` domain object.
//!
//! Distinct from the legacy `agentic::state::AgentRun` (doctor runtime), which
//! remains untouched. This object owns the two invariants every later AR2
//! stage builds on: state changes only pass through validated transitions,
//! and event sequence numbers are allocated monotonically per run.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use super::state::{AgentRunStateV2, AgentStateError};

/// Runtime V2 agent run. Fields are private so callers cannot bypass
/// transition validation (`run.state = …` is impossible outside this module).
///
/// Serialization is the stable snake_case protocol reused later by SQLite
/// persistence and checkpoints. The run never carries credentials, secrets or
/// model private reasoning.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRun {
    id: Uuid,
    state: AgentRunStateV2,
    created_at_epoch_ms: u64,
    updated_at_epoch_ms: u64,
    next_event_seq: u64,
}

impl AgentRun {
    /// Create a run in `Created` state with the wall clock timestamp.
    pub fn new(id: Uuid) -> Self {
        Self::with_created_at(id, now_epoch_ms())
    }

    /// Create a run with an explicit creation timestamp (deterministic tests).
    pub fn with_created_at(id: Uuid, created_at_epoch_ms: u64) -> Self {
        Self {
            id,
            state: AgentRunStateV2::Created,
            created_at_epoch_ms,
            updated_at_epoch_ms: created_at_epoch_ms,
            next_event_seq: 1,
        }
    }

    /// Rebuild a run from durable storage. Used only by the persistence layer;
    /// the reconstructed state is trusted as previously validated.
    pub(crate) fn from_persisted(
        id: Uuid,
        state: AgentRunStateV2,
        created_at_epoch_ms: u64,
        updated_at_epoch_ms: u64,
        next_event_seq: u64,
    ) -> Self {
        Self {
            id,
            state,
            created_at_epoch_ms,
            updated_at_epoch_ms,
            next_event_seq: next_event_seq.max(1),
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn state(&self) -> AgentRunStateV2 {
        self.state
    }

    pub fn created_at_epoch_ms(&self) -> u64 {
        self.created_at_epoch_ms
    }

    pub fn updated_at_epoch_ms(&self) -> u64 {
        self.updated_at_epoch_ms
    }

    /// The sequence number the next emitted event will receive.
    pub fn peek_next_event_seq(&self) -> u64 {
        self.next_event_seq
    }

    /// Validated state transition; invalid moves leave the run unchanged and
    /// return the stable `AGENT_INVALID_TRANSITION` error.
    pub fn transition_to(&mut self, next: AgentRunStateV2) -> Result<(), AgentStateError> {
        self.state = self.state.transition_to(next)?;
        // Monotonic even if the wall clock moves backwards.
        self.updated_at_epoch_ms = now_epoch_ms().max(self.updated_at_epoch_ms);
        Ok(())
    }

    /// A new explicit user turn may reopen a finished conversation. Runtime
    /// transitions remain terminal; this does not restore any action approval.
    pub(crate) fn begin_user_turn(&mut self) -> Result<(), AgentStateError> {
        if !self.state.is_terminal() {
            return Err(AgentStateError {
                from: self.state,
                to: AgentRunStateV2::Reasoning,
            });
        }
        self.state = AgentRunStateV2::Reasoning;
        self.updated_at_epoch_ms = now_epoch_ms().max(self.updated_at_epoch_ms);
        Ok(())
    }

    /// Allocate the next per-run monotonic event sequence number (1, 2, 3, …).
    ///
    /// Allocation requires `&mut self`, so Rust's aliasing rules make
    /// duplicates impossible for a single run; concurrent runtimes must hold
    /// the run's write lock while allocating and appending (AR2-B concern).
    pub fn next_event_sequence(&mut self) -> u64 {
        let seq = self.next_event_seq;
        self.next_event_seq = self.next_event_seq.saturating_add(1);
        seq
    }
}

pub(super) fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::state::AGENT_INVALID_TRANSITION;

    fn run() -> AgentRun {
        AgentRun::with_created_at(Uuid::new_v4(), 1_000)
    }

    #[test]
    fn a_new_run_starts_in_created_with_seq_one() {
        let run = run();
        assert_eq!(run.state(), AgentRunStateV2::Created);
        assert_eq!(run.peek_next_event_seq(), 1);
        assert_eq!(run.created_at_epoch_ms(), 1_000);
        assert_eq!(run.updated_at_epoch_ms(), 1_000);
    }

    #[test]
    fn valid_transitions_advance_the_run() {
        let mut run = run();
        run.transition_to(AgentRunStateV2::Running)
            .expect("Created -> Running");
        run.transition_to(AgentRunStateV2::Reasoning)
            .expect("Running -> Reasoning");
        assert_eq!(run.state(), AgentRunStateV2::Reasoning);
        assert!(run.updated_at_epoch_ms() >= run.created_at_epoch_ms());
    }

    #[test]
    fn invalid_transitions_leave_the_run_unchanged() {
        let mut run = run();
        let error = run
            .transition_to(AgentRunStateV2::Completed)
            .expect_err("Created -> Completed must fail");
        assert_eq!(error.code(), AGENT_INVALID_TRANSITION);
        assert_eq!(run.state(), AgentRunStateV2::Created);
    }

    #[test]
    fn terminal_runs_reject_reactivation() {
        let mut run = run();
        run.transition_to(AgentRunStateV2::Cancelled)
            .expect("Created -> Cancelled");
        for next in AgentRunStateV2::ALL {
            assert!(run.transition_to(next).is_err());
        }
        assert_eq!(run.state(), AgentRunStateV2::Cancelled);
    }

    #[test]
    fn event_sequence_is_monotonic_from_one() {
        let mut run = run();
        assert_eq!(run.next_event_sequence(), 1);
        assert_eq!(run.next_event_sequence(), 2);
        assert_eq!(run.next_event_sequence(), 3);
        assert_eq!(run.peek_next_event_seq(), 4);
    }

    #[test]
    fn event_sequences_never_repeat_over_many_allocations() {
        let mut run = run();
        let mut previous = 0;
        for _ in 0..10_000 {
            let seq = run.next_event_sequence();
            assert!(seq > previous, "sequence must strictly increase");
            previous = seq;
        }
    }

    #[test]
    fn serialization_round_trips_the_full_run() {
        let mut run = run();
        run.transition_to(AgentRunStateV2::Running)
            .expect("Created -> Running");
        let _ = run.next_event_sequence();
        let json = serde_json::to_string(&run).expect("run serializes");
        let back: AgentRun = serde_json::from_str(&json).expect("run deserializes");
        assert_eq!(back, run);
        assert_eq!(back.state(), AgentRunStateV2::Running);
        assert_eq!(back.peek_next_event_seq(), 2);
    }

    #[test]
    fn serialization_uses_stable_field_names() {
        let run = run();
        let value = serde_json::to_value(&run).expect("run serializes");
        let object = value.as_object().expect("run is a json object");
        for key in [
            "id",
            "state",
            "createdAtEpochMs",
            "updatedAtEpochMs",
            "nextEventSeq",
        ] {
            assert!(object.contains_key(key), "missing stable field {key}");
        }
        assert_eq!(object.len(), 5, "no undocumented fields in the protocol");
    }
}
