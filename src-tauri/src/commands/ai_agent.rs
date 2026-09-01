use tauri::State;

use crate::ai::AiAgentService;
use crate::cloud::{CloudPolicyAction, CloudPolicyService};
use crate::domain::{
    AiAgentPlan, AiAuditListRequest, AiAuditRecord, AiPlanGetRequest, AiPlanRequest,
    AiPlanStepRequest, AiToolExecution, AppResult,
};
use crate::ssh::ServerSessionManager;

#[tauri::command]
pub async fn ai_agent_plan_create(
    request: AiPlanRequest,
    agent: State<'_, AiAgentService>,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<AiAgentPlan> {
    for session_id in &request.session_ids {
        policies
            .authorize(
                sessions.profile_id(*session_id).await?,
                CloudPolicyAction::AiExecute,
            )
            .await?;
    }
    agent.create_plan(&sessions, request).await
}

#[tauri::command]
pub async fn ai_agent_plan_get(
    request: AiPlanGetRequest,
    agent: State<'_, AiAgentService>,
) -> AppResult<AiAgentPlan> {
    agent.get_plan(request.plan_id).await
}

#[tauri::command]
pub async fn ai_agent_plan_discard(
    request: AiPlanGetRequest,
    agent: State<'_, AiAgentService>,
) -> AppResult<()> {
    agent.discard_plan(request.plan_id).await
}

#[tauri::command]
pub async fn ai_agent_step_approve(
    request: AiPlanStepRequest,
    agent: State<'_, AiAgentService>,
) -> AppResult<AiAgentPlan> {
    agent.approve_step(request.plan_id, request.step_id).await
}

#[tauri::command]
pub async fn ai_agent_step_execute(
    request: AiPlanStepRequest,
    agent: State<'_, AiAgentService>,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<AiToolExecution> {
    let plan = agent.get_plan(request.plan_id).await?;
    let session_id = plan
        .steps
        .iter()
        .find(|step| step.id == request.step_id)
        .map(|step| step.session_id)
        .ok_or(crate::domain::AppError::InvalidOperation)?;
    policies
        .authorize(
            sessions.profile_id(session_id).await?,
            CloudPolicyAction::AiExecute,
        )
        .await?;
    agent
        .execute_step(&sessions, request.plan_id, request.step_id)
        .await
}

#[tauri::command]
pub async fn ai_agent_audit_list(
    request: AiAuditListRequest,
    agent: State<'_, AiAgentService>,
) -> AppResult<Vec<AiAuditRecord>> {
    agent.audit(request.profile_id).await
}
