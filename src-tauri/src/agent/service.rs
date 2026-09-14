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
use super::fleet_control::{
    FleetApprovalStateV2, FleetApprovalV2, FleetControlStore, FleetEventEnvelopeV2,
    FleetEventKindV2,
};
use super::fleet_coordinator::{
    FleetChildLeaseV2, FleetChildOutcomeV2, FleetCoordinatorError, FleetCoordinatorV2,
    MAX_FLEET_ACTIVE_CHILDREN,
};
use super::fleet_facts::{FleetFactSet, FleetInvestigationView, FLEET_CHILD_TOOL_CALL_BUDGET};
use super::fleet_run::{
    FleetFailurePolicyV2, FleetPlanError, FleetRunDraft, FleetRunStateV2, FleetRunStore,
    FleetRunV2, FleetStageDraft,
};
use super::fleet_target::{validate_fleet_target_sessions, FleetTargetBinding};
use super::reasoner::Observation;
use super::reasoner_planning::{HostSessionContext, PlanningReasoner};
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
use crate::policy::AgentPolicyService;
use crate::ssh::ServerSessionManager;

type ControllerSlot = Arc<AsyncMutex<Option<V2Controller>>>;

struct ActiveRun {
    cancel: watch::Sender<bool>,
    controller: ControllerSlot,
    session_id: SessionId,
    server_id: Uuid,
    hints: PlanningHints,
    host_context: Option<HostSessionContext>,
    fleet_parent: Option<FleetChildParent>,
}

#[derive(Clone, Copy)]
struct FleetChildParent {
    fleet_run_id: Uuid,
    approval_id: Uuid,
}

/// Shared V2 runtime wired from Tauri setup.
pub(crate) struct AgentRuntimeV2Service {
    database: Arc<SqliteAgentDatabase>,
    events: Arc<BroadcastingEventRepository>,
    active: Mutex<HashMap<Uuid, ActiveRun>>,
    active_fleets: Mutex<HashMap<Uuid, FleetRunV2>>,
    active_fleet_facts: Mutex<HashMap<Uuid, FleetFactSet>>,
}

impl AgentRuntimeV2Service {
    pub fn open(app_data_dir: &Path) -> AppResult<Self> {
        let path = app_data_dir.join("runory-agent.db");
        let database = Arc::new(SqliteAgentDatabase::open(&path).map_err(store_error)?);
        database.recover_on_startup().map_err(store_error)?;
        database.recover_fleet_runs().map_err(store_error)?;
        let events = Arc::new(BroadcastingEventRepository::new(database.clone()));
        Ok(Self {
            database,
            events,
            active: Mutex::new(HashMap::new()),
            active_fleets: Mutex::new(HashMap::new()),
            active_fleet_facts: Mutex::new(HashMap::new()),
        })
    }

    pub fn create_fleet_draft(
        &self,
        production: bool,
        failure_policy: FleetFailurePolicyV2,
        targets: Vec<FleetTargetBinding>,
        stages: Vec<FleetStageDraft>,
    ) -> AppResult<FleetRunV2> {
        let run = FleetRunV2::draft(
            FleetRunDraft {
                production,
                failure_policy,
                targets,
                stages,
            },
            super::run::now_epoch_ms(),
        )
        .map_err(fleet_plan_error)?;
        self.database.insert_fleet_run(&run).map_err(store_error)?;
        self.active_fleets
            .lock()
            .map_err(|_| AppError::Storage)?
            .insert(run.id, run.clone());
        self.active_fleet_facts
            .lock()
            .map_err(|_| AppError::Storage)?
            .insert(run.id, FleetFactSet::new(run.targets.clone()));
        Ok(run)
    }

    /// Returns a bounded live projection. Fleet facts are deliberately not
    /// persisted; metadata-only restart recovery cannot resurrect observations.
    pub fn fleet_investigation(&self, id: Uuid) -> AppResult<FleetInvestigationView> {
        let fleet = self.fleet_run(id)?;
        let now = super::run::now_epoch_ms();
        let facts = self
            .active_fleet_facts
            .lock()
            .map_err(|_| AppError::Storage)?;
        Ok(facts
            .get(&id)
            .map(|facts| facts.view(now))
            .unwrap_or_else(|| FleetFactSet::new(fleet.targets).view(now)))
    }

    pub fn fleet_run(&self, id: Uuid) -> AppResult<FleetRunV2> {
        if let Some(run) = self
            .active_fleets
            .lock()
            .map_err(|_| AppError::Storage)?
            .get(&id)
            .cloned()
        {
            return Ok(run);
        }
        self.database
            .get_fleet_run(id)
            .map_err(store_error)?
            .ok_or(AppError::InvalidOperation)
    }

    pub fn fleet_runs(&self) -> AppResult<Vec<FleetRunV2>> {
        let mut runs = self.database.list_fleet_runs().map_err(store_error)?;
        let active = self.active_fleets.lock().map_err(|_| AppError::Storage)?;
        for run in &mut runs {
            if let Some(live) = active.get(&run.id) {
                *run = live.clone();
            }
        }
        runs.sort_by_key(|run| (run.created_at_epoch_ms, run.id));
        Ok(runs)
    }

    pub fn request_fleet_approval(
        &self,
        id: Uuid,
        expected_version: u64,
        policy_version: u64,
        policy_hash: String,
    ) -> AppResult<FleetApprovalV2> {
        let now = super::run::now_epoch_ms();
        let mut fleets = self.active_fleets.lock().map_err(|_| AppError::Storage)?;
        let run = fleets.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        if run.version != expected_version
            || run.recovery_state != super::fleet_run::FleetRecoveryStateV2::Live
        {
            return Err(AppError::InvalidOperation);
        }
        for state in [
            FleetRunStateV2::ValidatingTargets,
            FleetRunStateV2::Investigating,
            FleetRunStateV2::Planning,
            FleetRunStateV2::AwaitingApproval,
        ] {
            run.transition_to(state, now).map_err(fleet_plan_error)?;
        }
        let approval = FleetApprovalV2::bind(run, policy_version, policy_hash, now)
            .map_err(|_| AppError::InvalidOperation)?;
        let seq = self.next_fleet_event_seq(id)?;
        let event = FleetEventEnvelopeV2 {
            fleet_run_id: id,
            seq,
            timestamp_ms: now,
            kind: FleetEventKindV2::ApprovalRequired,
            state: run.state,
            approval_id: Some(approval.id),
            code: None,
        };
        self.database
            .insert_fleet_approval(run, &approval, &event)
            .map_err(store_error)?;
        Ok(approval)
    }

    pub fn decide_fleet_approval(
        &self,
        id: Uuid,
        approval_id: Uuid,
        grant: bool,
        policy_version: u64,
        policy_hash: &str,
    ) -> AppResult<FleetApprovalV2> {
        let now = super::run::now_epoch_ms();
        let mut approval = self
            .database
            .get_fleet_approval(approval_id)
            .map_err(store_error)?
            .ok_or(AppError::InvalidOperation)?;
        let mut fleets = self.active_fleets.lock().map_err(|_| AppError::Storage)?;
        let run = fleets.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        if approval.fleet_run_id != id {
            return Err(AppError::InvalidOperation);
        }
        let validation = approval.validate(run, policy_version, policy_hash);
        let (kind, code) = match validation {
            Ok(()) if grant => {
                approval.decide(FleetApprovalStateV2::Granted, now);
                run.transition_to(FleetRunStateV2::Approved, now)
                    .map_err(fleet_plan_error)?;
                (FleetEventKindV2::ApprovalGranted, None)
            }
            Ok(()) => {
                approval.decide(FleetApprovalStateV2::Rejected, now);
                run.transition_to(FleetRunStateV2::Planning, now)
                    .map_err(fleet_plan_error)?;
                (FleetEventKindV2::ApprovalRejected, None)
            }
            Err(reason) => {
                approval.invalidate(reason, now);
                if run.state == FleetRunStateV2::AwaitingApproval {
                    run.transition_to(FleetRunStateV2::Planning, now)
                        .map_err(fleet_plan_error)?;
                }
                (FleetEventKindV2::ApprovalInvalidated, Some(reason.into()))
            }
        };
        let event = FleetEventEnvelopeV2 {
            fleet_run_id: id,
            seq: self.next_fleet_event_seq(id)?,
            timestamp_ms: now,
            kind,
            state: run.state,
            approval_id: Some(approval.id),
            code,
        };
        self.database
            .decide_fleet_approval(run, &approval, &event)
            .map_err(store_error)?;
        Ok(approval)
    }

    pub fn fleet_events_after(
        &self,
        id: Uuid,
        after_seq: u64,
    ) -> AppResult<Vec<FleetEventEnvelopeV2>> {
        self.database
            .fleet_events_after(id, after_seq)
            .map_err(store_error)
    }

    fn next_fleet_event_seq(&self, id: Uuid) -> AppResult<u64> {
        Ok(self
            .database
            .fleet_events_after(id, 0)
            .map_err(store_error)?
            .last()
            .map_or(1, |event| event.seq + 1))
    }

    /// Rust-only scheduling entry point. It binds child identities to exact
    /// sessions and persists ownership before any child controller may start.
    #[allow(dead_code)]
    pub(crate) async fn claim_fleet_children(
        &self,
        id: Uuid,
        approval_id: Uuid,
        capacity: usize,
        sessions: &ServerSessionManager,
        policies: &AgentPolicyService,
    ) -> AppResult<Vec<FleetChildLeaseV2>> {
        let snapshot = self.fleet_run(id)?;
        validate_fleet_target_sessions(&snapshot.targets, sessions).await?;
        let approval = self
            .database
            .get_fleet_approval(approval_id)
            .map_err(store_error)?
            .ok_or(AppError::InvalidOperation)?;
        let (policy_version, policy_hash) = policies.command_approval_identity().await;
        let now = super::run::now_epoch_ms();
        let mut fleets = self.active_fleets.lock().map_err(|_| AppError::Storage)?;
        let run = fleets.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        let leases = FleetCoordinatorV2::claim_ready(
            run,
            &approval,
            policy_version,
            &policy_hash,
            capacity,
            now,
        )
        .map_err(fleet_coordinator_error)?;
        let first_seq = self.next_fleet_event_seq(id)?;
        let events = leases
            .iter()
            .enumerate()
            .map(|(offset, _)| FleetEventEnvelopeV2 {
                fleet_run_id: id,
                seq: first_seq + offset as u64,
                timestamp_ms: now,
                kind: FleetEventKindV2::ChildClaimed,
                state: run.state,
                approval_id: Some(approval.id),
                code: None,
            })
            .collect::<Vec<_>>();
        let children = leases
            .iter()
            .map(|lease| AgentRun::new(lease.agent_run_id))
            .collect::<Vec<_>>();
        self.database
            .persist_fleet_schedule(run, &children, &events)
            .map_err(store_error)?;
        Ok(leases)
    }

    /// Starts claimed Fleet children through the normal Runtime V2 controller
    /// and exact Session dispatcher. Every remote command still requires the
    /// child run's standard per-command approval.
    pub(crate) async fn start_fleet_children(
        &self,
        app: AppHandle,
        gateway: Arc<ModelGateway>,
        id: Uuid,
        approval_id: Uuid,
        capacity: usize,
        sessions: &ServerSessionManager,
        policies: &AgentPolicyService,
    ) -> AppResult<Vec<Uuid>> {
        let leases = self
            .claim_fleet_children(id, approval_id, capacity, sessions, policies)
            .await?;
        let fleet = self.fleet_run(id)?;
        let summaries = fleet
            .stages
            .iter()
            .map(|stage| (stage.id, stage.summary.clone()))
            .collect::<HashMap<_, _>>();
        let mut started = Vec::with_capacity(leases.len());
        for lease in leases {
            let goal = summaries
                .get(&lease.stage_id)
                .filter(|summary| !summary.is_empty())
                .cloned()
                .ok_or(AppError::InvalidOperation)?;
            let run = AgentRunStore::get(self.database.as_ref(), lease.agent_run_id)
                .map_err(store_error)?
                .ok_or(AppError::InvalidOperation)?;
            let hints = planning_hints_from_goal(&goal);
            let (cancel_tx, cancel_rx) = watch::channel(false);
            let mut config = self.controller_config(
                app.clone(),
                lease.target.session_id,
                lease.target.profile_id,
                cancel_rx,
            );
            // Fleet fan-out is target-local and bounded independently for each
            // child. Typed reads still pass the normal Tool Registry/Policy;
            // command proposals still require their exact Runtime V2 approval.
            config.budget.max_tool_calls = FLEET_CHILD_TOOL_CALL_BUDGET;
            let reasoner = PlanningReasoner::new(gateway.clone(), hints.clone(), None);
            let dispatcher = SessionToolDispatcher::new(
                app.clone(),
                lease.target.session_id,
                lease.target.profile_id,
                cancel_tx.subscribe(),
            );
            let controller = V2Controller::attach_created(
                run,
                goal,
                reasoner,
                dispatcher,
                self.stores(),
                config,
            )
            .map_err(controller_error)?;
            self.database
                .record_history_target(lease.agent_run_id, lease.target.profile_id)
                .map_err(store_error)?;
            let controller_slot = Arc::new(AsyncMutex::new(Some(controller)));
            self.active.lock().map_err(|_| AppError::Storage)?.insert(
                lease.agent_run_id,
                ActiveRun {
                    cancel: cancel_tx,
                    controller: controller_slot.clone(),
                    session_id: lease.target.session_id,
                    server_id: lease.target.profile_id,
                    hints,
                    host_context: None,
                    fleet_parent: Some(FleetChildParent {
                        fleet_run_id: id,
                        approval_id,
                    }),
                },
            );
            Self::spawn_drive(app.clone(), lease.agent_run_id, controller_slot);
            started.push(lease.agent_run_id);
        }
        Ok(started)
    }

    #[allow(dead_code)]
    pub(crate) fn record_fleet_child_outcome(
        &self,
        id: Uuid,
        agent_run_id: Uuid,
        outcome: FleetChildOutcomeV2,
        error_code: Option<String>,
    ) -> AppResult<FleetRunV2> {
        let now = super::run::now_epoch_ms();
        let mut fleets = self.active_fleets.lock().map_err(|_| AppError::Storage)?;
        let run = fleets.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        FleetCoordinatorV2::record_outcome(run, agent_run_id, outcome, error_code.clone(), now)
            .map_err(fleet_coordinator_error)?;
        let event = FleetEventEnvelopeV2 {
            fleet_run_id: id,
            seq: self.next_fleet_event_seq(id)?,
            timestamp_ms: now,
            kind: FleetEventKindV2::StateChanged,
            state: run.state,
            approval_id: None,
            code: error_code,
        };
        self.database
            .persist_fleet_schedule(run, &[], &[event])
            .map_err(store_error)?;
        Ok(run.clone())
    }

    pub(crate) async fn pause_fleet(&self, id: Uuid) -> AppResult<Vec<Uuid>> {
        let children = self.interrupt_fleet(id, false)?;
        for child in &children {
            // A child at an approval/user interrupt is already quiescent. An
            // in-flight write is allowed to reach its safe boundary first.
            let _ = self.pause(*child).await;
        }
        Ok(children)
    }

    pub(crate) async fn continue_fleet(
        &self,
        app: AppHandle,
        gateway: Arc<ModelGateway>,
        id: Uuid,
        approval_id: Uuid,
        sessions: &ServerSessionManager,
        policies: &AgentPolicyService,
    ) -> AppResult<Vec<Uuid>> {
        let snapshot = self.fleet_run(id)?;
        validate_fleet_target_sessions(&snapshot.targets, sessions).await?;
        let approval = self
            .database
            .get_fleet_approval(approval_id)
            .map_err(store_error)?
            .ok_or(AppError::InvalidOperation)?;
        let (policy_version, policy_hash) = policies.command_approval_identity().await;
        let paused_children = {
            let now = super::run::now_epoch_ms();
            let mut fleets = self.active_fleets.lock().map_err(|_| AppError::Storage)?;
            let run = fleets.get_mut(&id).ok_or(AppError::InvalidOperation)?;
            let paused = FleetCoordinatorV2::continue_after_review(
                run,
                &approval,
                policy_version,
                &policy_hash,
                now,
            )
            .map_err(fleet_coordinator_error)?;
            let event = FleetEventEnvelopeV2 {
                fleet_run_id: id,
                seq: self.next_fleet_event_seq(id)?,
                timestamp_ms: now,
                kind: FleetEventKindV2::StateChanged,
                state: run.state,
                approval_id: Some(approval_id),
                code: Some("FLEET_CONTINUED".into()),
            };
            self.database
                .persist_fleet_schedule(run, &[], &[event])
                .map_err(store_error)?;
            paused
        };
        // Old paused controllers are retired before new target attempts are
        // claimed. This prevents two Runtime V2 runs owning one target step.
        for child in paused_children {
            self.cancel(child).await?;
        }
        self.start_fleet_children(
            app,
            gateway,
            id,
            approval_id,
            MAX_FLEET_ACTIVE_CHILDREN,
            sessions,
            policies,
        )
        .await
    }

    pub(crate) async fn cancel_fleet(&self, id: Uuid) -> AppResult<Vec<Uuid>> {
        let children = self.interrupt_fleet(id, true)?;
        for child in &children {
            self.cancel(*child).await?;
        }
        Ok(children)
    }

    fn interrupt_fleet(&self, id: Uuid, cancel: bool) -> AppResult<Vec<Uuid>> {
        let now = super::run::now_epoch_ms();
        let mut fleets = self.active_fleets.lock().map_err(|_| AppError::Storage)?;
        let run = fleets.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        let children = if cancel {
            FleetCoordinatorV2::cancel(run, now)
        } else {
            FleetCoordinatorV2::pause(run, now)
        }
        .map_err(fleet_coordinator_error)?;
        let event = FleetEventEnvelopeV2 {
            fleet_run_id: id,
            seq: self.next_fleet_event_seq(id)?,
            timestamp_ms: now,
            kind: FleetEventKindV2::StateChanged,
            state: run.state,
            approval_id: None,
            code: None,
        };
        self.database
            .persist_fleet_schedule(run, &[], &[event])
            .map_err(store_error)?;
        Ok(children)
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
        host_context: Option<HostSessionContext>,
    ) -> AppResult<Uuid> {
        let hints = planning_hints_from_goal(&goal);
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let stores = self.stores();
        let config = self.controller_config(app.clone(), session_id, server_id, cancel_rx);
        let reasoner = PlanningReasoner::new(gateway, hints.clone(), host_context.clone());
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
                host_context,
                fleet_parent: None,
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
        approval_id: Uuid,
    ) -> AppResult<()> {
        let pending = ApprovalStore::pending_for_run(self.database.as_ref(), run_id)
            .map_err(store_error)?
            .ok_or(AppError::InvalidOperation)?;
        if pending.id != approval_id {
            return Err(AppError::InvalidOperation);
        }
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
        approval_id: Uuid,
    ) -> AppResult<()> {
        let pending = ApprovalStore::pending_for_run(self.database.as_ref(), run_id)
            .map_err(store_error)?
            .ok_or(AppError::InvalidOperation)?;
        if pending.id != approval_id {
            return Err(AppError::InvalidOperation);
        }
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

    pub async fn retry(
        &self,
        app: AppHandle,
        gateway: Arc<ModelGateway>,
        run_id: Uuid,
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
            controller
                .retry_after_failure()
                .await
                .map_err(controller_error)?
        };
        self.finish_outcome(run_id, outcome).await
    }

    pub async fn cancel(&self, run_id: Uuid) -> AppResult<()> {
        if let Ok(active) = self.active_entry(run_id) {
            // Signal first so an in-flight drive_loop observes cancellation at
            // the next yield / loop boundary without waiting on this lock.
            let _ = active.cancel.send(true);
            match active.controller.try_lock() {
                Ok(mut guard) => {
                    if let Some(controller) = guard.as_mut() {
                        let _ = controller.cancel();
                    }
                    *guard = None;
                    self.active
                        .lock()
                        .map_err(|_| AppError::Storage)?
                        .remove(&run_id);
                }
                Err(_) => {
                    // drive_loop holds the controller; it will finish_cancelled
                    // after seeing the watch flag and finish_outcome removes it.
                }
            }
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
                host_context: None,
                fleet_parent: None,
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
                host_context: entry.host_context.clone(),
                fleet_parent: entry.fleet_parent,
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
        // Must share ActiveRun.cancel so Stop mid-resume still reaches the loop.
        let cancel_rx = active.cancel.subscribe();
        let stores = self.stores();
        let config =
            self.controller_config(app.clone(), active.session_id, active.server_id, cancel_rx);
        let reasoner =
            PlanningReasoner::new(gateway, active.hints.clone(), active.host_context.clone());
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
            let outcome_result = {
                let mut guard = controller_slot.lock().await;
                let Some(controller) = guard.as_mut() else {
                    return;
                };
                controller.run_to_interrupt().await
            };
            let service = app.state::<AgentRuntimeV2Service>();
            let parent = service
                .active_entry(run_id)
                .ok()
                .and_then(|entry| entry.fleet_parent);
            let outcome = match outcome_result {
                Ok(outcome) => outcome,
                Err(error) => {
                    if let Some(parent) = parent {
                        let _ = service.record_fleet_child_outcome(
                            parent.fleet_run_id,
                            run_id,
                            FleetChildOutcomeV2::Failed,
                            Some(error.code().into()),
                        );
                    }
                    if let Ok(mut active) = service.active.lock() {
                        active.remove(&run_id);
                    }
                    return;
                }
            };
            if service.finish_outcome(run_id, outcome).await.is_err() {
                return;
            }
            if let Some(parent) = parent {
                let gateway = app.state::<Arc<ModelGateway>>();
                let sessions = app.state::<ServerSessionManager>();
                let policies = app.state::<AgentPolicyService>();
                let _ = service
                    .start_fleet_children(
                        app.clone(),
                        gateway.inner().clone(),
                        parent.fleet_run_id,
                        parent.approval_id,
                        MAX_FLEET_ACTIVE_CHILDREN,
                        &sessions,
                        &policies,
                    )
                    .await;
            }
        });
    }

    async fn finish_outcome(&self, run_id: Uuid, outcome: RunOutcome) -> AppResult<()> {
        let fleet_binding = self.active.lock().ok().and_then(|active| {
            active
                .get(&run_id)
                .and_then(|entry| entry.fleet_parent.map(|parent| (parent, entry.server_id)))
        });
        if let Some((parent, target_id)) = fleet_binding {
            // Checkpoint facts are the sole cross-target aggregation source.
            // Observation detail and terminal previews never enter this path.
            if let Some(checkpoint) =
                CheckpointStore::get(self.database.as_ref(), run_id).map_err(store_error)?
            {
                if let Ok(facts) =
                    super::facts::WorkingFactSet::from_snapshot_json(&checkpoint.fact_snapshot)
                {
                    if let Ok(mut fleets) = self.active_fleet_facts.lock() {
                        if let Some(fleet) = fleets.get_mut(&parent.fleet_run_id) {
                            let _ = fleet.merge(target_id, &facts, super::run::now_epoch_ms());
                        }
                    }
                }
            }
            let child_outcome = match &outcome {
                RunOutcome::Completed { .. } => Some((FleetChildOutcomeV2::Succeeded, None)),
                RunOutcome::Cancelled => Some((FleetChildOutcomeV2::Cancelled, None)),
                RunOutcome::Failed { error_code } => {
                    Some((FleetChildOutcomeV2::Failed, Some((*error_code).into())))
                }
                RunOutcome::AwaitingUser { .. } | RunOutcome::AwaitingApproval { .. } => None,
            };
            if let Some((child_outcome, error_code)) = child_outcome {
                if let Ok(fleet) = self.record_fleet_child_outcome(
                    parent.fleet_run_id,
                    run_id,
                    child_outcome,
                    error_code,
                ) {
                    self.pause_blocked_fleet_children(&fleet, run_id).await;
                }
            }
        }
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

    async fn pause_blocked_fleet_children(&self, fleet: &FleetRunV2, completed: Uuid) {
        if fleet.state != FleetRunStateV2::PausedForReview {
            return;
        }
        for child in fleet.children.iter().filter(|child| {
            child.agent_run_id != Some(completed)
                && child.state == super::fleet_run::FleetStageStateV2::Blocked
        }) {
            if let Some(run_id) = child.agent_run_id {
                let _ = self.pause(run_id).await;
            }
        }
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

fn fleet_coordinator_error(_error: FleetCoordinatorError) -> AppError {
    AppError::InvalidOperation
}

fn fleet_plan_error(error: FleetPlanError) -> AppError {
    match error {
        FleetPlanError::InvalidTargets => AppError::AgentFleetTargetInvalid,
        FleetPlanError::DependencyCycle => AppError::AgentFleetPlanCycle,
        FleetPlanError::ProductionParallel => AppError::AgentFleetProductionParallelBlocked,
        FleetPlanError::InvalidStages
        | FleetPlanError::UnknownTarget
        | FleetPlanError::InvalidDependency
        | FleetPlanError::InvalidTransition => AppError::AgentFleetPlanInvalid,
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
    use crate::agent::fleet_run::FleetExecutionStrategyV2;

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

    #[tokio::test]
    async fn fleet_cancellation_is_durable_and_never_resumes_after_restart() {
        let directory = tempfile::tempdir().expect("temporary database directory");
        let targets = (0..2)
            .map(|ordinal| FleetTargetBinding {
                profile_id: Uuid::new_v4(),
                session_id: Uuid::new_v4(),
                role: None,
                ordinal,
            })
            .collect::<Vec<_>>();
        let service = AgentRuntimeV2Service::open(directory.path()).expect("open service");
        let run = service
            .create_fleet_draft(
                true,
                FleetFailurePolicyV2::PauseForReview,
                targets.clone(),
                vec![FleetStageDraft {
                    id: Uuid::new_v4(),
                    summary: "Inspect exact targets".into(),
                    target_ids: targets.iter().map(|target| target.profile_id).collect(),
                    depends_on: vec![],
                    execution_strategy: FleetExecutionStrategyV2::Sequential,
                    concurrency_limit: 1,
                }],
            )
            .expect("create Fleet draft");

        assert!(service
            .cancel_fleet(run.id)
            .await
            .expect("cancel")
            .is_empty());
        assert_eq!(
            service.fleet_run(run.id).expect("cancelled run").state,
            FleetRunStateV2::Cancelled
        );
        drop(service);

        let recovered = AgentRuntimeV2Service::open(directory.path()).expect("reopen service");
        let loaded = recovered.fleet_run(run.id).expect("load cancelled run");
        assert_eq!(loaded.state, FleetRunStateV2::Cancelled);
        assert_eq!(
            loaded.recovery_state,
            super::super::fleet_run::FleetRecoveryStateV2::MetadataOnly
        );
    }
}
