use tauri::State;

use crate::ai::AiService;
use crate::cloud::{CloudPolicyAction, CloudPolicyService};
use crate::domain::{
    AiAssistantResponse, AiCommandRequest, AiGenerateRequest, AiOutputRequest, AppResult,
};
use crate::ssh::ServerSessionManager;

#[tauri::command]
pub async fn ai_explain_command(
    request: AiCommandRequest,
    assistant: State<'_, AiService>,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<AiAssistantResponse> {
    policies
        .authorize(
            sessions.profile_id(request.session_id).await?,
            CloudPolicyAction::AiExecute,
        )
        .await?;
    assistant.explain(request.command)
}

#[tauri::command]
pub async fn ai_generate_command(
    request: AiGenerateRequest,
    assistant: State<'_, AiService>,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<AiAssistantResponse> {
    policies
        .authorize(
            sessions.profile_id(request.session_id).await?,
            CloudPolicyAction::AiExecute,
        )
        .await?;
    assistant.generate(request.intent).await
}

#[tauri::command]
pub async fn ai_diagnose_output(
    request: AiOutputRequest,
    assistant: State<'_, AiService>,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<AiAssistantResponse> {
    policies
        .authorize(
            sessions.profile_id(request.session_id).await?,
            CloudPolicyAction::AiExecute,
        )
        .await?;
    assistant
        .diagnose(&sessions, request.session_id, request.output, false)
        .await
}

#[tauri::command]
pub async fn ai_propose_fix(
    request: AiOutputRequest,
    assistant: State<'_, AiService>,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<AiAssistantResponse> {
    policies
        .authorize(
            sessions.profile_id(request.session_id).await?,
            CloudPolicyAction::AiExecute,
        )
        .await?;
    assistant
        .diagnose(&sessions, request.session_id, request.output, true)
        .await
}
