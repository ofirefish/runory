use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{ipc::Channel, AppHandle, State};
use uuid::Uuid;

use crate::agent::{AgentEvent, AgentEventEnvelope, AgentRun, AgentRuntimeV2Service};
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
        .unwrap_or_else(|| {
            if profile.username == "root" {
                "/root".into()
            } else {
                format!("/home/{}", profile.username)
            }
        });
    let run_id = runtime
        .start_run(
            app,
            gateway.inner().clone(),
            request.session_id,
            server_id,
            request.goal,
        )
        .await?;
    Ok(AgentV2StartResponse {
        run_id,
        context: AgentV2DisplayContext {
            os: "Linux".into(),
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
    request: AgentV2RunActionRequest,
    app: AppHandle,
    runtime: State<'_, AgentRuntimeV2Service>,
    gateway: State<'_, Arc<ModelGateway>>,
) -> AppResult<()> {
    runtime
        .approve(app, gateway.inner().clone(), request.run_id)
        .await
}

#[tauri::command]
pub async fn agent_v2_run_reject(
    request: AgentV2RunActionRequest,
    app: AppHandle,
    runtime: State<'_, AgentRuntimeV2Service>,
    gateway: State<'_, Arc<ModelGateway>>,
) -> AppResult<()> {
    runtime
        .reject(app, gateway.inner().clone(), request.run_id)
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
