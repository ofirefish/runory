use super::*;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentHistoryEntry {
    pub run: AgentRun,
    pub goal: String,
    pub target_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentHistoryDetail {
    pub run: AgentRun,
    pub events: Vec<AgentEventEnvelope>,
    pub truncated: bool,
}

impl SqliteAgentDatabase {
    pub(crate) fn history_targets(&self, run_id: Uuid) -> Result<Vec<Uuid>, AgentStoreError> {
        self.with_conn(|conn| {
            let json: String = conn.query_row(
                "SELECT COALESCE(c.target_ids, r.target_profile_ids) FROM agent_runs r LEFT JOIN agent_checkpoints c ON c.run_id = r.id WHERE r.id = ?1",
                params![run_id.to_string()], |row| row.get(0),
            ).map_err(|_| AgentStoreError::RunNotFound)?;
            serde_json::from_str(&json).map_err(|_| AgentStoreError::StoreCorrupt)
        })
    }
    // Content-free target metadata lets even failures before the first
    // checkpoint appear under the correct server. It grants no execution rights.
    pub(crate) fn record_history_target(
        &self,
        run_id: Uuid,
        target: Uuid,
    ) -> Result<(), AgentStoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE agent_runs SET target_profile_ids = ?2 WHERE id = ?1",
                params![
                    run_id.to_string(),
                    serde_json::to_string(&[target])
                        .map_err(|_| AgentStoreError::PersistenceFailed)?
                ],
            )
            .map_err(|_| AgentStoreError::PersistenceFailed)?;
            Ok(())
        })
    }

    pub(crate) fn recent_history(
        &self,
        target: Option<Uuid>,
    ) -> Result<Vec<AgentHistoryEntry>, AgentStoreError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT r.id, r.state, r.created_at, r.updated_at, r.next_event_seq,
                    COALESCE((SELECT content FROM agent_messages m WHERE m.run_id = r.id AND role = 'user' ORDER BY created_at, rowid LIMIT 1), ''),
                    COALESCE(c.target_ids, r.target_profile_ids)
                 FROM agent_runs r LEFT JOIN agent_checkpoints c ON c.run_id = r.id
                 WHERE ?1 IS NULL OR EXISTS (SELECT 1 FROM json_each(COALESCE(c.target_ids, r.target_profile_ids)) WHERE value = ?1)
                 ORDER BY r.updated_at DESC, r.created_at DESC, r.id DESC LIMIT 50"
            ).map_err(|_| AgentStoreError::PersistenceFailed)?;
            let rows = stmt.query_map(params![target.map(|id| id.to_string())], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?, row.get::<_, i64>(4)?, row.get::<_, String>(5)?, row.get::<_, String>(6)?))
            }).map_err(|_| AgentStoreError::StoreCorrupt)?;
            rows.map(|row| {
                let (id, state, created, updated, seq, goal, targets) = row.map_err(|_| AgentStoreError::StoreCorrupt)?;
                Ok(AgentHistoryEntry {
                    run: AgentRun::from_persisted(parse_uuid(&id)?, parse_state(&state)?, created as u64, updated as u64, seq as u64),
                    goal: redact_secrets(&goal).0.chars().take(200).collect(),
                    target_ids: serde_json::from_str(&targets).map_err(|_| AgentStoreError::StoreCorrupt)?,
                })
            }).collect()
        })
    }

    /// Read-only history projection: no rebind, approval, or controller resume.
    pub(crate) fn history_detail(
        &self,
        run_id: Uuid,
    ) -> Result<AgentHistoryDetail, AgentStoreError> {
        self.with_conn(|conn| {
            let run = load_run(conn, run_id)?.ok_or(AgentStoreError::RunNotFound)?;
            let mut stmt = conn
                .prepare(
                    "SELECT seq, timestamp_ms, event_json FROM agent_events WHERE run_id = ?1
                 AND json_extract(event_json, '$.type') != 'tool_output_chunk'
                 ORDER BY seq DESC LIMIT 501",
                )
                .map_err(|_| AgentStoreError::PersistenceFailed)?;
            let rows = stmt
                .query_map(params![run_id.to_string()], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|_| AgentStoreError::StoreCorrupt)?;
            let mut events = Vec::new();
            let mut bytes = 0;
            let mut truncated = false;
            for row in rows {
                let (seq, timestamp, json) = row.map_err(|_| AgentStoreError::StoreCorrupt)?;
                bytes += json.len();
                if events.len() == 500 || bytes > 1024 * 1024 {
                    truncated = true;
                    break;
                }
                let safe_json = redact_secrets(&json).0;
                let mut event =
                    serde_json::from_str(&safe_json).map_err(|_| AgentStoreError::StoreCorrupt)?;
                if let AgentEvent::CommandCompleted { output_preview, .. }
                | AgentEvent::CommandFailed { output_preview, .. } = &mut event
                {
                    if output_preview.len() > 8 * 1024 {
                        let mut end = 8 * 1024;
                        while !output_preview.is_char_boundary(end) {
                            end -= 1;
                        }
                        output_preview.truncate(end);
                        truncated = true;
                    }
                }
                events.push(AgentEventEnvelope {
                    run_id,
                    seq: seq as u64,
                    timestamp_epoch_ms: timestamp as u64,
                    event,
                });
            }
            events.reverse();
            Ok(AgentHistoryDetail {
                run,
                events,
                truncated,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_uses_existing_checkpoint_targets_without_resuming_pending_work() {
        let db = SqliteAgentDatabase::open_in_memory().expect("database");
        let run =
            AgentRun::from_persisted(Uuid::new_v4(), AgentRunStateV2::AwaitingApproval, 1, 2, 1);
        AgentRunStore::insert(&db, run.clone()).expect("run");
        let target = Uuid::new_v4();
        let checkpoint = AgentCheckpoint {
            run_id: run.id(),
            state: AgentRunStateV2::AwaitingApproval,
            event_cursor: 0,
            pending_interrupt: Some(
                super::super::super::checkpoint::PendingInterruptRef::Approval {
                    approval_id: Uuid::new_v4(),
                },
            ),
            budget: super::super::super::checkpoint::BudgetCheckpoint {
                rounds_used: 1,
                tool_calls_used: 0,
                elapsed_ms: 1,
            },
            target_ids: vec![target],
            fact_snapshot: "[]".into(),
            created_at_epoch_ms: 2,
        };
        CheckpointStore::save(&db, checkpoint.clone()).expect("checkpoint");
        assert_eq!(
            db.recent_history(Some(target)).expect("history")[0].target_ids,
            vec![target]
        );
        assert_eq!(
            db.history_detail(run.id()).expect("detail").run.state(),
            AgentRunStateV2::AwaitingApproval
        );
        assert_eq!(
            CheckpointStore::get(&db, run.id()).expect("checkpoint"),
            Some(checkpoint)
        );
    }

    #[test]
    fn history_includes_terminal_runs_filters_targets_and_survives_reopen() {
        let dir = tempfile::tempdir().expect("directory");
        let path = dir.path().join("history.db");
        let db = SqliteAgentDatabase::open(&path).expect("database");
        let target = Uuid::new_v4();
        let older = AgentRun::from_persisted(Uuid::new_v4(), AgentRunStateV2::Completed, 1, 2, 2);
        let newer = AgentRun::from_persisted(Uuid::new_v4(), AgentRunStateV2::Failed, 3, 4, 2);
        for run in [&older, &newer] {
            AgentRunStore::insert(&db, run.clone()).expect("save");
            db.record_history_target(run.id(), target).expect("target");
            AgentEventRepository::append(
                &db,
                AgentEventEnvelope {
                    run_id: run.id(),
                    seq: 1,
                    timestamp_epoch_ms: 1,
                    event: AgentEvent::UserMessageAdded {
                        content: "Inspect disk".into(),
                    },
                },
            )
            .expect("event");
        }
        let entries = db.recent_history(Some(target)).expect("history");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].run.id(), newer.id());
        assert_eq!(entries[0].goal, "Inspect disk");
        assert!(db
            .recent_history(Some(Uuid::new_v4()))
            .expect("other target")
            .is_empty());
        assert!(db.list_resumable_runs().expect("resumable").is_empty());
        drop(db);
        let db = SqliteAgentDatabase::open(path).expect("reopen");
        assert_eq!(db.recent_history(None).expect("reloaded").len(), 2);
        assert_eq!(
            db.history_detail(older.id()).expect("detail").events.len(),
            1
        );
        assert_eq!(
            AgentRunStore::get(&db, older.id())
                .expect("run")
                .expect("exists")
                .state(),
            AgentRunStateV2::Completed
        );
    }

    #[test]
    fn history_is_bounded_and_does_not_replay_raw_tool_output() {
        let db = SqliteAgentDatabase::open_in_memory().expect("database");
        let run = AgentRun::new(Uuid::new_v4());
        AgentRunStore::insert(&db, run.clone()).expect("run");
        for seq in 1..=502 {
            AgentEventRepository::append(
                &db,
                AgentEventEnvelope {
                    run_id: run.id(),
                    seq,
                    timestamp_epoch_ms: seq,
                    event: AgentEvent::ProgressUpdated {
                        summary: "Checking".into(),
                    },
                },
            )
            .expect("event");
        }
        AgentEventRepository::append(
            &db,
            AgentEventEnvelope {
                run_id: run.id(),
                seq: 503,
                timestamp_epoch_ms: 503,
                event: AgentEvent::ToolOutputChunk {
                    tool_call_id: Uuid::new_v4(),
                    chunk: "raw".into(),
                },
            },
        )
        .expect("raw event");
        let detail = db.history_detail(run.id()).expect("detail");
        assert!(detail.truncated);
        assert_eq!(detail.events.len(), 500);
        assert_eq!(detail.events[0].seq, 3);
        assert_eq!(detail.events.last().expect("last").seq, 502);
        assert!(db.history_detail(Uuid::new_v4()).is_err());
    }
}
