use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{ipc::Channel, AppHandle, State};
use uuid::Uuid;

use crate::agent::{
    validate_fleet_target_shape, AgentEvent, AgentEventEnvelope, AgentRun, AgentRuntimeV2Service,
    FleetFailurePolicyV2, FleetRunV2, FleetStageDraft, FleetTargetBinding, FleetTargetRequest,
    HostSessionContext,
};
use crate::agentic::ModelGateway;
use crate::domain::{AppResult, SessionId};
use crate::profiles::ProfileService;
use crate::ssh::ServerSessionManager;

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

#[tauri::command]
pub fn agent_v2_fleet_plan_get(
    fleet_run_id: Uuid,
    runtime: State<'_, AgentRuntimeV2Service>,
) -> AppResult<FleetRunV2> {
    runtime.fleet_run(fleet_run_id)
}

#[tauri::command]
pub fn agent_v2_fleet_plan_list(
    runtime: State<'_, AgentRuntimeV2Service>,
) -> AppResult<Vec<FleetRunV2>> {
    runtime.fleet_runs()
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
