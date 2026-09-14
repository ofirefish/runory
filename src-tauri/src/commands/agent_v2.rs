use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{ipc::Channel, AppHandle, State};
use uuid::Uuid;

use crate::agent::{
    validate_fleet_target_sessions, validate_fleet_target_shape, AgentEvent, AgentEventEnvelope,
    AgentRun, AgentRuntimeV2Service, FleetApprovalV2, FleetChangeSetDraftRequest,
    FleetChangeSetReview, FleetEventEnvelopeV2, FleetExecutionStrategyV2, FleetFailurePolicyV2,
    FleetInvestigationView, FleetRunV2, FleetStageDraft, FleetTargetBinding, FleetTargetRequest,
    HostSessionContext,
};
use crate::agentic::{ChangeSetService, FleetExecutionService, ModelGateway, MultiChangeSet};
use crate::domain::{AppResult, SessionId};
use crate::policy::AgentPolicyService;
use crate::profiles::ProfileService;
use crate::ssh::ServerSessionManager;
use crate::tools::NativeToolExecutionService;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentV2StartRequest {
    pub session_id: SessionId,
    pub goal: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentV2SubscribeRequest {
    pub run_id: Uuid,
    #[serde(default)]
    pub after_seq: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentV2RunActionRequest {
    pub run_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentV2ApprovalActionRequest {
    pub run_id: Uuid,
    pub approval_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentV2ReplyRequest {
    pub run_id: Uuid,
    pub text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentV2ResumableRun {
    pub run: AgentRun,
    pub goal: String,
    pub target_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentV2StartResponse {
    pub run_id: Uuid,
    pub context: AgentV2DisplayContext,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentV2FleetTargetValidationRequest {
    pub targets: Vec<FleetTargetRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentV2FleetPlanDraftRequest {
    pub targets: Vec<FleetTargetRequest>,
    pub stages: Vec<FleetStageDraft>,
    pub production: bool,
    pub failure_policy: FleetFailurePolicyV2,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentV2FleetPromptDraftRequest {
    pub targets: Vec<FleetTargetRequest>,
    pub goal: String,
    pub production: bool,
    pub execution_strategy: FleetExecutionStrategyV2,
    pub failure_policy: FleetFailurePolicyV2,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentV2FleetApprovalRequest {
    pub fleet_run_id: Uuid,
    pub version: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentV2FleetApprovalActionRequest {
    pub fleet_run_id: Uuid,
    pub approval_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentV2FleetStartRequest {
    pub fleet_run_id: Uuid,
    pub approval_id: Uuid,
    pub capacity: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentV2FleetContinueRequest {
    pub fleet_run_id: Uuid,
    pub approval_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentV2FleetChangeActionRequest {
    pub fleet_run_id: Uuid,
    pub fleet_run_version: u64,
    pub execution_id: Uuid,
    pub execution_version: u64,
}

async fn revalidate_fleet_run_sessions(
    run: &FleetRunV2,
    sessions: &ServerSessionManager,
) -> AppResult<()> {
    validate_fleet_target_sessions(&run.targets, sessions).await
}

async fn exact_fleet_bindings(
    targets: Vec<FleetTargetRequest>,
    sessions: &ServerSessionManager,
) -> AppResult<Vec<FleetTargetBinding>> {
    let roles = validate_fleet_target_shape(&targets)?;
    let mut bindings = Vec::with_capacity(targets.len());
    for (ordinal, (target, role)) in targets.into_iter().zip(roles).enumerate() {
        if sessions.profile_id(target.session_id).await? != target.profile_id {
            return Err(crate::domain::AppError::AgentFleetTargetMismatch);
        }
        bindings.push(FleetTargetBinding {
            profile_id: target.profile_id,
            session_id: target.session_id,
            role,
            ordinal,
        });
    }
    Ok(bindings)
}

/// Revalidates frontend-resolved mentions against live Rust-owned sessions.
/// This command only returns exact bindings; it creates no run and performs no
/// remote action.
#[tauri::command]
pub async fn agent_v2_fleet_validate_targets(
    request: AgentV2FleetTargetValidationRequest,
    sessions: State<'_, ServerSessionManager>,
) -> AppResult<Vec<FleetTargetBinding>> {
    exact_fleet_bindings(request.targets, &sessions).await
}

/// Creates a validated live Fleet draft. It schedules no child run and causes
/// no remote side effect; later milestones attach the coordinator and approval
/// flow to this domain object.
#[tauri::command]
pub async fn agent_v2_fleet_plan_draft(
    request: AgentV2FleetPlanDraftRequest,
    runtime: State<'_, AgentRuntimeV2Service>,
    sessions: State<'_, ServerSessionManager>,
) -> AppResult<FleetRunV2> {
    let targets = exact_fleet_bindings(request.targets, &sessions).await?;
    runtime.create_fleet_draft(
        request.production,
        request.failure_policy,
        targets,
        request.stages,
    )
}

/// Creates the initial single-stage Fleet graph for an explicit multi-target
/// user request. React submits intent and exact candidate bindings only; Rust
/// owns stage identity, concurrency limits and production strategy validation.
#[tauri::command]
pub async fn agent_v2_fleet_prompt_draft(
    request: AgentV2FleetPromptDraftRequest,
    runtime: State<'_, AgentRuntimeV2Service>,
    sessions: State<'_, ServerSessionManager>,
) -> AppResult<FleetRunV2> {
    let goal = request.goal.trim();
    if goal.is_empty() || goal.len() > 4_000 {
        return Err(crate::domain::AppError::InvalidOperation);
    }
    let targets = exact_fleet_bindings(request.targets, &sessions).await?;
    let concurrency_limit = match request.execution_strategy {
        FleetExecutionStrategyV2::Sequential | FleetExecutionStrategyV2::Canary => 1,
        FleetExecutionStrategyV2::RollingBatch => targets.len().min(2),
        FleetExecutionStrategyV2::Parallel => targets.len(),
    };
    let target_ids = targets.iter().map(|target| target.profile_id).collect();
    runtime.create_fleet_draft(
        request.production,
        request.failure_policy,
        targets,
        vec![FleetStageDraft {
            id: Uuid::new_v4(),
            summary: goal.to_owned(),
            target_ids,
            depends_on: vec![],
            execution_strategy: request.execution_strategy,
            concurrency_limit,
        }],
    )
}

#[tauri::command]
pub fn agent_v2_fleet_plan_get(
    fleet_run_id: Uuid,
    runtime: State<'_, AgentRuntimeV2Service>,
) -> AppResult<FleetRunV2> {
    runtime.fleet_run(fleet_run_id)
}

/// Live, bounded structured facts only. This endpoint cannot return terminal
/// transcripts, model reasoning, commands or raw remote output.
#[tauri::command]
pub fn agent_v2_fleet_investigation_get(
    fleet_run_id: Uuid,
    runtime: State<'_, AgentRuntimeV2Service>,
) -> AppResult<FleetInvestigationView> {
    runtime.fleet_investigation(fleet_run_id)
}

/// Loads only an existing live ChangeSet execution that is cryptographically
/// bound to the exact Fleet graph. Absence is not treated as an empty or
/// synthetic proposal.
#[tauri::command]
pub async fn agent_v2_fleet_changeset_latest(
    fleet_run_id: Uuid,
    runtime: State<'_, AgentRuntimeV2Service>,
    changes: State<'_, ChangeSetService>,
    fleets: State<'_, FleetExecutionService>,
) -> AppResult<Option<FleetChangeSetReview>> {
    let run = runtime.fleet_run(fleet_run_id)?;
    FleetChangeSetReview::latest_bound(&run, &changes, &fleets).await
}

/// Drafts one independent typed ChangeSet per exact Fleet target and returns
/// the real step previews, risks, rollback capabilities and policy results.
#[tauri::command]
pub async fn agent_v2_fleet_changeset_draft(
    request: FleetChangeSetDraftRequest,
    runtime: State<'_, AgentRuntimeV2Service>,
    changes: State<'_, ChangeSetService>,
    fleets: State<'_, FleetExecutionService>,
    policies: State<'_, AgentPolicyService>,
    sessions: State<'_, ServerSessionManager>,
    tools: State<'_, NativeToolExecutionService>,
) -> AppResult<FleetChangeSetReview> {
    let run = runtime.fleet_run(request.fleet_run_id)?;
    revalidate_fleet_run_sessions(&run, &sessions).await?;
    FleetChangeSetReview::draft(&run, request, &changes, &fleets, &policies)
        .await?
        .capture_preconditions(&changes, &sessions, &tools)
        .await
}

/// Seals the previously reviewed target-local preconditions and exact
/// ChangeSet versions through the established Fleet approval boundary.
#[tauri::command]
pub async fn agent_v2_fleet_changeset_approve(
    request: AgentV2FleetChangeActionRequest,
    runtime: State<'_, AgentRuntimeV2Service>,
    changes: State<'_, ChangeSetService>,
    fleets: State<'_, FleetExecutionService>,
    sessions: State<'_, ServerSessionManager>,
) -> AppResult<MultiChangeSet> {
    let run = runtime.fleet_run(request.fleet_run_id)?;
    if run.version != request.fleet_run_version {
        return Err(crate::domain::AppError::InvalidOperation);
    }
    revalidate_fleet_run_sessions(&run, &sessions).await?;
    FleetChangeSetReview::approve(
        &run,
        request.execution_id,
        request.execution_version,
        &changes,
        &fleets,
    )
    .await
}

/// Executes only a previously approved, still-exact Fleet ChangeSet through
/// its existing sequential/canary/rolling verification pipeline.
#[tauri::command]
pub async fn agent_v2_fleet_changeset_execute(
    request: AgentV2FleetChangeActionRequest,
    runtime: State<'_, AgentRuntimeV2Service>,
    changes: State<'_, ChangeSetService>,
    fleets: State<'_, FleetExecutionService>,
    policies: State<'_, AgentPolicyService>,
    sessions: State<'_, ServerSessionManager>,
    tools: State<'_, NativeToolExecutionService>,
) -> AppResult<MultiChangeSet> {
    let run = runtime.fleet_run(request.fleet_run_id)?;
    if run.version != request.fleet_run_version {
        return Err(crate::domain::AppError::InvalidOperation);
    }
    revalidate_fleet_run_sessions(&run, &sessions).await?;
    FleetChangeSetReview::execute(
        &run,
        request.execution_id,
        request.execution_version,
        &changes,
        &fleets,
        &sessions,
        &tools,
        &policies,
    )
    .await
}

#[tauri::command]
pub fn agent_v2_fleet_plan_list(
    runtime: State<'_, AgentRuntimeV2Service>,
) -> AppResult<Vec<FleetRunV2>> {
    runtime.fleet_runs()
}

/// Seals the current Fleet graph for review. This does not start execution.
#[tauri::command]
pub async fn agent_v2_fleet_plan_request_approval(
    request: AgentV2FleetApprovalRequest,
    runtime: State<'_, AgentRuntimeV2Service>,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, AgentPolicyService>,
) -> AppResult<FleetApprovalV2> {
    let run = runtime.fleet_run(request.fleet_run_id)?;
    revalidate_fleet_run_sessions(&run, &sessions).await?;
    let (policy_version, policy_hash) = policies.command_approval_identity().await;
    runtime.request_fleet_approval(
        request.fleet_run_id,
        request.version,
        policy_version,
        policy_hash,
    )
}

#[tauri::command]
pub async fn agent_v2_fleet_plan_approve(
    request: AgentV2FleetApprovalActionRequest,
    runtime: State<'_, AgentRuntimeV2Service>,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, AgentPolicyService>,
) -> AppResult<FleetApprovalV2> {
    let run = runtime.fleet_run(request.fleet_run_id)?;
    revalidate_fleet_run_sessions(&run, &sessions).await?;
    let (policy_version, policy_hash) = policies.command_approval_identity().await;
    runtime.decide_fleet_approval(
        request.fleet_run_id,
        request.approval_id,
        true,
        policy_version,
        &policy_hash,
    )
}

#[tauri::command]
pub async fn agent_v2_fleet_plan_reject(
    request: AgentV2FleetApprovalActionRequest,
    runtime: State<'_, AgentRuntimeV2Service>,
    policies: State<'_, AgentPolicyService>,
) -> AppResult<FleetApprovalV2> {
    let (policy_version, policy_hash) = policies.command_approval_identity().await;
    runtime.decide_fleet_approval(
        request.fleet_run_id,
        request.approval_id,
        false,
        policy_version,
        &policy_hash,
    )
}

#[tauri::command]
pub fn agent_v2_fleet_events_after(
    fleet_run_id: Uuid,
    after_seq: u64,
    runtime: State<'_, AgentRuntimeV2Service>,
) -> AppResult<Vec<FleetEventEnvelopeV2>> {
    runtime.fleet_events_after(fleet_run_id, after_seq)
}

/// Explicitly starts the next scheduler batch after Fleet approval. Child
/// commands remain individually approval-gated by Runtime V2.
#[tauri::command]
pub async fn agent_v2_fleet_plan_start(
    request: AgentV2FleetStartRequest,
    app: AppHandle,
    runtime: State<'_, AgentRuntimeV2Service>,
    gateway: State<'_, Arc<ModelGateway>>,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, AgentPolicyService>,
) -> AppResult<Vec<Uuid>> {
    runtime
        .start_fleet_children(
            app,
            gateway.inner().clone(),
            request.fleet_run_id,
            request.approval_id,
            request.capacity,
            &sessions,
            &policies,
        )
        .await
}

/// Rolls back only completed targets with real ChangeSet rollback payloads.
/// The exact Fleet graph/session binding and execution version are rechecked
/// before any rollback tool is invoked.
#[tauri::command]
pub async fn agent_v2_fleet_changeset_rollback(
    request: AgentV2FleetChangeActionRequest,
    runtime: State<'_, AgentRuntimeV2Service>,
    changes: State<'_, ChangeSetService>,
    fleets: State<'_, FleetExecutionService>,
    sessions: State<'_, ServerSessionManager>,
    tools: State<'_, NativeToolExecutionService>,
) -> AppResult<MultiChangeSet> {
    let run = runtime.fleet_run(request.fleet_run_id)?;
    if run.version != request.fleet_run_version {
        return Err(crate::domain::AppError::InvalidOperation);
    }
    revalidate_fleet_run_sessions(&run, &sessions).await?;
    FleetChangeSetReview::rollback(
        &run,
        request.execution_id,
        request.execution_version,
        &changes,
        &fleets,
        &sessions,
        &tools,
    )
    .await
}

#[tauri::command]
pub async fn agent_v2_fleet_plan_pause(
    fleet_run_id: Uuid,
    runtime: State<'_, AgentRuntimeV2Service>,
) -> AppResult<Vec<Uuid>> {
    runtime.pause_fleet(fleet_run_id).await
}

/// Continues the same live, approved Fleet graph after explicit review. Rust
/// revalidates every target/session and policy binding before retiring paused
/// child controllers and creating bounded fresh attempts.
#[tauri::command]
pub async fn agent_v2_fleet_plan_continue(
    request: AgentV2FleetContinueRequest,
    app: AppHandle,
    runtime: State<'_, AgentRuntimeV2Service>,
    gateway: State<'_, Arc<ModelGateway>>,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, AgentPolicyService>,
) -> AppResult<Vec<Uuid>> {
    runtime
        .continue_fleet(
            app,
            gateway.inner().clone(),
            request.fleet_run_id,
            request.approval_id,
            &sessions,
            &policies,
        )
        .await
}

#[tauri::command]
pub async fn agent_v2_fleet_plan_cancel(
    fleet_run_id: Uuid,
    runtime: State<'_, AgentRuntimeV2Service>,
) -> AppResult<Vec<Uuid>> {
    runtime.cancel_fleet(fleet_run_id).await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentV2DisplayContext {
    pub os: String,
    pub user: String,
    pub directory: String,
}

#[tauri::command]
pub async fn agent_v2_run_start(
    request: AgentV2StartRequest,
    app: AppHandle,
    runtime: State<'_, AgentRuntimeV2Service>,
    gateway: State<'_, Arc<ModelGateway>>,
    sessions: State<'_, ServerSessionManager>,
    profiles: State<'_, ProfileService>,
) -> AppResult<AgentV2StartResponse> {
    let server_id = sessions.profile_id(request.session_id).await?;
    let profile = profiles.get(server_id).await?;
    let directory = sessions
        .current_terminal_directory(request.session_id, &profile.username)
        .await?
        .unwrap_or_else(|| "unknown".into());
    let os = profile
        .os_distribution
        .map(|value| value.display_label().to_owned())
        .unwrap_or_else(|| "Linux".into());
    let host_context = HostSessionContext {
        os: os.clone(),
        user: profile.username.clone(),
        directory: directory.clone(),
        system_info: sessions.host_system_info(request.session_id).await,
    };
    let run_id = runtime
        .start_run(
            app,
            gateway.inner().clone(),
            request.session_id,
            server_id,
            request.goal,
            Some(host_context),
        )
        .await?;
    Ok(AgentV2StartResponse {
        run_id,
        context: AgentV2DisplayContext {
            os,
            user: profile.username,
            directory,
        },
    })
}

#[tauri::command]
pub async fn agent_v2_run_subscribe(
    request: AgentV2SubscribeRequest,
    on_event: Channel<AgentEventEnvelope>,
    runtime: State<'_, AgentRuntimeV2Service>,
) -> AppResult<()> {
    for envelope in runtime.events_after(request.run_id, request.after_seq)? {
        on_event
            .send(envelope)
            .map_err(|_| crate::domain::AppError::InvalidOperation)?;
    }
    let mut receiver = runtime.subscribe();
    tauri::async_runtime::spawn(async move {
        while let Ok(envelope) = receiver.recv().await {
            if envelope.run_id != request.run_id {
                continue;
            }
            if on_event.send(envelope).is_err() {
                break;
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn agent_v2_run_approve(
    request: AgentV2ApprovalActionRequest,
    app: AppHandle,
    runtime: State<'_, AgentRuntimeV2Service>,
    gateway: State<'_, Arc<ModelGateway>>,
) -> AppResult<()> {
    runtime
        .approve(
            app,
            gateway.inner().clone(),
            request.run_id,
            request.approval_id,
        )
        .await
}

#[tauri::command]
pub async fn agent_v2_run_reject(
    request: AgentV2ApprovalActionRequest,
    app: AppHandle,
    runtime: State<'_, AgentRuntimeV2Service>,
    gateway: State<'_, Arc<ModelGateway>>,
) -> AppResult<()> {
    runtime
        .reject(
            app,
            gateway.inner().clone(),
            request.run_id,
            request.approval_id,
        )
        .await
}

#[tauri::command]
pub async fn agent_v2_run_reply(
    request: AgentV2ReplyRequest,
    app: AppHandle,
    runtime: State<'_, AgentRuntimeV2Service>,
    gateway: State<'_, Arc<ModelGateway>>,
) -> AppResult<()> {
    runtime
        .reply(app, gateway.inner().clone(), request.run_id, request.text)
        .await
}

#[tauri::command]
pub async fn agent_v2_run_retry(
    request: AgentV2RunActionRequest,
    app: AppHandle,
    runtime: State<'_, AgentRuntimeV2Service>,
    gateway: State<'_, Arc<ModelGateway>>,
) -> AppResult<()> {
    runtime
        .retry(app, gateway.inner().clone(), request.run_id)
        .await
}

#[tauri::command]
pub async fn agent_v2_run_cancel(
    run_id: Uuid,
    runtime: State<'_, AgentRuntimeV2Service>,
) -> AppResult<()> {
    runtime.cancel(run_id).await
}

#[tauri::command]
pub async fn agent_v2_run_pause(
    request: AgentV2RunActionRequest,
    runtime: State<'_, AgentRuntimeV2Service>,
) -> AppResult<()> {
    runtime.pause(request.run_id).await
}

#[tauri::command]
pub async fn agent_v2_run_resume(
    request: AgentV2RunActionRequest,
    app: AppHandle,
    runtime: State<'_, AgentRuntimeV2Service>,
    gateway: State<'_, Arc<ModelGateway>>,
) -> AppResult<()> {
    runtime
        .resume(app, gateway.inner().clone(), request.run_id)
        .await
}

#[tauri::command]
pub async fn agent_v2_list_resumable_runs(
    runtime: State<'_, AgentRuntimeV2Service>,
) -> AppResult<Vec<AgentV2ResumableRun>> {
    let runs = runtime.list_resumable_runs()?;
    let mut items = Vec::new();
    for run in runs {
        let events = runtime.events_after(run.id(), 0)?;
        let goal = events
            .iter()
            .find_map(|envelope| match &envelope.event {
                AgentEvent::UserMessageAdded { content } => Some(content.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let target_ids = runtime.resumable_target_ids(run.id())?;
        items.push(AgentV2ResumableRun {
            run,
            goal,
            target_ids,
        });
    }
    Ok(items)
}

#[tauri::command]
pub async fn agent_v2_history_list(
    target_id: Option<Uuid>,
    runtime: State<'_, AgentRuntimeV2Service>,
) -> AppResult<Vec<crate::agent::AgentHistoryEntry>> {
    runtime.recent_history(target_id)
}

#[tauri::command]
pub async fn agent_v2_history_get(
    run_id: Uuid,
    runtime: State<'_, AgentRuntimeV2Service>,
) -> AppResult<crate::agent::AgentHistoryDetail> {
    runtime.history_detail(run_id)
}

#[tauri::command]
pub async fn agent_v2_bind_resumable_run(
    run_id: Uuid,
    session_id: SessionId,
    runtime: State<'_, AgentRuntimeV2Service>,
    sessions: State<'_, ServerSessionManager>,
) -> AppResult<()> {
    let server_id = sessions.profile_id(session_id).await?;
    let events = runtime.events_after(run_id, 0)?;
    let goal = events
        .iter()
        .find_map(|envelope| match &envelope.event {
            AgentEvent::UserMessageAdded { content } => Some(content.as_str()),
            _ => None,
        })
        .unwrap_or("");
    runtime.register_resumable(run_id, session_id, server_id, goal)
}
