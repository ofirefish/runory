//! Runtime V2 persistence abstractions and in-memory implementations.
//!
//! The traits are the contract AR2-D will implement on SQLite
//! (`runory-agent.db`). The in-memory implementations exist for unit tests and
//! the AR2-B controller prototype; they are thread-safe (`RwLock`) but bring
//! no durability. Nothing here persists secrets: runs and events already
//! exclude credentials and private reasoning by construction.

use std::collections::HashMap;
use std::sync::RwLock;
use thiserror::Error;
use uuid::Uuid;

use super::approval::ApprovalRequest;
use super::checkpoint::AgentCheckpoint;
use super::decision::{PreparedCommandProposal, PreparedToolCall};
use super::event::AgentEventEnvelope;
use super::run::AgentRun;

/// Stable error codes for the Runtime V2 store boundary.
pub const AGENT_RUN_NOT_FOUND: &str = "AGENT_RUN_NOT_FOUND";
pub const AGENT_RUN_ALREADY_EXISTS: &str = "AGENT_RUN_ALREADY_EXISTS";
pub const AGENT_EVENT_SEQUENCE_INVALID: &str = "AGENT_EVENT_SEQUENCE_INVALID";
pub const AGENT_APPROVAL_NOT_FOUND: &str = "AGENT_APPROVAL_NOT_FOUND";
pub const AGENT_STORE_CORRUPT: &str = "AGENT_STORE_CORRUPT";
pub const AGENT_STORE_MIGRATION_FAILED: &str = "AGENT_STORE_MIGRATION_FAILED";
const AGENT_STORE_UNAVAILABLE: &str = "AGENT_STORE_UNAVAILABLE";
pub const AGENT_PERSISTENCE_FAILED: &str = "AGENT_PERSISTENCE_FAILED";

/// Typed store errors. Sequence errors carry only numbers — no payload data.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AgentStoreError {
    #[error("agent run not found")]
    RunNotFound,
    #[error("agent run already exists")]
    RunAlreadyExists,
    #[error("agent event sequence invalid: last persisted {last}, attempted {attempted}")]
    EventSequenceInvalid { last: u64, attempted: u64 },
    #[error("agent approval not found")]
    ApprovalNotFound,
    #[error("agent store corrupt")]
    StoreCorrupt,
    #[error("agent store migration failed")]
    MigrationFailed,
    #[error("agent persistence failed")]
    PersistenceFailed,
    /// A poisoned lock (a panic while writing) makes the store fail closed
    /// instead of serving possibly inconsistent state.
    #[error("agent store unavailable")]
    StoreUnavailable,
}

impl AgentStoreError {
    pub fn code(&self) -> &'static str {
        match self {
            AgentStoreError::RunNotFound => AGENT_RUN_NOT_FOUND,
            AgentStoreError::RunAlreadyExists => AGENT_RUN_ALREADY_EXISTS,
            AgentStoreError::EventSequenceInvalid { .. } => AGENT_EVENT_SEQUENCE_INVALID,
            AgentStoreError::ApprovalNotFound => AGENT_APPROVAL_NOT_FOUND,
            AgentStoreError::StoreCorrupt => AGENT_STORE_CORRUPT,
            AgentStoreError::MigrationFailed => AGENT_STORE_MIGRATION_FAILED,
            AgentStoreError::PersistenceFailed => AGENT_PERSISTENCE_FAILED,
            AgentStoreError::StoreUnavailable => AGENT_STORE_UNAVAILABLE,
        }
    }
}

/// Store for `AgentRun` records. AR2-D adds the SQLite implementation behind
/// this same trait.
pub trait AgentRunStore: Send + Sync {
    /// Insert a new run; rejects an already-known run id.
    fn insert(&self, run: AgentRun) -> Result<(), AgentStoreError>;
    /// Load a run by id.
    fn get(&self, run_id: Uuid) -> Result<Option<AgentRun>, AgentStoreError>;
    /// Persist the current state of an existing run; rejects unknown runs so
    /// updates can never silently become inserts.
    fn save(&self, run: &AgentRun) -> Result<(), AgentStoreError>;
    /// Persist content-free observability metrics at run completion.
    fn save_metrics(&self, run_id: Uuid, metrics_json: &str) -> Result<(), AgentStoreError> {
        let _ = (run_id, metrics_json);
        Ok(())
    }
}

/// Append-only event log with strictly increasing per-run sequence numbers.
pub trait AgentEventRepository: Send + Sync {
    /// Append one event. The envelope's `seq` must be strictly greater than
    /// the last persisted `seq` for the same run; violations are rejected
    /// (`AGENT_EVENT_SEQUENCE_INVALID`), never silently reordered.
    fn append(&self, envelope: AgentEventEnvelope) -> Result<(), AgentStoreError>;
    /// Events with `seq > after_seq`, in ascending order (replay/streaming).
    fn events_after(
        &self,
        run_id: Uuid,
        after_seq: u64,
    ) -> Result<Vec<AgentEventEnvelope>, AgentStoreError>;
    /// Full ordered event log for one run.
    fn all_events(&self, run_id: Uuid) -> Result<Vec<AgentEventEnvelope>, AgentStoreError>;
}

/// Durable approval records for interrupt/resume (AR2-C).
pub trait ApprovalStore: Send + Sync {
    fn insert(&self, approval: ApprovalRequest) -> Result<(), AgentStoreError>;
    fn get(&self, approval_id: Uuid) -> Result<Option<ApprovalRequest>, AgentStoreError>;
    fn save(&self, approval: &ApprovalRequest) -> Result<(), AgentStoreError>;
    fn pending_for_run(&self, run_id: Uuid) -> Result<Option<ApprovalRequest>, AgentStoreError>;
}

/// Resume checkpoints (AR2-C defines, AR2-D persists).
pub trait CheckpointStore: Send + Sync {
    fn save(&self, checkpoint: AgentCheckpoint) -> Result<(), AgentStoreError>;
    fn get(&self, run_id: Uuid) -> Result<Option<AgentCheckpoint>, AgentStoreError>;
}

/// Pending validated calls awaiting approval. The binding hash lives on
/// `ApprovalRequest`; the typed invocation is stored here only until the
/// user decides — never in the event log.
pub trait PendingToolCallStore: Send + Sync {
    fn put(&self, call: PreparedToolCall) -> Result<(), AgentStoreError>;
    fn get(&self, tool_call_id: Uuid) -> Result<Option<PreparedToolCall>, AgentStoreError>;
    fn remove(&self, tool_call_id: Uuid) -> Result<(), AgentStoreError>;
    fn put_command(&self, command: PreparedCommandProposal) -> Result<(), AgentStoreError>;
    fn get_command(
        &self,
        command_id: Uuid,
    ) -> Result<Option<PreparedCommandProposal>, AgentStoreError>;
    fn remove_command(&self, command_id: Uuid) -> Result<(), AgentStoreError>;
}

/// Thread-safe in-memory `AgentRunStore`.
#[derive(Default)]
pub struct InMemoryAgentRunStore {
    runs: RwLock<HashMap<Uuid, AgentRun>>,
}

impl AgentRunStore for InMemoryAgentRunStore {
    fn insert(&self, run: AgentRun) -> Result<(), AgentStoreError> {
        let mut runs = self
            .runs
            .write()
            .map_err(|_| AgentStoreError::StoreUnavailable)?;
        if runs.contains_key(&run.id()) {
            return Err(AgentStoreError::RunAlreadyExists);
        }
        runs.insert(run.id(), run);
        Ok(())
    }

    fn get(&self, run_id: Uuid) -> Result<Option<AgentRun>, AgentStoreError> {
        let runs = self
            .runs
            .read()
            .map_err(|_| AgentStoreError::StoreUnavailable)?;
        Ok(runs.get(&run_id).cloned())
    }

    fn save(&self, run: &AgentRun) -> Result<(), AgentStoreError> {
        let mut runs = self
            .runs
            .write()
            .map_err(|_| AgentStoreError::StoreUnavailable)?;
        if !runs.contains_key(&run.id()) {
            return Err(AgentStoreError::RunNotFound);
        }
        runs.insert(run.id(), run.clone());
        Ok(())
    }
}

/// Thread-safe in-memory `AgentEventRepository` keeping one ordered event list
/// per run.
#[derive(Default)]
pub struct InMemoryAgentEventRepository {
    events: RwLock<HashMap<Uuid, Vec<AgentEventEnvelope>>>,
}

impl AgentEventRepository for InMemoryAgentEventRepository {
    fn append(&self, envelope: AgentEventEnvelope) -> Result<(), AgentStoreError> {
        let mut events = self
            .events
            .write()
            .map_err(|_| AgentStoreError::StoreUnavailable)?;
        let log = events.entry(envelope.run_id).or_default();
        let last = log.last().map(|event| event.seq).unwrap_or(0);
        if envelope.seq <= last {
            return Err(AgentStoreError::EventSequenceInvalid {
                last,
                attempted: envelope.seq,
            });
        }
        log.push(envelope);
        Ok(())
    }

    fn events_after(
        &self,
        run_id: Uuid,
        after_seq: u64,
    ) -> Result<Vec<AgentEventEnvelope>, AgentStoreError> {
        let events = self
            .events
            .read()
            .map_err(|_| AgentStoreError::StoreUnavailable)?;
        Ok(events
            .get(&run_id)
            .map(|log| {
                log.iter()
                    .filter(|event| event.seq > after_seq)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    fn all_events(&self, run_id: Uuid) -> Result<Vec<AgentEventEnvelope>, AgentStoreError> {
        self.events_after(run_id, 0)
    }
}

#[derive(Default)]
pub struct InMemoryApprovalStore {
    approvals: RwLock<HashMap<Uuid, ApprovalRequest>>,
}

impl ApprovalStore for InMemoryApprovalStore {
    fn insert(&self, approval: ApprovalRequest) -> Result<(), AgentStoreError> {
        let mut store = self
            .approvals
            .write()
            .map_err(|_| AgentStoreError::StoreUnavailable)?;
        store.insert(approval.id, approval);
        Ok(())
    }

    fn get(&self, approval_id: Uuid) -> Result<Option<ApprovalRequest>, AgentStoreError> {
        let store = self
            .approvals
            .read()
            .map_err(|_| AgentStoreError::StoreUnavailable)?;
        Ok(store.get(&approval_id).cloned())
    }

    fn save(&self, approval: &ApprovalRequest) -> Result<(), AgentStoreError> {
        let mut store = self
            .approvals
            .write()
            .map_err(|_| AgentStoreError::StoreUnavailable)?;
        if !store.contains_key(&approval.id) {
            return Err(AgentStoreError::ApprovalNotFound);
        }
        store.insert(approval.id, approval.clone());
        Ok(())
    }

    fn pending_for_run(&self, run_id: Uuid) -> Result<Option<ApprovalRequest>, AgentStoreError> {
        use super::approval::ApprovalRequestState;
        let store = self
            .approvals
            .read()
            .map_err(|_| AgentStoreError::StoreUnavailable)?;
        Ok(store
            .values()
            .find(|approval| {
                approval.run_id == run_id && approval.state == ApprovalRequestState::Pending
            })
            .cloned())
    }
}

#[derive(Default)]
pub struct InMemoryCheckpointStore {
    checkpoints: RwLock<HashMap<Uuid, AgentCheckpoint>>,
}

impl CheckpointStore for InMemoryCheckpointStore {
    fn save(&self, checkpoint: AgentCheckpoint) -> Result<(), AgentStoreError> {
        let mut store = self
            .checkpoints
            .write()
            .map_err(|_| AgentStoreError::StoreUnavailable)?;
        store.insert(checkpoint.run_id, checkpoint);
        Ok(())
    }

    fn get(&self, run_id: Uuid) -> Result<Option<AgentCheckpoint>, AgentStoreError> {
        let store = self
            .checkpoints
            .read()
            .map_err(|_| AgentStoreError::StoreUnavailable)?;
        Ok(store.get(&run_id).cloned())
    }
}

#[derive(Default)]
pub struct InMemoryPendingToolCallStore {
    calls: RwLock<HashMap<Uuid, PreparedToolCall>>,
    commands: RwLock<HashMap<Uuid, PreparedCommandProposal>>,
}

impl PendingToolCallStore for InMemoryPendingToolCallStore {
    fn put(&self, call: PreparedToolCall) -> Result<(), AgentStoreError> {
        let mut store = self
            .calls
            .write()
            .map_err(|_| AgentStoreError::StoreUnavailable)?;
        store.insert(call.tool_call_id, call);
        Ok(())
    }

    fn get(&self, tool_call_id: Uuid) -> Result<Option<PreparedToolCall>, AgentStoreError> {
        let store = self
            .calls
            .read()
            .map_err(|_| AgentStoreError::StoreUnavailable)?;
        Ok(store.get(&tool_call_id).cloned())
    }

    fn remove(&self, tool_call_id: Uuid) -> Result<(), AgentStoreError> {
        let mut store = self
            .calls
            .write()
            .map_err(|_| AgentStoreError::StoreUnavailable)?;
        store.remove(&tool_call_id);
        Ok(())
    }

    fn put_command(&self, command: PreparedCommandProposal) -> Result<(), AgentStoreError> {
        self.commands
            .write()
            .map_err(|_| AgentStoreError::StoreUnavailable)?
            .insert(command.command_id, command);
        Ok(())
    }

    fn get_command(
        &self,
        command_id: Uuid,
    ) -> Result<Option<PreparedCommandProposal>, AgentStoreError> {
        Ok(self
            .commands
            .read()
            .map_err(|_| AgentStoreError::StoreUnavailable)?
            .get(&command_id)
            .cloned())
    }

    fn remove_command(&self, command_id: Uuid) -> Result<(), AgentStoreError> {
        self.commands
            .write()
            .map_err(|_| AgentStoreError::StoreUnavailable)?
            .remove(&command_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::event::AgentEvent;
    use crate::agent::state::AgentRunStateV2;
    use std::sync::Arc;

    fn envelope(run_id: Uuid, seq: u64) -> AgentEventEnvelope {
        AgentEventEnvelope {
            run_id,
            seq,
            timestamp_epoch_ms: 1_725_000_000_000 + seq,
            event: AgentEvent::ProgressUpdated {
                summary: format!("step {seq}"),
            },
        }
    }

    #[test]
    fn run_store_inserts_gets_and_saves() {
        let store = InMemoryAgentRunStore::default();
        let mut run = AgentRun::with_created_at(Uuid::new_v4(), 1_000);
        let run_id = run.id();
        store.insert(run.clone()).expect("insert");

        run.transition_to(AgentRunStateV2::Running)
            .expect("Created -> Running");
        store.save(&run).expect("save");

        let loaded = store.get(run_id).expect("get").expect("run exists");
        assert_eq!(loaded.state(), AgentRunStateV2::Running);
    }

    #[test]
    fn run_store_rejects_duplicate_insert_and_unknown_save() {
        let store = InMemoryAgentRunStore::default();
        let run = AgentRun::with_created_at(Uuid::new_v4(), 1_000);
        store.insert(run.clone()).expect("first insert");

        let duplicate = store.insert(run.clone()).expect_err("duplicate insert");
        assert_eq!(duplicate.code(), AGENT_RUN_ALREADY_EXISTS);

        let unknown = AgentRun::with_created_at(Uuid::new_v4(), 1_000);
        let missing = store.save(&unknown).expect_err("save of unknown run");
        assert_eq!(missing.code(), AGENT_RUN_NOT_FOUND);

        assert!(store.get(unknown.id()).expect("get").is_none());
    }

    #[test]
    fn append_keeps_sequences_strictly_increasing() {
        let repository = InMemoryAgentEventRepository::default();
        let run_id = Uuid::new_v4();
        for seq in 1..=5 {
            repository.append(envelope(run_id, seq)).expect("append");
        }
        let all = repository.all_events(run_id).expect("all_events");
        let sequences: Vec<u64> = all.iter().map(|event| event.seq).collect();
        assert_eq!(sequences, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn append_rejects_duplicate_and_out_of_order_sequences() {
        let repository = InMemoryAgentEventRepository::default();
        let run_id = Uuid::new_v4();
        repository.append(envelope(run_id, 3)).expect("seq 3");

        let duplicate = repository
            .append(envelope(run_id, 3))
            .expect_err("duplicate seq");
        assert_eq!(duplicate.code(), AGENT_EVENT_SEQUENCE_INVALID);

        let out_of_order = repository
            .append(envelope(run_id, 2))
            .expect_err("out-of-order seq");
        assert_eq!(
            out_of_order,
            AgentStoreError::EventSequenceInvalid {
                last: 3,
                attempted: 2
            }
        );

        // Zero is never a valid sequence number.
        let zero = repository
            .append(envelope(run_id, 0))
            .expect_err("seq zero");
        assert_eq!(zero.code(), AGENT_EVENT_SEQUENCE_INVALID);

        // The log is unchanged after rejections.
        let all = repository.all_events(run_id).expect("all_events");
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn events_after_returns_only_later_events_in_order() {
        let repository = InMemoryAgentEventRepository::default();
        let run_id = Uuid::new_v4();
        for seq in 1..=15 {
            repository.append(envelope(run_id, seq)).expect("append");
        }
        let tail = repository.events_after(run_id, 10).expect("events_after");
        let sequences: Vec<u64> = tail.iter().map(|event| event.seq).collect();
        assert_eq!(sequences, vec![11, 12, 13, 14, 15]);

        let unknown = repository
            .events_after(Uuid::new_v4(), 0)
            .expect("unknown run yields empty log");
        assert!(unknown.is_empty());
    }

    #[test]
    fn runs_keep_independent_sequences() {
        let repository = InMemoryAgentEventRepository::default();
        let run_a = Uuid::new_v4();
        let run_b = Uuid::new_v4();

        repository.append(envelope(run_a, 1)).expect("a1");
        repository.append(envelope(run_b, 1)).expect("b1");
        repository.append(envelope(run_a, 2)).expect("a2");
        repository.append(envelope(run_b, 2)).expect("b2");

        assert_eq!(repository.all_events(run_a).expect("a").len(), 2);
        assert_eq!(repository.all_events(run_b).expect("b").len(), 2);
    }

    #[test]
    fn concurrent_appends_to_distinct_runs_stay_consistent() {
        let repository = Arc::new(InMemoryAgentEventRepository::default());
        let runs: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();

        let handles: Vec<_> = runs
            .iter()
            .map(|run_id| {
                let repository = Arc::clone(&repository);
                let run_id = *run_id;
                std::thread::spawn(move || {
                    for seq in 1..=100 {
                        repository
                            .append(envelope(run_id, seq))
                            .expect("concurrent append");
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().expect("thread completes");
        }

        for run_id in runs {
            let all = repository.all_events(run_id).expect("all_events");
            let sequences: Vec<u64> = all.iter().map(|event| event.seq).collect();
            let expected: Vec<u64> = (1..=100).collect();
            assert_eq!(sequences, expected);
        }
    }
}
