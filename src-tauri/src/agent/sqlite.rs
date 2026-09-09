//! Runtime V2 SQLite persistence (`runory-agent.db`) — AR2-D.
//!
//! Implements the repository traits over a bundled `rusqlite` database with
//! schema versioning, transactional interrupt bundles, and conservative
//! startup recovery. Nothing here auto-executes a pending action after
//! reopen: discovery returns resumable runs and leaves approval revalidation
//! to the controller.
//!
//! Secrets (SSH password, private keys, passphrase, vault/cloud secrets) and
//! model private reasoning are never persisted — event payloads and pending
//! call JSON pass through `redact_secrets` before write.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::de::DeserializeOwned;
use serde::Serialize;
use uuid::Uuid;

use super::approval::{ApprovalRequest, ApprovalRequestState};
use super::checkpoint::AgentCheckpoint;
use super::decision::{PreparedCommandProposal, PreparedToolCall};
use super::event::{AgentEvent, AgentEventEnvelope};
use super::fleet_run::{
    FleetChildRun, FleetRecoveryStateV2, FleetRunStateV2, FleetRunStore, FleetRunV2, FleetStage,
};
use super::fleet_target::FleetTargetBinding;
use super::repository::{
    AgentEventRepository, AgentRunStore, AgentStoreError, ApprovalStore, CheckpointStore,
    PendingToolCallStore,
};
use super::run::AgentRun;
use super::state::AgentRunStateV2;
use crate::agentic::context::redact_secrets;

/// Current schema version written by this build.
pub const SCHEMA_VERSION: i64 = 3;

const EMPTY_JSON_OBJECT: &str = "{}";
const EMPTY_JSON_ARRAY: &str = "[]";

#[path = "sqlite_history.rs"]
mod history;
pub(crate) use history::{AgentHistoryDetail, AgentHistoryEntry};

/// One durable Agent store backed by SQLite.
pub struct SqliteAgentDatabase {
    path: PathBuf,
    conn: Mutex<Connection>,
}

/// A non-terminal run discovered at startup (never auto-executed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveredRun {
    pub run_id: Uuid,
    pub state: AgentRunStateV2,
    /// True when an active mid-loop state was conservatively moved to Paused.
    pub interrupted: bool,
}

impl SqliteAgentDatabase {
    /// Open (or create) `runory-agent.db` at `path` and migrate.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, AgentStoreError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| AgentStoreError::PersistenceFailed)?;
        }
        let conn = Connection::open(&path).map_err(|_| AgentStoreError::PersistenceFailed)?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;",
        )
        .map_err(|_| AgentStoreError::PersistenceFailed)?;
        let db = Self {
            path,
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    /// In-memory database for unit tests.
    pub fn open_in_memory() -> Result<Self, AgentStoreError> {
        let conn = Connection::open_in_memory().map_err(|_| AgentStoreError::PersistenceFailed)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|_| AgentStoreError::PersistenceFailed)?;
        let db = Self {
            path: PathBuf::from(":memory:"),
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn with_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, AgentStoreError>,
    ) -> Result<T, AgentStoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AgentStoreError::StoreUnavailable)?;
        f(&conn)
    }

    fn with_tx<T>(
        &self,
        f: impl FnOnce(&Transaction<'_>) -> Result<T, AgentStoreError>,
    ) -> Result<T, AgentStoreError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| AgentStoreError::StoreUnavailable)?;
        let tx = conn
            .transaction()
            .map_err(|_| AgentStoreError::PersistenceFailed)?;
        let result = f(&tx)?;
        tx.commit()
            .map_err(|_| AgentStoreError::PersistenceFailed)?;
        Ok(result)
    }

    fn migrate(&self) -> Result<(), AgentStoreError> {
        self.with_conn(|conn| {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_version (
                    version INTEGER NOT NULL
                 );",
            )
            .map_err(|_| AgentStoreError::MigrationFailed)?;

            let version: Option<i64> = conn
                .query_row(
                    "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| AgentStoreError::StoreCorrupt)?;

            match version {
                None => {
                    apply_v1(conn)?;
                    apply_v3(conn)?;
                    conn.execute(
                        "INSERT INTO schema_version (version) VALUES (?1)",
                        params![SCHEMA_VERSION],
                    )
                    .map_err(|_| AgentStoreError::MigrationFailed)?;
                    Ok(())
                }
                Some(v) if v == SCHEMA_VERSION => Ok(()),
                Some(v) if v < SCHEMA_VERSION => {
                    match v {
                        1 => {
                            apply_v2(conn)?;
                            apply_v3(conn)?;
                        }
                        2 => apply_v3(conn)?,
                        _ => return Err(AgentStoreError::MigrationFailed),
                    }
                    conn.execute(
                        "INSERT INTO schema_version (version) VALUES (?1)",
                        params![SCHEMA_VERSION],
                    )
                    .map_err(|_| AgentStoreError::MigrationFailed)?;
                    Ok(())
                }
                Some(_) => {
                    // Newer schema than this build understands — fail closed.
                    Err(AgentStoreError::MigrationFailed)
                }
            }
        })
    }

    /// Atomically persist run + events + checkpoint + approval (+ optional
    /// pending call) at an interrupt boundary.
    pub fn persist_interrupt_bundle(
        &self,
        run: &AgentRun,
        events: &[AgentEventEnvelope],
        checkpoint: &AgentCheckpoint,
        approval: Option<&ApprovalRequest>,
        pending_call: Option<&PreparedToolCall>,
    ) -> Result<(), AgentStoreError> {
        self.with_tx(|tx| {
            upsert_run(tx, run)?;
            for event in events {
                append_event(tx, event)?;
                project_event_side_tables(tx, event)?;
            }
            upsert_checkpoint(tx, checkpoint)?;
            if let Some(approval) = approval {
                upsert_approval(tx, approval)?;
            }
            if let Some(call) = pending_call {
                put_pending_call(tx, call)?;
            }
            Ok(())
        })
    }

    /// Startup recovery: keep interrupt states; move active mid-loop states
    /// to `Paused`. Never executes pending tools or auto-resumes.
    pub fn recover_on_startup(&self) -> Result<Vec<RecoveredRun>, AgentStoreError> {
        self.with_tx(|tx| {
            let mut stmt = tx
                .prepare("SELECT id, state FROM agent_runs")
                .map_err(|_| AgentStoreError::PersistenceFailed)?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|_| AgentStoreError::StoreCorrupt)?;

            let mut recovered = Vec::new();
            for row in rows {
                let (id_text, state_text) = row.map_err(|_| AgentStoreError::StoreCorrupt)?;
                let run_id = parse_uuid(&id_text)?;
                let state = parse_state(&state_text)?;
                if state.is_terminal() {
                    continue;
                }
                if state.is_interrupt() || state == AgentRunStateV2::Created {
                    recovered.push(RecoveredRun {
                        run_id,
                        state,
                        interrupted: false,
                    });
                    continue;
                }
                // Active / write-in-flight → conservative Paused(Interrupted).
                tx.execute(
                    "UPDATE agent_runs SET state = ?1, updated_at = ?2 WHERE id = ?3",
                    params![
                        state_name(AgentRunStateV2::Paused),
                        now_ms() as i64,
                        id_text
                    ],
                )
                .map_err(|_| AgentStoreError::PersistenceFailed)?;
                // Refresh checkpoint state if present.
                let _ = tx.execute(
                    "UPDATE agent_checkpoints SET state = ?1, updated_at = ?2 WHERE run_id = ?3",
                    params![
                        state_name(AgentRunStateV2::Paused),
                        now_ms() as i64,
                        id_text
                    ],
                );
                recovered.push(RecoveredRun {
                    run_id,
                    state: AgentRunStateV2::Paused,
                    interrupted: true,
                });
            }
            Ok(recovered)
        })
    }

    /// List non-terminal runs after recovery (resumable surface for UI/IPC).
    pub fn list_resumable_runs(&self) -> Result<Vec<AgentRun>, AgentStoreError> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, state, created_at, updated_at, next_event_seq
                     FROM agent_runs",
                )
                .map_err(|_| AgentStoreError::PersistenceFailed)?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                })
                .map_err(|_| AgentStoreError::StoreCorrupt)?;
            let mut runs = Vec::new();
            for row in rows {
                let (id, state, created, updated, seq) =
                    row.map_err(|_| AgentStoreError::StoreCorrupt)?;
                let run = AgentRun::from_persisted(
                    parse_uuid(&id)?,
                    parse_state(&state)?,
                    created as u64,
                    updated as u64,
                    seq as u64,
                );
                if !run.state().is_terminal() {
                    runs.push(run);
                }
            }
            Ok(runs)
        })
    }
}

fn apply_v1(conn: &Connection) -> Result<(), AgentStoreError> {
    conn.execute_batch(
        "
        CREATE TABLE agent_runs (
            id TEXT PRIMARY KEY NOT NULL,
            state TEXT NOT NULL,
            failure_code TEXT,
            next_event_seq INTEGER NOT NULL,
            goal_summary TEXT NOT NULL DEFAULT '',
            target_profile_ids TEXT NOT NULL DEFAULT '[]',
            budget_json TEXT NOT NULL DEFAULT '{}',
            metrics_json TEXT NOT NULL DEFAULT '{}',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE agent_events (
            run_id TEXT NOT NULL REFERENCES agent_runs(id),
            seq INTEGER NOT NULL,
            timestamp_ms INTEGER NOT NULL,
            event_json TEXT NOT NULL,
            PRIMARY KEY (run_id, seq)
        );

        CREATE TABLE agent_messages (
            id TEXT PRIMARY KEY NOT NULL,
            run_id TEXT NOT NULL REFERENCES agent_runs(id),
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE tool_calls (
            id TEXT PRIMARY KEY NOT NULL,
            run_id TEXT NOT NULL REFERENCES agent_runs(id),
            tool_name TEXT NOT NULL,
            sanitized_arguments TEXT NOT NULL,
            arguments_hash TEXT NOT NULL,
            target_ids TEXT NOT NULL,
            risk TEXT NOT NULL,
            authorization TEXT NOT NULL,
            state TEXT NOT NULL,
            requested_seq INTEGER NOT NULL
        );

        CREATE TABLE tool_results (
            tool_call_id TEXT PRIMARY KEY NOT NULL REFERENCES tool_calls(id),
            success INTEGER NOT NULL,
            error_code TEXT,
            sanitized_summary TEXT NOT NULL,
            duration_ms INTEGER NOT NULL,
            artifact_ref TEXT,
            completed_at INTEGER NOT NULL
        );

        CREATE TABLE observations (
            id TEXT PRIMARY KEY NOT NULL,
            run_id TEXT NOT NULL REFERENCES agent_runs(id),
            source_event_seq INTEGER NOT NULL,
            kind TEXT NOT NULL,
            sanitized_content TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE approval_requests (
            id TEXT PRIMARY KEY NOT NULL,
            run_id TEXT NOT NULL REFERENCES agent_runs(id),
            tool_call_id TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            arguments_hash TEXT NOT NULL,
            target_ids TEXT NOT NULL,
            risk TEXT NOT NULL,
            policy_version INTEGER NOT NULL,
            policy_hash TEXT NOT NULL,
            precondition_ref TEXT,
            change_set_id TEXT,
            change_set_version INTEGER,
            state TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            decided_at INTEGER
        );

        CREATE TABLE agent_checkpoints (
            run_id TEXT PRIMARY KEY NOT NULL REFERENCES agent_runs(id),
            state TEXT NOT NULL,
            event_cursor INTEGER NOT NULL,
            pending_interrupt TEXT,
            budget_state TEXT NOT NULL,
            target_ids TEXT NOT NULL,
            fact_snapshot TEXT NOT NULL DEFAULT '{}',
            policy_snapshot TEXT NOT NULL DEFAULT '{}',
            change_set_ref TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE pending_tool_calls (
            tool_call_id TEXT PRIMARY KEY NOT NULL,
            run_id TEXT,
            call_json TEXT NOT NULL
        );
        ",
    )
    .map_err(|_| AgentStoreError::MigrationFailed)
}

fn apply_v2(conn: &Connection) -> Result<(), AgentStoreError> {
    conn.execute_batch(
        "ALTER TABLE approval_requests ADD COLUMN change_set_id TEXT;
         ALTER TABLE approval_requests ADD COLUMN change_set_version INTEGER;",
    )
    .map_err(|_| AgentStoreError::MigrationFailed)
}

fn apply_v3(conn: &Connection) -> Result<(), AgentStoreError> {
    conn.execute_batch(
        "CREATE TABLE fleet_runs_v2 (
            id TEXT PRIMARY KEY NOT NULL,
            version INTEGER NOT NULL,
            state TEXT NOT NULL,
            production INTEGER NOT NULL,
            failure_policy TEXT NOT NULL,
            graph_digest TEXT NOT NULL,
            recovery_state TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
         );

         CREATE TABLE fleet_targets_v2 (
            fleet_run_id TEXT NOT NULL REFERENCES fleet_runs_v2(id) ON DELETE CASCADE,
            ordinal INTEGER NOT NULL,
            profile_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            role TEXT,
            PRIMARY KEY (fleet_run_id, ordinal),
            UNIQUE (fleet_run_id, profile_id),
            UNIQUE (fleet_run_id, session_id)
         );

         CREATE TABLE fleet_stages_v2 (
            fleet_run_id TEXT NOT NULL REFERENCES fleet_runs_v2(id) ON DELETE CASCADE,
            ordinal INTEGER NOT NULL,
            id TEXT NOT NULL,
            target_ids TEXT NOT NULL,
            dependency_ids TEXT NOT NULL,
            execution_strategy TEXT NOT NULL,
            concurrency_limit INTEGER NOT NULL,
            state TEXT NOT NULL,
            PRIMARY KEY (fleet_run_id, id),
            UNIQUE (fleet_run_id, ordinal)
         );

         CREATE TABLE fleet_child_runs_v2 (
            fleet_run_id TEXT NOT NULL REFERENCES fleet_runs_v2(id) ON DELETE CASCADE,
            stage_id TEXT NOT NULL,
            target_id TEXT NOT NULL,
            agent_run_id TEXT,
            attempt INTEGER NOT NULL,
            state TEXT NOT NULL,
            error_code TEXT,
            PRIMARY KEY (fleet_run_id, stage_id, target_id, attempt)
         );",
    )
    .map_err(|_| AgentStoreError::MigrationFailed)
}

fn upsert_run(tx: &Transaction<'_>, run: &AgentRun) -> Result<(), AgentStoreError> {
    tx.execute(
        "INSERT INTO agent_runs (
            id, state, failure_code, next_event_seq, goal_summary,
            target_profile_ids, budget_json, metrics_json, created_at, updated_at
         ) VALUES (?1, ?2, NULL, ?3, '', ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET
            state = excluded.state,
            next_event_seq = excluded.next_event_seq,
            updated_at = excluded.updated_at",
        params![
            run.id().to_string(),
            state_name(run.state()),
            run.peek_next_event_seq() as i64,
            EMPTY_JSON_ARRAY,
            EMPTY_JSON_OBJECT,
            EMPTY_JSON_OBJECT,
            run.created_at_epoch_ms() as i64,
            run.updated_at_epoch_ms() as i64,
        ],
    )
    .map_err(|_| AgentStoreError::PersistenceFailed)?;
    Ok(())
}

fn insert_run_new(tx: &Transaction<'_>, run: &AgentRun) -> Result<(), AgentStoreError> {
    let exists: Option<String> = tx
        .query_row(
            "SELECT id FROM agent_runs WHERE id = ?1",
            params![run.id().to_string()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| AgentStoreError::PersistenceFailed)?;
    if exists.is_some() {
        return Err(AgentStoreError::RunAlreadyExists);
    }
    upsert_run(tx, run)
}

fn load_run(conn: &Connection, run_id: Uuid) -> Result<Option<AgentRun>, AgentStoreError> {
    let row = conn
        .query_row(
            "SELECT id, state, created_at, updated_at, next_event_seq
             FROM agent_runs WHERE id = ?1",
            params![run_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|_| AgentStoreError::StoreCorrupt)?;
    row.map(|(id, state, created, updated, seq)| {
        Ok(AgentRun::from_persisted(
            parse_uuid(&id)?,
            parse_state(&state)?,
            created as u64,
            updated as u64,
            seq as u64,
        ))
    })
    .transpose()
}

fn append_event(
    tx: &Transaction<'_>,
    envelope: &AgentEventEnvelope,
) -> Result<(), AgentStoreError> {
    let last: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM agent_events WHERE run_id = ?1",
            params![envelope.run_id.to_string()],
            |row| row.get(0),
        )
        .map_err(|_| AgentStoreError::PersistenceFailed)?;
    if (envelope.seq as i64) <= last {
        return Err(AgentStoreError::EventSequenceInvalid {
            last: last as u64,
            attempted: envelope.seq,
        });
    }
    let event_json =
        serde_json::to_string(&envelope.event).map_err(|_| AgentStoreError::PersistenceFailed)?;
    let (event_json, _) = redact_secrets(&event_json);
    tx.execute(
        "INSERT INTO agent_events (run_id, seq, timestamp_ms, event_json)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            envelope.run_id.to_string(),
            envelope.seq as i64,
            envelope.timestamp_epoch_ms as i64,
            event_json,
        ],
    )
    .map_err(|_| AgentStoreError::PersistenceFailed)?;
    Ok(())
}

fn project_event_side_tables(
    tx: &Transaction<'_>,
    envelope: &AgentEventEnvelope,
) -> Result<(), AgentStoreError> {
    match &envelope.event {
        AgentEvent::UserMessageAdded { content }
        | AgentEvent::AssistantMessageAdded { content } => {
            let role = if matches!(envelope.event, AgentEvent::UserMessageAdded { .. }) {
                "user"
            } else {
                "assistant"
            };
            let (content, _) = redact_secrets(content);
            tx.execute(
                "INSERT INTO agent_messages (id, run_id, role, content, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    Uuid::new_v4().to_string(),
                    envelope.run_id.to_string(),
                    role,
                    content,
                    envelope.timestamp_epoch_ms as i64,
                ],
            )
            .map_err(|_| AgentStoreError::PersistenceFailed)?;
        }
        AgentEvent::ToolRequested {
            tool_call_id,
            tool_name,
        } => {
            tx.execute(
                "INSERT OR IGNORE INTO tool_calls (
                    id, run_id, tool_name, sanitized_arguments, arguments_hash,
                    target_ids, risk, authorization, state, requested_seq
                 ) VALUES (?1, ?2, ?3, '{}', '', '[]', '', 'pending', 'requested', ?4)",
                params![
                    tool_call_id.to_string(),
                    envelope.run_id.to_string(),
                    tool_name,
                    envelope.seq as i64,
                ],
            )
            .map_err(|_| AgentStoreError::PersistenceFailed)?;
        }
        AgentEvent::ToolAutoAuthorized { tool_call_id } => {
            let _ = tx.execute(
                "UPDATE tool_calls SET authorization = 'auto' WHERE id = ?1",
                params![tool_call_id.to_string()],
            );
        }
        AgentEvent::ToolApprovalRequired { tool_call_id } => {
            let _ = tx.execute(
                "UPDATE tool_calls SET authorization = 'approval_required', state = 'awaiting_approval'
                 WHERE id = ?1",
                params![tool_call_id.to_string()],
            );
        }
        AgentEvent::ToolCompleted { tool_call_id } => {
            let _ = tx.execute(
                "UPDATE tool_calls SET state = 'completed' WHERE id = ?1",
                params![tool_call_id.to_string()],
            );
            tx.execute(
                "INSERT OR REPLACE INTO tool_results (
                    tool_call_id, success, error_code, sanitized_summary,
                    duration_ms, artifact_ref, completed_at
                 ) VALUES (?1, 1, NULL, 'completed', 0, NULL, ?2)",
                params![tool_call_id.to_string(), envelope.timestamp_epoch_ms as i64],
            )
            .map_err(|_| AgentStoreError::PersistenceFailed)?;
        }
        AgentEvent::ToolFailed {
            tool_call_id,
            error_code,
        } => {
            let _ = tx.execute(
                "UPDATE tool_calls SET state = 'failed' WHERE id = ?1",
                params![tool_call_id.to_string()],
            );
            let (error_code, _) = redact_secrets(error_code);
            tx.execute(
                "INSERT OR REPLACE INTO tool_results (
                    tool_call_id, success, error_code, sanitized_summary,
                    duration_ms, artifact_ref, completed_at
                 ) VALUES (?1, 0, ?2, 'failed', 0, NULL, ?3)",
                params![
                    tool_call_id.to_string(),
                    error_code,
                    envelope.timestamp_epoch_ms as i64
                ],
            )
            .map_err(|_| AgentStoreError::PersistenceFailed)?;
        }
        AgentEvent::ObservationAdded { summary } => {
            let (summary, _) = redact_secrets(summary);
            tx.execute(
                "INSERT INTO observations (
                    id, run_id, source_event_seq, kind, sanitized_content, created_at
                 ) VALUES (?1, ?2, ?3, 'observation', ?4, ?5)",
                params![
                    Uuid::new_v4().to_string(),
                    envelope.run_id.to_string(),
                    envelope.seq as i64,
                    summary,
                    envelope.timestamp_epoch_ms as i64,
                ],
            )
            .map_err(|_| AgentStoreError::PersistenceFailed)?;
        }
        _ => {}
    }
    Ok(())
}

fn load_events_after(
    conn: &Connection,
    run_id: Uuid,
    after_seq: u64,
) -> Result<Vec<AgentEventEnvelope>, AgentStoreError> {
    let mut stmt = conn
        .prepare(
            "SELECT run_id, seq, timestamp_ms, event_json
             FROM agent_events
             WHERE run_id = ?1 AND seq > ?2
             ORDER BY seq ASC",
        )
        .map_err(|_| AgentStoreError::PersistenceFailed)?;
    let rows = stmt
        .query_map(params![run_id.to_string(), after_seq as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|_| AgentStoreError::StoreCorrupt)?;
    let mut events = Vec::new();
    for row in rows {
        let (id, seq, ts, json) = row.map_err(|_| AgentStoreError::StoreCorrupt)?;
        let event: AgentEvent =
            serde_json::from_str(&json).map_err(|_| AgentStoreError::StoreCorrupt)?;
        events.push(AgentEventEnvelope {
            run_id: parse_uuid(&id)?,
            seq: seq as u64,
            timestamp_epoch_ms: ts as u64,
            event,
        });
    }
    Ok(events)
}

fn upsert_approval(
    tx: &Transaction<'_>,
    approval: &ApprovalRequest,
) -> Result<(), AgentStoreError> {
    let targets = serde_json::to_string(&approval.target_ids)
        .map_err(|_| AgentStoreError::PersistenceFailed)?;
    tx.execute(
        "INSERT INTO approval_requests (
            id, run_id, tool_call_id, tool_name, arguments_hash, target_ids,
            risk, policy_version, policy_hash, precondition_ref,
            change_set_id, change_set_version, state, created_at, decided_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(id) DO UPDATE SET
            state = excluded.state,
            decided_at = excluded.decided_at",
        params![
            approval.id.to_string(),
            approval.run_id.to_string(),
            approval.tool_call_id.to_string(),
            approval.tool_name,
            approval.arguments_hash,
            targets,
            approval.risk,
            approval.policy_version as i64,
            approval.policy_hash,
            approval.precondition_ref,
            approval.change_set_id.map(|id| id.to_string()),
            approval.change_set_version.map(|v| v as i64),
            approval_state_name(approval.state),
            approval.created_at_epoch_ms as i64,
            approval.decided_at_epoch_ms.map(|v| v as i64),
        ],
    )
    .map_err(|_| AgentStoreError::PersistenceFailed)?;
    Ok(())
}

fn load_approval(
    conn: &Connection,
    approval_id: Uuid,
) -> Result<Option<ApprovalRequest>, AgentStoreError> {
    let row = conn
        .query_row(
            "SELECT id, run_id, tool_call_id, tool_name, arguments_hash, target_ids,
                    risk, policy_version, policy_hash, precondition_ref,
                    change_set_id, change_set_version, state, created_at, decided_at
             FROM approval_requests WHERE id = ?1",
            params![approval_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, Option<i64>>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, i64>(13)?,
                    row.get::<_, Option<i64>>(14)?,
                ))
            },
        )
        .optional()
        .map_err(|_| AgentStoreError::StoreCorrupt)?;
    row.map(
        |(
            id,
            run_id,
            tool_call_id,
            tool_name,
            arguments_hash,
            targets,
            risk,
            policy_version,
            policy_hash,
            precondition_ref,
            change_set_id,
            change_set_version,
            state,
            created_at,
            decided_at,
        )| {
            let target_ids: Vec<Uuid> =
                serde_json::from_str(&targets).map_err(|_| AgentStoreError::StoreCorrupt)?;
            Ok(ApprovalRequest {
                id: parse_uuid(&id)?,
                run_id: parse_uuid(&run_id)?,
                tool_call_id: parse_uuid(&tool_call_id)?,
                tool_name,
                arguments_hash,
                target_ids,
                risk,
                policy_version: policy_version as u64,
                policy_hash,
                precondition_ref,
                change_set_id: change_set_id.map(|value| parse_uuid(&value)).transpose()?,
                change_set_version: change_set_version.map(|value| value as u64),
                state: parse_approval_state(&state)?,
                created_at_epoch_ms: created_at as u64,
                decided_at_epoch_ms: decided_at.map(|v| v as u64),
            })
        },
    )
    .transpose()
}

fn upsert_checkpoint(
    tx: &Transaction<'_>,
    checkpoint: &AgentCheckpoint,
) -> Result<(), AgentStoreError> {
    let pending = checkpoint
        .pending_interrupt
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|_| AgentStoreError::PersistenceFailed)?;
    let pending = pending.as_deref().map(|value| redact_secrets(value).0);
    let budget = serde_json::to_string(&checkpoint.budget)
        .map_err(|_| AgentStoreError::PersistenceFailed)?;
    let targets = serde_json::to_string(&checkpoint.target_ids)
        .map_err(|_| AgentStoreError::PersistenceFailed)?;
    tx.execute(
        "INSERT INTO agent_checkpoints (
            run_id, state, event_cursor, pending_interrupt, budget_state,
            target_ids, fact_snapshot, policy_snapshot, change_set_ref,
            created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, '{}', NULL, ?8, ?9)
         ON CONFLICT(run_id) DO UPDATE SET
            state = excluded.state,
            event_cursor = excluded.event_cursor,
            pending_interrupt = excluded.pending_interrupt,
            budget_state = excluded.budget_state,
            target_ids = excluded.target_ids,
            fact_snapshot = excluded.fact_snapshot,
            updated_at = excluded.updated_at",
        params![
            checkpoint.run_id.to_string(),
            state_name(checkpoint.state),
            checkpoint.event_cursor as i64,
            pending,
            budget,
            targets,
            if checkpoint.fact_snapshot.is_empty() {
                "[]".to_owned()
            } else {
                checkpoint.fact_snapshot.clone()
            },
            checkpoint.created_at_epoch_ms as i64,
            now_ms() as i64,
        ],
    )
    .map_err(|_| AgentStoreError::PersistenceFailed)?;
    Ok(())
}

fn load_checkpoint(
    conn: &Connection,
    run_id: Uuid,
) -> Result<Option<AgentCheckpoint>, AgentStoreError> {
    let row = conn
        .query_row(
            "SELECT run_id, state, event_cursor, pending_interrupt, budget_state,
                    target_ids, fact_snapshot, created_at
             FROM agent_checkpoints WHERE run_id = ?1",
            params![run_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            },
        )
        .optional()
        .map_err(|_| AgentStoreError::StoreCorrupt)?;
    row.map(
        |(id, state, cursor, pending, budget, targets, fact_snapshot, created)| {
            let pending_interrupt = match pending {
                Some(json) => {
                    Some(serde_json::from_str(&json).map_err(|_| AgentStoreError::StoreCorrupt)?)
                }
                None => None,
            };
            Ok(AgentCheckpoint {
                run_id: parse_uuid(&id)?,
                state: parse_state(&state)?,
                event_cursor: cursor as u64,
                pending_interrupt,
                budget: serde_json::from_str(&budget).map_err(|_| AgentStoreError::StoreCorrupt)?,
                target_ids: serde_json::from_str(&targets)
                    .map_err(|_| AgentStoreError::StoreCorrupt)?,
                fact_snapshot,
                created_at_epoch_ms: created as u64,
            })
        },
    )
    .transpose()
}

fn put_pending_call(tx: &Transaction<'_>, call: &PreparedToolCall) -> Result<(), AgentStoreError> {
    let json = serde_json::to_string(call).map_err(|_| AgentStoreError::PersistenceFailed)?;
    let (json, _) = redact_secrets(&json);
    // Guardrail: refuse to persist obvious secret field names.
    for forbidden in ["password", "passphrase", "private_key", "vault_master"] {
        if json.to_ascii_lowercase().contains(forbidden) {
            return Err(AgentStoreError::PersistenceFailed);
        }
    }
    tx.execute(
        "INSERT INTO pending_tool_calls (tool_call_id, run_id, call_json)
         VALUES (?1, NULL, ?2)
         ON CONFLICT(tool_call_id) DO UPDATE SET call_json = excluded.call_json",
        params![call.tool_call_id.to_string(), json],
    )
    .map_err(|_| AgentStoreError::PersistenceFailed)?;
    Ok(())
}

fn load_pending_call(
    conn: &Connection,
    tool_call_id: Uuid,
) -> Result<Option<PreparedToolCall>, AgentStoreError> {
    let json: Option<String> = conn
        .query_row(
            "SELECT call_json FROM pending_tool_calls WHERE tool_call_id = ?1",
            params![tool_call_id.to_string()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| AgentStoreError::StoreCorrupt)?;
    json.map(|value| serde_json::from_str(&value).map_err(|_| AgentStoreError::StoreCorrupt))
        .transpose()
}

fn put_pending_command(
    tx: &Transaction<'_>,
    command: &PreparedCommandProposal,
) -> Result<(), AgentStoreError> {
    let json = serde_json::to_string(command).map_err(|_| AgentStoreError::PersistenceFailed)?;
    let (sanitized, redacted) = redact_secrets(&json);
    if redacted || sanitized != json {
        return Err(AgentStoreError::PersistenceFailed);
    }
    tx.execute(
        "INSERT INTO pending_tool_calls (tool_call_id, run_id, call_json)
         VALUES (?1, NULL, ?2)
         ON CONFLICT(tool_call_id) DO UPDATE SET call_json = excluded.call_json",
        params![command.command_id.to_string(), json],
    )
    .map_err(|_| AgentStoreError::PersistenceFailed)?;
    Ok(())
}

fn load_pending_command(
    conn: &Connection,
    command_id: Uuid,
) -> Result<Option<PreparedCommandProposal>, AgentStoreError> {
    let json: Option<String> = conn
        .query_row(
            "SELECT call_json FROM pending_tool_calls WHERE tool_call_id = ?1",
            params![command_id.to_string()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| AgentStoreError::StoreCorrupt)?;
    json.map(|value| serde_json::from_str(&value).map_err(|_| AgentStoreError::StoreCorrupt))
        .transpose()
}

fn state_name(state: AgentRunStateV2) -> String {
    serde_json::to_value(state)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "failed".into())
}

fn parse_state(raw: &str) -> Result<AgentRunStateV2, AgentStoreError> {
    serde_json::from_value(serde_json::Value::String(raw.to_owned()))
        .map_err(|_| AgentStoreError::StoreCorrupt)
}

fn approval_state_name(state: ApprovalRequestState) -> String {
    serde_json::to_value(state)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "invalidated".into())
}

fn parse_approval_state(raw: &str) -> Result<ApprovalRequestState, AgentStoreError> {
    serde_json::from_value(serde_json::Value::String(raw.to_owned()))
        .map_err(|_| AgentStoreError::StoreCorrupt)
}

fn parse_uuid(raw: &str) -> Result<Uuid, AgentStoreError> {
    Uuid::parse_str(raw).map_err(|_| AgentStoreError::StoreCorrupt)
}

fn now_ms() -> u64 {
    super::run::now_epoch_ms()
}

impl AgentRunStore for SqliteAgentDatabase {
    fn insert(&self, run: AgentRun) -> Result<(), AgentStoreError> {
        self.with_tx(|tx| insert_run_new(tx, &run))
    }

    fn get(&self, run_id: Uuid) -> Result<Option<AgentRun>, AgentStoreError> {
        self.with_conn(|conn| load_run(conn, run_id))
    }

    fn save(&self, run: &AgentRun) -> Result<(), AgentStoreError> {
        self.with_tx(|tx| {
            let exists: Option<String> = tx
                .query_row(
                    "SELECT id FROM agent_runs WHERE id = ?1",
                    params![run.id().to_string()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| AgentStoreError::PersistenceFailed)?;
            if exists.is_none() {
                return Err(AgentStoreError::RunNotFound);
            }
            upsert_run(tx, run)
        })
    }

    fn save_metrics(&self, run_id: Uuid, metrics_json: &str) -> Result<(), AgentStoreError> {
        self.with_tx(|tx| {
            let updated = tx
                .execute(
                    "UPDATE agent_runs SET metrics_json = ?1, updated_at = ?2 WHERE id = ?3",
                    params![
                        metrics_json,
                        super::run::now_epoch_ms() as i64,
                        run_id.to_string(),
                    ],
                )
                .map_err(|_| AgentStoreError::PersistenceFailed)?;
            if updated == 0 {
                return Err(AgentStoreError::RunNotFound);
            }
            Ok(())
        })
    }
}

impl AgentEventRepository for SqliteAgentDatabase {
    fn append(&self, envelope: AgentEventEnvelope) -> Result<(), AgentStoreError> {
        self.with_tx(|tx| {
            append_event(tx, &envelope)?;
            project_event_side_tables(tx, &envelope)?;
            Ok(())
        })
    }

    fn events_after(
        &self,
        run_id: Uuid,
        after_seq: u64,
    ) -> Result<Vec<AgentEventEnvelope>, AgentStoreError> {
        self.with_conn(|conn| load_events_after(conn, run_id, after_seq))
    }

    fn all_events(&self, run_id: Uuid) -> Result<Vec<AgentEventEnvelope>, AgentStoreError> {
        self.events_after(run_id, 0)
    }
}

impl ApprovalStore for SqliteAgentDatabase {
    fn insert(&self, approval: ApprovalRequest) -> Result<(), AgentStoreError> {
        self.with_tx(|tx| upsert_approval(tx, &approval))
    }

    fn get(&self, approval_id: Uuid) -> Result<Option<ApprovalRequest>, AgentStoreError> {
        self.with_conn(|conn| load_approval(conn, approval_id))
    }

    fn save(&self, approval: &ApprovalRequest) -> Result<(), AgentStoreError> {
        self.with_tx(|tx| {
            let exists: Option<String> = tx
                .query_row(
                    "SELECT id FROM approval_requests WHERE id = ?1",
                    params![approval.id.to_string()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| AgentStoreError::PersistenceFailed)?;
            if exists.is_none() {
                return Err(AgentStoreError::ApprovalNotFound);
            }
            upsert_approval(tx, approval)
        })
    }

    fn pending_for_run(&self, run_id: Uuid) -> Result<Option<ApprovalRequest>, AgentStoreError> {
        self.with_conn(|conn| {
            let id: Option<String> = conn
                .query_row(
                    "SELECT id FROM approval_requests
                     WHERE run_id = ?1 AND state = 'pending'
                     LIMIT 1",
                    params![run_id.to_string()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| AgentStoreError::StoreCorrupt)?;
            match id {
                Some(id) => load_approval(conn, parse_uuid(&id)?),
                None => Ok(None),
            }
        })
    }
}

impl CheckpointStore for SqliteAgentDatabase {
    fn save(&self, checkpoint: AgentCheckpoint) -> Result<(), AgentStoreError> {
        self.with_tx(|tx| upsert_checkpoint(tx, &checkpoint))
    }

    fn get(&self, run_id: Uuid) -> Result<Option<AgentCheckpoint>, AgentStoreError> {
        self.with_conn(|conn| load_checkpoint(conn, run_id))
    }
}

impl PendingToolCallStore for SqliteAgentDatabase {
    fn put(&self, call: PreparedToolCall) -> Result<(), AgentStoreError> {
        self.with_tx(|tx| put_pending_call(tx, &call))
    }

    fn get(&self, tool_call_id: Uuid) -> Result<Option<PreparedToolCall>, AgentStoreError> {
        self.with_conn(|conn| load_pending_call(conn, tool_call_id))
    }

    fn remove(&self, tool_call_id: Uuid) -> Result<(), AgentStoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM pending_tool_calls WHERE tool_call_id = ?1",
                params![tool_call_id.to_string()],
            )
            .map_err(|_| AgentStoreError::PersistenceFailed)?;
            Ok(())
        })
    }

    fn put_command(&self, command: PreparedCommandProposal) -> Result<(), AgentStoreError> {
        self.with_tx(|tx| put_pending_command(tx, &command))
    }

    fn get_command(
        &self,
        command_id: Uuid,
    ) -> Result<Option<PreparedCommandProposal>, AgentStoreError> {
        self.with_conn(|conn| load_pending_command(conn, command_id))
    }

    fn remove_command(&self, command_id: Uuid) -> Result<(), AgentStoreError> {
        self.remove(command_id)
    }
}

fn fleet_enum_name<T: Serialize>(value: T) -> Result<String, AgentStoreError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or(AgentStoreError::PersistenceFailed)
}

fn parse_fleet_enum<T: DeserializeOwned>(raw: &str) -> Result<T, AgentStoreError> {
    serde_json::from_value(serde_json::Value::String(raw.to_owned()))
        .map_err(|_| AgentStoreError::StoreCorrupt)
}

fn checked_u64(value: i64) -> Result<u64, AgentStoreError> {
    u64::try_from(value).map_err(|_| AgentStoreError::StoreCorrupt)
}

fn checked_usize(value: i64) -> Result<usize, AgentStoreError> {
    usize::try_from(value).map_err(|_| AgentStoreError::StoreCorrupt)
}

fn write_fleet_run(
    tx: &Transaction<'_>,
    run: &FleetRunV2,
    insert_only: bool,
) -> Result<(), AgentStoreError> {
    run.validate_metadata()
        .map_err(|_| AgentStoreError::StoreCorrupt)?;
    if insert_only {
        let exists: Option<String> = tx
            .query_row(
                "SELECT id FROM fleet_runs_v2 WHERE id = ?1",
                params![run.id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| AgentStoreError::PersistenceFailed)?;
        if exists.is_some() {
            return Err(AgentStoreError::RunAlreadyExists);
        }
    }
    tx.execute(
        "INSERT INTO fleet_runs_v2 (
            id, version, state, production, failure_policy, graph_digest,
            recovery_state, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
            version = excluded.version,
            state = excluded.state,
            production = excluded.production,
            failure_policy = excluded.failure_policy,
            graph_digest = excluded.graph_digest,
            recovery_state = excluded.recovery_state,
            updated_at = excluded.updated_at",
        params![
            run.id.to_string(),
            run.version as i64,
            fleet_enum_name(run.state)?,
            i64::from(run.production),
            fleet_enum_name(run.failure_policy)?,
            run.graph_digest.as_str(),
            fleet_enum_name(run.recovery_state)?,
            run.created_at_epoch_ms as i64,
            run.updated_at_epoch_ms as i64,
        ],
    )
    .map_err(|_| AgentStoreError::PersistenceFailed)?;

    tx.execute(
        "DELETE FROM fleet_child_runs_v2 WHERE fleet_run_id = ?1",
        params![run.id.to_string()],
    )
    .map_err(|_| AgentStoreError::PersistenceFailed)?;
    tx.execute(
        "DELETE FROM fleet_stages_v2 WHERE fleet_run_id = ?1",
        params![run.id.to_string()],
    )
    .map_err(|_| AgentStoreError::PersistenceFailed)?;
    tx.execute(
        "DELETE FROM fleet_targets_v2 WHERE fleet_run_id = ?1",
        params![run.id.to_string()],
    )
    .map_err(|_| AgentStoreError::PersistenceFailed)?;

    for target in &run.targets {
        tx.execute(
            "INSERT INTO fleet_targets_v2 (
                fleet_run_id, ordinal, profile_id, session_id, role
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                run.id.to_string(),
                target.ordinal as i64,
                target.profile_id.to_string(),
                target.session_id.to_string(),
                target.role.as_deref(),
            ],
        )
        .map_err(|_| AgentStoreError::PersistenceFailed)?;
    }
    for (ordinal, stage) in run.stages.iter().enumerate() {
        let target_ids = serde_json::to_string(&stage.target_ids)
            .map_err(|_| AgentStoreError::PersistenceFailed)?;
        let dependency_ids = serde_json::to_string(&stage.depends_on)
            .map_err(|_| AgentStoreError::PersistenceFailed)?;
        tx.execute(
            "INSERT INTO fleet_stages_v2 (
                fleet_run_id, ordinal, id, target_ids, dependency_ids,
                execution_strategy, concurrency_limit, state
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                run.id.to_string(),
                ordinal as i64,
                stage.id.to_string(),
                target_ids,
                dependency_ids,
                fleet_enum_name(stage.execution_strategy)?,
                stage.concurrency_limit as i64,
                fleet_enum_name(stage.state)?,
            ],
        )
        .map_err(|_| AgentStoreError::PersistenceFailed)?;
    }
    for child in &run.children {
        tx.execute(
            "INSERT INTO fleet_child_runs_v2 (
                fleet_run_id, stage_id, target_id, agent_run_id, attempt, state, error_code
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                run.id.to_string(),
                child.stage_id.to_string(),
                child.target_id.to_string(),
                child.agent_run_id.map(|id| id.to_string()),
                child.attempt as i64,
                fleet_enum_name(child.state)?,
                child.error_code.as_deref(),
            ],
        )
        .map_err(|_| AgentStoreError::PersistenceFailed)?;
    }
    Ok(())
}

fn load_fleet_run(
    conn: &Connection,
    fleet_run_id: Uuid,
) -> Result<Option<FleetRunV2>, AgentStoreError> {
    let row = conn
        .query_row(
            "SELECT id, version, state, production, failure_policy, graph_digest,
                    recovery_state, created_at, updated_at
             FROM fleet_runs_v2 WHERE id = ?1",
            params![fleet_run_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            },
        )
        .optional()
        .map_err(|_| AgentStoreError::StoreCorrupt)?;
    let Some((
        id,
        version,
        state,
        production,
        failure_policy,
        graph_digest,
        recovery,
        created,
        updated,
    )) = row
    else {
        return Ok(None);
    };

    let mut target_stmt = conn
        .prepare(
            "SELECT ordinal, profile_id, session_id, role
             FROM fleet_targets_v2 WHERE fleet_run_id = ?1 ORDER BY ordinal",
        )
        .map_err(|_| AgentStoreError::PersistenceFailed)?;
    let target_rows = target_stmt
        .query_map(params![id.clone()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|_| AgentStoreError::StoreCorrupt)?;
    let mut targets = Vec::new();
    for target in target_rows {
        let (ordinal, profile_id, session_id, role) =
            target.map_err(|_| AgentStoreError::StoreCorrupt)?;
        targets.push(FleetTargetBinding {
            profile_id: parse_uuid(&profile_id)?,
            session_id: parse_uuid(&session_id)?,
            role,
            ordinal: checked_usize(ordinal)?,
        });
    }

    let mut stage_stmt = conn
        .prepare(
            "SELECT id, target_ids, dependency_ids, execution_strategy,
                    concurrency_limit, state
             FROM fleet_stages_v2 WHERE fleet_run_id = ?1 ORDER BY ordinal",
        )
        .map_err(|_| AgentStoreError::PersistenceFailed)?;
    let stage_rows = stage_stmt
        .query_map(params![id.clone()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(|_| AgentStoreError::StoreCorrupt)?;
    let mut stages = Vec::new();
    for stage in stage_rows {
        let (stage_id, target_ids, dependencies, strategy, concurrency, state) =
            stage.map_err(|_| AgentStoreError::StoreCorrupt)?;
        stages.push(FleetStage {
            id: parse_uuid(&stage_id)?,
            // Recovery is deliberately content-free. A recovered plan can be
            // inspected as metadata but can never execute.
            summary: String::new(),
            target_ids: serde_json::from_str(&target_ids)
                .map_err(|_| AgentStoreError::StoreCorrupt)?,
            depends_on: serde_json::from_str(&dependencies)
                .map_err(|_| AgentStoreError::StoreCorrupt)?,
            execution_strategy: parse_fleet_enum(&strategy)?,
            concurrency_limit: checked_usize(concurrency)?,
            state: parse_fleet_enum(&state)?,
        });
    }

    let mut child_stmt = conn
        .prepare(
            "SELECT stage_id, target_id, agent_run_id, attempt, state, error_code
             FROM fleet_child_runs_v2 WHERE fleet_run_id = ?1
             ORDER BY stage_id, target_id, attempt",
        )
        .map_err(|_| AgentStoreError::PersistenceFailed)?;
    let child_rows = child_stmt
        .query_map(params![id.clone()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|_| AgentStoreError::StoreCorrupt)?;
    let mut children = Vec::new();
    for child in child_rows {
        let (stage_id, target_id, agent_run_id, attempt, state, error_code) =
            child.map_err(|_| AgentStoreError::StoreCorrupt)?;
        children.push(FleetChildRun {
            stage_id: parse_uuid(&stage_id)?,
            target_id: parse_uuid(&target_id)?,
            agent_run_id: agent_run_id.as_deref().map(parse_uuid).transpose()?,
            attempt: u32::try_from(attempt).map_err(|_| AgentStoreError::StoreCorrupt)?,
            state: parse_fleet_enum(&state)?,
            error_code,
        });
    }

    let run = FleetRunV2::from_metadata(
        parse_uuid(&id)?,
        checked_u64(version)?,
        parse_fleet_enum(&state)?,
        match production {
            0 => false,
            1 => true,
            _ => return Err(AgentStoreError::StoreCorrupt),
        },
        parse_fleet_enum(&failure_policy)?,
        targets,
        stages,
        children,
        graph_digest,
        {
            let _: FleetRecoveryStateV2 = parse_fleet_enum(&recovery)?;
            // Database reads are always metadata-only. Only the live service's
            // in-memory copy may retain execution payload and authority.
            FleetRecoveryStateV2::MetadataOnly
        },
        checked_u64(created)?,
        checked_u64(updated)?,
    );
    run.validate_metadata()
        .map_err(|_| AgentStoreError::StoreCorrupt)?;
    Ok(Some(run))
}

impl FleetRunStore for SqliteAgentDatabase {
    fn insert_fleet_run(&self, run: &FleetRunV2) -> Result<(), AgentStoreError> {
        self.with_tx(|tx| write_fleet_run(tx, run, true))
    }

    fn save_fleet_run(&self, run: &FleetRunV2) -> Result<(), AgentStoreError> {
        self.with_tx(|tx| {
            let exists: Option<String> = tx
                .query_row(
                    "SELECT id FROM fleet_runs_v2 WHERE id = ?1",
                    params![run.id.to_string()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| AgentStoreError::PersistenceFailed)?;
            if exists.is_none() {
                return Err(AgentStoreError::RunNotFound);
            }
            write_fleet_run(tx, run, false)
        })
    }

    fn get_fleet_run(&self, id: Uuid) -> Result<Option<FleetRunV2>, AgentStoreError> {
        self.with_conn(|conn| load_fleet_run(conn, id))
    }

    fn list_fleet_runs(&self) -> Result<Vec<FleetRunV2>, AgentStoreError> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT id FROM fleet_runs_v2 ORDER BY created_at, id")
                .map_err(|_| AgentStoreError::PersistenceFailed)?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|_| AgentStoreError::StoreCorrupt)?;
            let mut runs = Vec::new();
            for row in rows {
                let id = parse_uuid(&row.map_err(|_| AgentStoreError::StoreCorrupt)?)?;
                runs.push(load_fleet_run(conn, id)?.ok_or(AgentStoreError::StoreCorrupt)?);
            }
            Ok(runs)
        })
    }

    fn recover_fleet_runs(&self) -> Result<usize, AgentStoreError> {
        self.with_tx(|tx| {
            let interrupted = fleet_enum_name(FleetRunStateV2::Interrupted)?;
            let changed = tx
                .execute(
                    "UPDATE fleet_runs_v2 SET state = ?1, updated_at = ?2
                     WHERE state IN (?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        interrupted,
                        now_ms() as i64,
                        fleet_enum_name(FleetRunStateV2::ValidatingTargets)?,
                        fleet_enum_name(FleetRunStateV2::Investigating)?,
                        fleet_enum_name(FleetRunStateV2::Planning)?,
                        fleet_enum_name(FleetRunStateV2::Executing)?,
                        fleet_enum_name(FleetRunStateV2::Verifying)?,
                        fleet_enum_name(FleetRunStateV2::RollingBack)?,
                    ],
                )
                .map_err(|_| AgentStoreError::PersistenceFailed)?;
            tx.execute(
                "UPDATE fleet_runs_v2 SET recovery_state = ?1",
                params![fleet_enum_name(FleetRecoveryStateV2::MetadataOnly)?],
            )
            .map_err(|_| AgentStoreError::PersistenceFailed)?;
            Ok(changed)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::approval::ApprovalRequest;
    use crate::agent::checkpoint::{BudgetCheckpoint, PendingInterruptRef};
    use crate::agent::decision::ValidatedDecision;
    use crate::agent::decision::{
        validate_decision, AgentDecision, CommandProposalRequest, ToolCallRequest,
    };
    use crate::agent::event::AgentEvent;
    use crate::agent::fleet_run::{
        FleetExecutionStrategyV2, FleetFailurePolicyV2, FleetRunDraft, FleetStageDraft,
        FleetStageStateV2,
    };
    use crate::agent::repository::{
        AgentEventRepository, AgentRunStore, ApprovalStore, CheckpointStore, PendingToolCallStore,
        AGENT_STORE_CORRUPT,
    };
    use serde_json::json;
    use tempfile::TempDir;

    fn disk_call() -> PreparedToolCall {
        match validate_decision(AgentDecision::ToolCalls(vec![ToolCallRequest {
            tool_name: "system.disk_usage".into(),
            arguments: json!({}),
            reason_summary: "Checking disks".into(),
        }]))
        .expect("validates")
        {
            ValidatedDecision::ToolCalls(mut calls) => calls.remove(0),
            _ => unreachable!(),
        }
    }

    fn proposed_command(value: &str) -> PreparedCommandProposal {
        match validate_decision(AgentDecision::CommandProposal(CommandProposalRequest {
            command: value.into(),
            reason_summary: "Inspect filesystem usage".into(),
            observation_analysis: None,
        }))
        .expect("validates")
        {
            ValidatedDecision::CommandProposal(command) => command,
            _ => unreachable!(),
        }
    }

    fn envelope(run_id: Uuid, seq: u64, event: AgentEvent) -> AgentEventEnvelope {
        AgentEventEnvelope {
            run_id,
            seq,
            timestamp_epoch_ms: 1_725_000_000_000 + seq,
            event,
        }
    }

    #[test]
    fn schema_migrates_to_current_version() {
        let db = SqliteAgentDatabase::open_in_memory().expect("open");
        let version: i64 = db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .map_err(|_| AgentStoreError::StoreCorrupt)
            })
            .expect("version");
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn schema_version_two_migrates_to_content_free_fleet_tables() {
        let directory = TempDir::new().expect("temp");
        let path = directory.path().join("agent-v2.db");
        {
            let conn = Connection::open(&path).expect("open fixture");
            conn.execute_batch(
                "PRAGMA foreign_keys = ON;
                 CREATE TABLE schema_version (version INTEGER NOT NULL);",
            )
            .expect("schema table");
            apply_v1(&conn).expect("v2 baseline tables");
            conn.execute("INSERT INTO schema_version (version) VALUES (2)", [])
                .expect("v2 marker");
        }

        let db = SqliteAgentDatabase::open(&path).expect("migrate v2");
        db.with_conn(|conn| {
            let version: i64 = conn
                .query_row(
                    "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .map_err(|_| AgentStoreError::StoreCorrupt)?;
            let fleet_table: String = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'fleet_runs_v2'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|_| AgentStoreError::StoreCorrupt)?;
            assert_eq!(version, SCHEMA_VERSION);
            assert_eq!(fleet_table, "fleet_runs_v2");
            Ok(())
        })
        .expect("verify migration");
    }

    #[test]
    fn pending_command_round_trips_through_durable_store() {
        let db = SqliteAgentDatabase::open_in_memory().expect("open");
        let command = proposed_command("df -h");
        PendingToolCallStore::put_command(&db, command.clone()).expect("put command");
        let loaded = PendingToolCallStore::get_command(&db, command.command_id)
            .expect("get command")
            .expect("command exists");
        assert_eq!(loaded, command);
        PendingToolCallStore::remove_command(&db, command.command_id).expect("remove command");
        assert!(PendingToolCallStore::get_command(&db, command.command_id)
            .expect("get after remove")
            .is_none());
    }

    #[test]
    fn awaiting_approval_survives_reopen() {
        let directory = TempDir::new().expect("temp");
        let path = directory.path().join("runory-agent.db");
        let run_id;
        let approval_id;
        let tool_call_id;
        {
            let db = SqliteAgentDatabase::open(&path).expect("open");
            let mut run = AgentRun::new(Uuid::new_v4());
            run_id = run.id();
            run.transition_to(AgentRunStateV2::Running).unwrap();
            run.transition_to(AgentRunStateV2::Reasoning).unwrap();
            run.transition_to(AgentRunStateV2::Acting).unwrap();
            run.transition_to(AgentRunStateV2::AwaitingApproval)
                .unwrap();
            let call = disk_call();
            tool_call_id = call.tool_call_id;
            let approval = ApprovalRequest::bind(
                run_id,
                &call,
                vec![Uuid::new_v4()],
                "R1".into(),
                1,
                "hash".into(),
            );
            approval_id = approval.id;
            let checkpoint = AgentCheckpoint {
                run_id,
                state: AgentRunStateV2::AwaitingApproval,
                event_cursor: 1,
                pending_interrupt: Some(PendingInterruptRef::Approval { approval_id }),
                budget: BudgetCheckpoint {
                    rounds_used: 1,
                    tool_calls_used: 0,
                    elapsed_ms: 10,
                },
                target_ids: approval.target_ids.clone(),
                fact_snapshot: "[]".into(),
                created_at_epoch_ms: 1,
            };
            db.persist_interrupt_bundle(
                &run,
                &[envelope(
                    run_id,
                    1,
                    AgentEvent::ToolApprovalRequired { tool_call_id },
                )],
                &checkpoint,
                Some(&approval),
                Some(&call),
            )
            .expect("persist");
        }

        let db = SqliteAgentDatabase::open(&path).expect("reopen");
        let recovered = db.recover_on_startup().expect("recover");
        assert!(recovered.iter().any(|item| {
            item.run_id == run_id
                && item.state == AgentRunStateV2::AwaitingApproval
                && !item.interrupted
        }));
        let loaded = AgentRunStore::get(&db, run_id).expect("get").expect("run");
        assert_eq!(loaded.state(), AgentRunStateV2::AwaitingApproval);
        assert_eq!(
            CheckpointStore::get(&db, run_id)
                .expect("checkpoint")
                .expect("present")
                .state,
            AgentRunStateV2::AwaitingApproval
        );
        assert!(ApprovalStore::get(&db, approval_id)
            .expect("approval")
            .is_some());
        assert!(PendingToolCallStore::get(&db, tool_call_id)
            .expect("pending")
            .is_some());
    }

    #[test]
    fn awaiting_user_survives_reopen() {
        let directory = TempDir::new().expect("temp");
        let path = directory.path().join("agent.db");
        let run_id;
        {
            let db = SqliteAgentDatabase::open(&path).expect("open");
            let mut run = AgentRun::new(Uuid::new_v4());
            run_id = run.id();
            run.transition_to(AgentRunStateV2::Running).unwrap();
            run.transition_to(AgentRunStateV2::Reasoning).unwrap();
            run.transition_to(AgentRunStateV2::AwaitingUser).unwrap();
            AgentRunStore::insert(&db, run.clone()).unwrap();
            CheckpointStore::save(
                &db,
                AgentCheckpoint {
                    run_id,
                    state: AgentRunStateV2::AwaitingUser,
                    event_cursor: 0,
                    pending_interrupt: Some(PendingInterruptRef::UserInput {
                        question: "Which site?".into(),
                    }),
                    budget: BudgetCheckpoint {
                        rounds_used: 1,
                        tool_calls_used: 0,
                        elapsed_ms: 5,
                    },
                    target_ids: Vec::new(),
                    fact_snapshot: "[]".into(),
                    created_at_epoch_ms: 1,
                },
            )
            .unwrap();
        }
        let db = SqliteAgentDatabase::open(&path).expect("reopen");
        let recovered = db.recover_on_startup().expect("recover");
        assert!(recovered.iter().any(|item| {
            item.run_id == run_id
                && item.state == AgentRunStateV2::AwaitingUser
                && !item.interrupted
        }));
    }

    #[test]
    fn active_running_state_is_conservatively_paused() {
        let db = SqliteAgentDatabase::open_in_memory().expect("open");
        let mut run = AgentRun::new(Uuid::new_v4());
        run.transition_to(AgentRunStateV2::Running).unwrap();
        run.transition_to(AgentRunStateV2::Reasoning).unwrap();
        run.transition_to(AgentRunStateV2::Acting).unwrap();
        AgentRunStore::insert(&db, run.clone()).unwrap();
        let recovered = db.recover_on_startup().expect("recover");
        assert!(recovered.iter().any(|item| {
            item.run_id == run.id() && item.state == AgentRunStateV2::Paused && item.interrupted
        }));
        assert_eq!(
            AgentRunStore::get(&db, run.id()).unwrap().unwrap().state(),
            AgentRunStateV2::Paused
        );
    }

    #[test]
    fn corrupt_checkpoint_json_fails_closed() {
        let db = SqliteAgentDatabase::open_in_memory().expect("open");
        let run = AgentRun::new(Uuid::new_v4());
        AgentRunStore::insert(&db, run.clone()).unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO agent_checkpoints (
                    run_id, state, event_cursor, pending_interrupt, budget_state,
                    target_ids, created_at, updated_at
                 ) VALUES (?1, 'awaiting_user', 1, 'not-json', '{}', '[]', 1, 1)",
                params![run.id().to_string()],
            )
            .map_err(|_| AgentStoreError::PersistenceFailed)?;
            Ok(())
        })
        .unwrap();
        let error = CheckpointStore::get(&db, run.id()).expect_err("corrupt");
        assert_eq!(error.code(), AGENT_STORE_CORRUPT);
    }

    #[test]
    fn event_replay_keeps_order_across_reopen() {
        let directory = TempDir::new().expect("temp");
        let path = directory.path().join("events.db");
        let run_id;
        {
            let db = SqliteAgentDatabase::open(&path).expect("open");
            let run = AgentRun::new(Uuid::new_v4());
            run_id = run.id();
            AgentRunStore::insert(&db, run).unwrap();
            for seq in 1..=5 {
                AgentEventRepository::append(
                    &db,
                    envelope(
                        run_id,
                        seq,
                        AgentEvent::ProgressUpdated {
                            summary: format!("step {seq}"),
                        },
                    ),
                )
                .unwrap();
            }
        }
        let db = SqliteAgentDatabase::open(&path).expect("reopen");
        let events = AgentEventRepository::events_after(&db, run_id, 2).unwrap();
        let seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![3, 4, 5]);
    }

    #[test]
    fn persistence_never_writes_secret_field_names() {
        let db = SqliteAgentDatabase::open_in_memory().expect("open");
        let run = AgentRun::new(Uuid::new_v4());
        AgentRunStore::insert(&db, run.clone()).unwrap();
        AgentEventRepository::append(
            &db,
            envelope(
                run.id(),
                1,
                AgentEvent::UserMessageAdded {
                    content: "check disk".into(),
                },
            ),
        )
        .unwrap();
        let dump: String = db
            .with_conn(|conn| {
                let mut out = String::new();
                for table in [
                    "agent_runs",
                    "agent_events",
                    "agent_messages",
                    "approval_requests",
                    "agent_checkpoints",
                    "pending_tool_calls",
                ] {
                    let mut stmt = conn
                        .prepare(&format!("SELECT * FROM {table}"))
                        .map_err(|_| AgentStoreError::PersistenceFailed)?;
                    let column_count = stmt.column_count();
                    let mut rows = stmt
                        .query([])
                        .map_err(|_| AgentStoreError::PersistenceFailed)?;
                    while let Some(row) = rows
                        .next()
                        .map_err(|_| AgentStoreError::PersistenceFailed)?
                    {
                        for i in 0..column_count {
                            if let Ok(value) = row.get::<_, String>(i) {
                                out.push_str(&value);
                                out.push('\n');
                            }
                        }
                    }
                }
                Ok(out)
            })
            .unwrap();
        for forbidden in [
            "password",
            "passphrase",
            "private_key",
            "vault_master",
            "chain_of_thought",
        ] {
            assert!(
                !dump.to_ascii_lowercase().contains(forbidden),
                "persisted data must not contain {forbidden}"
            );
        }
    }

    #[test]
    fn stale_pending_approval_remains_blocked_until_revalidated() {
        let db = SqliteAgentDatabase::open_in_memory().expect("open");
        let mut run = AgentRun::new(Uuid::new_v4());
        run.transition_to(AgentRunStateV2::Running).unwrap();
        run.transition_to(AgentRunStateV2::Reasoning).unwrap();
        run.transition_to(AgentRunStateV2::Acting).unwrap();
        run.transition_to(AgentRunStateV2::AwaitingApproval)
            .unwrap();
        let call = disk_call();
        let approval = ApprovalRequest::bind(
            run.id(),
            &call,
            vec![Uuid::new_v4()],
            "R1".into(),
            1,
            "old-policy-hash".into(),
        );
        AgentRunStore::insert(&db, run.clone()).unwrap();
        ApprovalStore::insert(&db, approval.clone()).unwrap();
        PendingToolCallStore::put(&db, call.clone()).unwrap();

        // Recovery discovers the interrupt but does not grant or execute.
        let recovered = db.recover_on_startup().unwrap();
        assert!(recovered
            .iter()
            .any(|item| item.state == AgentRunStateV2::AwaitingApproval));
        let pending = ApprovalStore::pending_for_run(&db, run.id())
            .unwrap()
            .expect("still pending");
        assert_eq!(pending.state, ApprovalRequestState::Pending);
        // Binding still requires live policy revalidation before execute.
        let invalid = crate::agent::approval::validate_pending_approval(
            &pending,
            &call,
            &pending.target_ids,
            false,
        );
        assert_eq!(
            invalid,
            Err(crate::agent::approval::ApprovalInvalidationReason::PolicyChanged)
        );
    }

    #[test]
    fn terminal_run_metrics_persist_to_sqlite() {
        use crate::agent::metrics::RunMetrics;

        let db = SqliteAgentDatabase::open_in_memory().expect("open");
        let run = AgentRun::new(Uuid::new_v4());
        let run_id = run.id();
        AgentRunStore::insert(&db, run).expect("insert");
        let mut metrics = RunMetrics::new(run_id);
        metrics.record_reasoner_round(256);
        metrics.record_tool_call(true);
        let metrics_json = serde_json::to_string(&metrics).expect("serialize");
        AgentRunStore::save_metrics(&db, run_id, &metrics_json).expect("save metrics");
        let stored: String = db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT metrics_json FROM agent_runs WHERE id = ?1",
                    params![run_id.to_string()],
                    |row| row.get(0),
                )
                .map_err(|_| AgentStoreError::StoreCorrupt)
            })
            .expect("load metrics");
        let back: RunMetrics = serde_json::from_str(&stored).expect("deserialize");
        assert_eq!(back.reasoner_rounds, 1);
        assert_eq!(back.auto_authorized_tools, 1);
    }

    fn fleet_run(state: FleetRunStateV2) -> FleetRunV2 {
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
                production: true,
                failure_policy: FleetFailurePolicyV2::PauseForReview,
                stages: vec![FleetStageDraft {
                    id: Uuid::new_v4(),
                    summary: "configure with password=do-not-persist".into(),
                    target_ids: targets.iter().map(|target| target.profile_id).collect(),
                    depends_on: vec![],
                    execution_strategy: FleetExecutionStrategyV2::Sequential,
                    concurrency_limit: 1,
                }],
                targets,
            },
            100,
        )
        .expect("fleet draft");
        if state != FleetRunStateV2::Draft {
            run.state = state;
        }
        run
    }

    #[test]
    fn fleet_metadata_round_trips_without_stage_summary() {
        let db = SqliteAgentDatabase::open_in_memory().expect("open");
        let mut run = fleet_run(FleetRunStateV2::Draft);
        let stage_id = run.stages[0].id;
        let target_id = run.targets[0].profile_id;
        run.children.push(FleetChildRun {
            stage_id,
            target_id,
            agent_run_id: None,
            attempt: 1,
            state: FleetStageStateV2::Pending,
            error_code: None,
        });
        FleetRunStore::insert_fleet_run(&db, &run).expect("insert fleet");

        let loaded = FleetRunStore::get_fleet_run(&db, run.id)
            .expect("load fleet")
            .expect("fleet exists");
        assert_eq!(loaded.id, run.id);
        assert_eq!(loaded.targets, run.targets);
        assert_eq!(loaded.stages[0].summary, "");
        assert_eq!(loaded.children, run.children);
        assert_eq!(loaded.recovery_state, FleetRecoveryStateV2::MetadataOnly);
        let mut recovered = loaded.clone();
        assert!(recovered
            .transition_to(FleetRunStateV2::ValidatingTargets, 200)
            .is_err());

        let persisted_text = db
            .with_conn(|conn| {
                let stage_json: String = conn
                    .query_row(
                        "SELECT target_ids || dependency_ids FROM fleet_stages_v2 WHERE fleet_run_id = ?1",
                        params![run.id.to_string()],
                        |row| row.get(0),
                    )
                    .map_err(|_| AgentStoreError::StoreCorrupt)?;
                Ok(stage_json)
            })
            .expect("persisted fleet text");
        assert!(!persisted_text.contains("do-not-persist"));
        assert!(!persisted_text.to_ascii_lowercase().contains("password"));
    }

    #[test]
    fn startup_recovery_interrupts_in_flight_fleet_and_invalidates_live_payload() {
        let db = SqliteAgentDatabase::open_in_memory().expect("open");
        let run = fleet_run(FleetRunStateV2::Executing);
        FleetRunStore::insert_fleet_run(&db, &run).expect("insert fleet");

        assert_eq!(FleetRunStore::recover_fleet_runs(&db).expect("recover"), 1);
        let loaded = FleetRunStore::get_fleet_run(&db, run.id)
            .expect("load fleet")
            .expect("fleet exists");
        assert_eq!(loaded.state, FleetRunStateV2::Interrupted);
        assert_eq!(loaded.recovery_state, FleetRecoveryStateV2::MetadataOnly);
    }

    #[test]
    fn corrupt_fleet_graph_fails_closed() {
        let db = SqliteAgentDatabase::open_in_memory().expect("open");
        let run = fleet_run(FleetRunStateV2::Draft);
        FleetRunStore::insert_fleet_run(&db, &run).expect("insert fleet");
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE fleet_stages_v2 SET target_ids = 'not-json' WHERE fleet_run_id = ?1",
                params![run.id.to_string()],
            )
            .map_err(|_| AgentStoreError::PersistenceFailed)?;
            Ok(())
        })
        .expect("corrupt fixture");

        let error = FleetRunStore::get_fleet_run(&db, run.id).expect_err("corrupt fleet");
        assert_eq!(error, AgentStoreError::StoreCorrupt);
    }
}
