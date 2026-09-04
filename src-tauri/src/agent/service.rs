//! Runtime V2 production service: SQLite persistence, live event broadcast,
//! and in-process controller handles for interrupt/resume.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager};
use tokio::sync::{broadcast, watch, Mutex as AsyncMutex};
use uuid::Uuid;

use super::artifact::InMemoryArtifactStore;
use super::broadcast::BroadcastingEventRepository;
use super::changeset::SessionChangeSetExecutor;
use super::checkpoint::{AgentCheckpoint, BudgetCheckpoint};
use super::controller::{
    AgentControllerConfig, AgentControllerError, AgentStores, RunBudget, RunOutcome,
};
use super::event::{AgentEvent, AgentEventEnvelope};
use super::reasoner::Observation;
use super::reasoner_planning::PlanningReasoner;
use super::repository::{
    AgentEventRepository, AgentRunStore, AgentStoreError, ApprovalStore, CheckpointStore,
};
use super::routing::planning_hints_from_goal;
use super::run::AgentRun;
use super::session_dispatch::{
    SessionPolicyGate, SessionPolicyMatcher, SessionToolDispatcher, V2Controller,
};
use super::sqlite::SqliteAgentDatabase;
use super::state::AgentRunStateV2;
use crate::agentic::ModelGateway;
use crate::agentic::PlanningHints;
use crate::domain::{AppError, AppResult, SessionId};

type ControllerSlot = Arc<AsyncMutex<Option<V2Controller>>>;

struct ActiveRun {
    cancel: watch::Sender<bool>,
    controller: ControllerSlot,
    session_id: SessionId,
    server_id: Uuid,
    hints: PlanningHints,
}

/// Shared V2 runtime wired from Tauri setup.
pub(crate) struct AgentRuntimeV2Service {
    database: Arc<SqliteAgentDatabase>,
    events: Arc<BroadcastingEventRepository>,
    active: Mutex<HashMap<Uuid, ActiveRun>>,
}

impl AgentRuntimeV2Service {
    pub fn open(app_data_dir: &Path) -> AppResult<Self> {
        let path = app_data_dir.join("runory-agent.db");
        let database = Arc::new(SqliteAgentDatabase::open(&path).map_err(store_error)?);
        database.recover_on_startup().map_err(store_error)?;
        let events = Arc::new(BroadcastingEventRepository::new(database.clone()));
        Ok(Self {
            database,
            events,
            active: Mutex::new(HashMap::new()),
        })
    }

    pub fn list_resumable_runs(&self) -> AppResult<Vec<AgentRun>> {
        self.database.list_resumable_runs().map_err(store_error)
    }

    pub fn recent_history(&self, target: Option<Uuid>) -> AppResult<Vec<super::AgentHistoryEntry>> {
        self.database.recent_history(target).map_err(store_error)
    }

    pub fn history_detail(&self, run_id: Uuid) -> AppResult<super::AgentHistoryDetail> {
        self.database.history_detail(run_id).map_err(store_error)
    }

    pub fn events_after(&self, run_id: Uuid, after_seq: u64) -> AppResult<Vec<AgentEventEnvelope>> {
        self.events
            .events_after(run_id, after_seq)
            .map_err(store_error)
    }

    pub fn resumable_target_ids(&self, run_id: Uuid) -> AppResult<Vec<Uuid>> {
        Ok(CheckpointStore::get(self.database.as_ref(), run_id)
            .map_err(store_error)?
            .map(|checkpoint| checkpoint.target_ids)
            .unwrap_or_default())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AgentEventEnvelope> {
        self.events.subscribe()
    }

    pub async fn start_run(
        &self,
        app: AppHandle,
        gateway: Arc<ModelGateway>,
        session_id: SessionId,
        server_id: Uuid,
        goal: String,
    ) -> AppResult<Uuid> {
        let hints = planning_hints_from_goal(&goal);
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let stores = self.stores();
        let config = self.controller_config(app.clone(), session_id, server_id, cancel_rx);
        let reasoner = PlanningReasoner::new(gateway, hints.clone());
        let dispatcher =
            SessionToolDispatcher::new(app.clone(), session_id, server_id, cancel_tx.subscribe());
        let controller = V2Controller::new(&goal, reasoner, dispatcher, stores, config)
            .map_err(controller_error)?;
        let run_id = controller.run_id();
        self.database
            .record_history_target(run_id, server_id)
            .map_err(store_error)?;
        let controller_slot = Arc::new(AsyncMutex::new(Some(controller)));
        self.active.lock().map_err(|_| AppError::Storage)?.insert(
            run_id,
            ActiveRun {
                cancel: cancel_tx.clone(),
                controller: controller_slot.clone(),
                session_id,
                server_id,
                hints,
            },
        );
        Self::spawn_drive(app, run_id, controller_slot);
        Ok(run_id)
    }

    pub async fn approve(
        &self,
        app: AppHandle,
        gateway: Arc<ModelGateway>,
        run_id: Uuid,
    ) -> AppResult<()> {
        let approval_id = ApprovalStore::pending_for_run(self.database.as_ref(), run_id)
            .map_err(store_error)?
            .ok_or(AppError::InvalidOperation)?
            .id;
        let outcome = {
            let active = self.active_entry(run_id)?;
            let controller_slot = active.controller.clone();
            let mut guard = controller_slot.lock().await;
            if guard.is_none() {
                *guard = Some(
                    self.rehydrate(app.clone(), gateway.clone(), run_id, &active)
                        .map_err(controller_error)?,
                );
            }
            let controller = guard.as_mut().expect("controller");
            controller
                .approve(approval_id)
                .await
                .map_err(controller_error)?
        };
        self.finish_outcome(run_id, outcome).await
    }

    pub async fn reject(
        &self,
        app: AppHandle,
        gateway: Arc<ModelGateway>,
        run_id: Uuid,
    ) -> AppResult<()> {
        let approval_id = ApprovalStore::pending_for_run(self.database.as_ref(), run_id)
            .map_err(store_error)?
            .ok_or(AppError::InvalidOperation)?
            .id;
        let outcome = {
            let active = self.active_entry(run_id)?;
            let controller_slot = active.controller.clone();
            let mut guard = controller_slot.lock().await;
            if guard.is_none() {
                *guard = Some(
                    self.rehydrate(app.clone(), gateway.clone(), run_id, &active)
                        .map_err(controller_error)?,
                );
            }
            let controller = guard.as_mut().expect("controller");
            controller
                .reject(approval_id)
                .await
                .map_err(controller_error)?
        };
        self.finish_outcome(run_id, outcome).await
    }

    pub async fn reply(
        &self,
        app: AppHandle,
        gateway: Arc<ModelGateway>,
        run_id: Uuid,
        text: String,
    ) -> AppResult<()> {
        let outcome = {
            let active = self.active_entry(run_id)?;
            let controller_slot = active.controller.clone();
            let mut guard = controller_slot.lock().await;
            if guard.is_none() {
                *guard = Some(
                    self.rehydrate(app.clone(), gateway.clone(), run_id, &active)
                        .map_err(controller_error)?,
                );
            }
            let controller = guard.as_mut().expect("controller");
            if AgentRunStore::get(self.database.as_ref(), run_id)
                .map_err(store_error)?
                .is_some_and(|run| run.state().is_terminal())
            {
                controller
                    .continue_conversation(&text)
                    .await
                    .map_err(controller_error)?
            } else {
                controller
                    .resume_with_user_input(&text)
                    .await
                    .map_err(controller_error)?
            }
        };
        self.finish_outcome(run_id, outcome).await
    }

    pub async fn cancel(&self, run_id: Uuid) -> AppResult<()> {
        if let Ok(active) = self.active_entry(run_id) {
            let _ = active.cancel.send(true);
            let mut guard = active.controller.lock().await;
            if let Some(controller) = guard.as_mut() {
                let _ = controller.cancel();
            }
            *guard = None;
            self.active
                .lock()
                .map_err(|_| AppError::Storage)?
                .remove(&run_id);
            return Ok(());
        }
        if let Some(mut run) =
            AgentRunStore::get(self.database.as_ref(), run_id).map_err(store_error)?
        {
            if !run.state().is_terminal() {
                run.transition_to(AgentRunStateV2::Cancelled)
                    .map_err(|_| AppError::InvalidOperation)?;
                AgentRunStore::save(self.database.as_ref(), &run).map_err(store_error)?;
            }
        }
        Ok(())
    }

    pub async fn pause(&self, run_id: Uuid) -> AppResult<()> {
        let active = self.active_entry(run_id)?;
        let mut guard = active.controller.lock().await;
        let controller = guard.as_mut().ok_or(AppError::InvalidOperation)?;
        controller.pause().map_err(controller_error)
    }

    pub async fn resume(
        &self,
        app: AppHandle,
        gateway: Arc<ModelGateway>,
        run_id: Uuid,
    ) -> AppResult<()> {
        let active = self.active_entry(run_id)?;
        let outcome = {
            let controller_slot = active.controller.clone();
            let mut guard = controller_slot.lock().await;
            if guard.is_none() {
                *guard = Some(
                    self.rehydrate(app, gateway, run_id, &active)
                        .map_err(controller_error)?,
                );
            }
            let controller = guard.as_mut().expect("controller");
            controller.resume().await.map_err(controller_error)?
        };
        self.finish_outcome(run_id, outcome).await
    }

    pub fn register_resumable(
        &self,
        run_id: Uuid,
        session_id: SessionId,
        server_id: Uuid,
        goal: &str,
    ) -> AppResult<()> {
        let target_ids = self.database.history_targets(run_id).map_err(store_error)?;
        if target_ids != vec![server_id] {
            return Err(AppError::InvalidOperation);
        }
        if let Ok(active) = self.active_entry(run_id) {
            return if active.session_id == session_id && active.server_id == server_id {
                Ok(())
            } else {
                Err(AppError::InvalidOperation)
            };
        }
        let hints = planning_hints_from_goal(goal);
        let (cancel_tx, _) = watch::channel(false);
        self.active.lock().map_err(|_| AppError::Storage)?.insert(
            run_id,
            ActiveRun {
                cancel: cancel_tx,
                controller: Arc::new(AsyncMutex::new(None)),
                session_id,
                server_id,
                hints,
            },
        );
        Ok(())
    }

    fn active_entry(&self, run_id: Uuid) -> AppResult<ActiveRun> {
        self.active
            .lock()
            .map_err(|_| AppError::Storage)?
            .get(&run_id)
            .map(|entry| ActiveRun {
                cancel: entry.cancel.clone(),
                controller: entry.controller.clone(),
                session_id: entry.session_id,
                server_id: entry.server_id,
                hints: entry.hints.clone(),
            })
            .ok_or(AppError::InvalidOperation)
    }

    fn stores(&self) -> AgentStores {
        AgentStores {
            runs: self.database.clone(),
            events: self.events.clone(),
            approvals: self.database.clone(),
            checkpoints: self.database.clone(),
            pending_calls: self.database.clone(),
        }
    }

    fn controller_config(
        &self,
        app: AppHandle,
        session_id: SessionId,
        server_id: Uuid,
        cancellation: watch::Receiver<bool>,
    ) -> AgentControllerConfig {
        AgentControllerConfig {
            gate: Arc::new(SessionPolicyGate::new(app.clone(), server_id)),
            policy_matcher: Arc::new(SessionPolicyMatcher::new(app.clone())),
            changesets: Arc::new(SessionChangeSetExecutor::new(
                app.clone(),
                session_id,
                server_id,
            )),
            artifacts: Arc::new(InMemoryArtifactStore::default()),
            target_ids: vec![server_id],
            session_id: Some(session_id),
            budget: RunBudget::default(),
            cancellation,
        }
    }

    fn rehydrate(
        &self,
        app: AppHandle,
        gateway: Arc<ModelGateway>,
        run_id: Uuid,
        active: &ActiveRun,
    ) -> Result<V2Controller, AgentControllerError> {
        let run = AgentRunStore::get(self.database.as_ref(), run_id)
            .map_err(AgentControllerError::Store)?
            .ok_or(AgentControllerError::AlreadyTerminal)?;
        let events = self
            .events
            .all_events(run_id)
            .map_err(AgentControllerError::Store)?;
        let goal = extract_goal(&events).ok_or(AgentControllerError::InvalidGoal)?;
        let (observations, user_replies) = extract_loop_state(&events);
        let pending_command_verification = extract_pending_command_verification(&events);
        let checkpoint = CheckpointStore::get(self.database.as_ref(), run_id)
            .map_err(AgentControllerError::Store)?;
        let budget = checkpoint_budget(checkpoint.as_ref());
        let (_, cancel_rx) = watch::channel(false);
        let stores = self.stores();
        let config =
            self.controller_config(app.clone(), active.session_id, active.server_id, cancel_rx);
        let reasoner = PlanningReasoner::new(gateway, active.hints.clone());
        let dispatcher = SessionToolDispatcher::new(
            app,
            active.session_id,
            active.server_id,
            active.cancel.subscribe(),
        );
        V2Controller::attach(
            run,
            goal,
            reasoner,
            dispatcher,
            stores,
            config,
            observations,
            user_replies,
            budget.rounds_used,
            budget.tool_calls_used,
            budget.elapsed_ms,
        )
        .map(|mut controller| {
            if let Some(checkpoint) = checkpoint.as_ref() {
                controller.restore_facts_from_checkpoint(checkpoint);
            }
            controller.restore_pending_command_verification(pending_command_verification);
            controller
        })
    }

    fn spawn_drive(app: AppHandle, run_id: Uuid, controller_slot: ControllerSlot) {
        tauri::async_runtime::spawn(async move {
            let outcome = {
                let mut guard = controller_slot.lock().await;
                let Some(controller) = guard.as_mut() else {
                    return;
                };
                controller.run_to_interrupt().await
            };
            let Ok(outcome) = outcome else {
                return;
            };
            let service = app.state::<AgentRuntimeV2Service>();
            let _ = service.finish_outcome(run_id, outcome).await;
        });
    }

    async fn finish_outcome(&self, run_id: Uuid, outcome: RunOutcome) -> AppResult<()> {
        if matches!(
            outcome,
            RunOutcome::Completed { .. } | RunOutcome::Cancelled | RunOutcome::Failed { .. }
        ) {
            if let Ok(mut active) = self.active.lock() {
                active.remove(&run_id);
            }
        }
        Ok(())
    }
}

fn checkpoint_budget(checkpoint: Option<&AgentCheckpoint>) -> BudgetCheckpoint {
    checkpoint
        .map(|value| value.budget)
        .unwrap_or(BudgetCheckpoint {
            rounds_used: 0,
            tool_calls_used: 0,
            elapsed_ms: 0,
        })
}

fn extract_goal(events: &[AgentEventEnvelope]) -> Option<String> {
    events.iter().find_map(|envelope| match &envelope.event {
        AgentEvent::UserMessageAdded { content } => Some(content.clone()),
        _ => None,
    })
}

fn extract_loop_state(events: &[AgentEventEnvelope]) -> (Vec<Observation>, Vec<String>) {
    let mut observations = Vec::new();
    let mut user_replies = Vec::new();
    let mut commands = HashMap::new();
    for envelope in events {
        match &envelope.event {
            AgentEvent::CommandProposed {
                command_id,
                command,
                ..
            } => {
                commands.insert(*command_id, command.clone());
            }
            AgentEvent::CommandCompleted {
                command_id,
                exit_code,
                output_preview,
                ..
            } => observations.push(Observation {
                tool_call_id: Some(*command_id),
                tool_name: Some("agent.command".into()),
                success: true,
                error_code: None,
                summary: format!(
                    "Terminal command completed{}",
                    exit_code.map_or_else(String::new, |code| format!(" with exit code {code}"))
                ),
                detail: Some(format!(
                    "Command: {}\nOutput preview:\n{}",
                    commands
                        .get(command_id)
                        .map(String::as_str)
                        .unwrap_or("unknown"),
                    output_preview
                )),
            }),
            AgentEvent::CommandFailed {
                command_id,
                exit_code,
                output_preview,
                error_code,
                ..
            } => observations.push(Observation {
                tool_call_id: Some(*command_id),
                tool_name: Some("agent.command".into()),
                success: false,
                error_code: Some(error_code.clone()),
                summary: format!(
                    "Command failed with exit code {}",
                    exit_code.map_or_else(|| "unavailable".into(), |code| code.to_string())
                ),
                detail: Some(format!(
                    "Command: {}\nOutput preview:\n{}",
                    commands
                        .get(command_id)
                        .map(String::as_str)
                        .unwrap_or("unknown"),
                    output_preview
                )),
            }),
            AgentEvent::ObservationAdded { summary } => observations.push(Observation {
                tool_call_id: None,
                tool_name: None,
                success: true,
                error_code: None,
                summary: summary.clone(),
                detail: None,
            }),
            AgentEvent::UserInputReceived => {}
            AgentEvent::UserMessageAdded { content } => user_replies.push(content.clone()),
            AgentEvent::AssistantMessageAdded { content } => observations.push(Observation {
                tool_call_id: None,
                tool_name: None,
                success: true,
                error_code: None,
                summary: "Previous assistant response (historical context)".into(),
                detail: Some(content.clone()),
            }),
            _ => {}
        }
    }
    (observations, user_replies)
}

fn extract_pending_command_verification(events: &[AgentEventEnvelope]) -> bool {
    let mut mutability_by_command = HashMap::new();
    let mut required = false;
    for envelope in events {
        match &envelope.event {
            AgentEvent::CommandProposed {
                command_id,
                mutability,
                ..
            } => {
                mutability_by_command.insert(*command_id, mutability.clone());
            }
            AgentEvent::CommandCompleted { command_id, .. } => {
                match mutability_by_command.get(command_id).map(String::as_str) {
                    Some("read") => required = false,
                    Some("mutating" | "unknown") => required = true,
                    _ => {}
                }
            }
            AgentEvent::CommandFailed { command_id, .. } => {
                if matches!(
                    mutability_by_command.get(command_id).map(String::as_str),
                    Some("mutating" | "unknown")
                ) {
                    required = true;
                }
            }
            _ => {}
        }
    }
    required
}

fn store_error(error: AgentStoreError) -> AppError {
    match error {
        AgentStoreError::StoreCorrupt | AgentStoreError::MigrationFailed => AppError::Storage,
        AgentStoreError::PersistenceFailed | AgentStoreError::StoreUnavailable => AppError::Storage,
        _ => AppError::InvalidOperation,
    }
}

fn controller_error(error: AgentControllerError) -> AppError {
    match error.code() {
        "AGENT_GOAL_INVALID" | "AGENT_INVALID_USER_INPUT" => AppError::InvalidOperation,
        _ => AppError::InvalidOperation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(run_id: Uuid, seq: u64, event: AgentEvent) -> AgentEventEnvelope {
        AgentEventEnvelope {
            run_id,
            seq,
            timestamp_epoch_ms: seq,
            event,
        }
    }

    #[test]
    fn recovery_preserves_mutating_command_verification_requirement() {
        let run_id = Uuid::new_v4();
        let write_id = Uuid::new_v4();
        let read_id = Uuid::new_v4();
        let mut events = vec![
            envelope(
                run_id,
                1,
                AgentEvent::CommandProposed {
                    command_id: write_id,
                    command: "systemctl restart nginx".into(),
                    reason: "Restart nginx".into(),
                    risk: "high".into(),
                    mutability: "mutating".into(),
                },
            ),
            envelope(
                run_id,
                2,
                AgentEvent::CommandCompleted {
                    command_id: write_id,
                    exit_code: Some(0),
                    output_preview: String::new(),
                    duration_ms: 1,
                },
            ),
        ];
        assert!(extract_pending_command_verification(&events));

        events.extend([
            envelope(
                run_id,
                3,
                AgentEvent::CommandProposed {
                    command_id: read_id,
                    command: "systemctl is-active nginx".into(),
                    reason: "Verify nginx".into(),
                    risk: "low".into(),
                    mutability: "read".into(),
                },
            ),
            envelope(
                run_id,
                4,
                AgentEvent::CommandCompleted {
                    command_id: read_id,
                    exit_code: Some(0),
                    output_preview: "active".into(),
                    duration_ms: 1,
                },
            ),
        ]);
        assert!(!extract_pending_command_verification(&events));
    }
}
