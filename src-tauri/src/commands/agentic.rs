use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use crate::agentic::{
    ChangeSetService, FleetExecutionService, Incident, IncidentAuditExport, IncidentChangeSet,
    IncidentClosureRequest, IncidentHandoffRequest, IncidentRequest, IncidentService,
    ModelConfigureRequest, ModelGateway, ModelProfile, ModelProviderStatus, OauthProvider,
    ObservationCache,
};
use crate::credentials::CredentialService;
use crate::domain::AppResult;
use crate::mcp::{McpConfigureRequest, McpGateway, McpServerConfig};
use crate::policy::{AgentPolicyService, EffectiveAgentPolicy, PolicyTarget};
use crate::skills::{Skill, SkillRegistry};
use crate::ssh::ServerSessionManager;
use crate::tools::NativeToolExecutionService;

#[tauri::command]
pub async fn agent_model_profiles(
    models: State<'_, Arc<ModelGateway>>,
) -> AppResult<Vec<ModelProfile>> {
    models.profiles().await
}

#[tauri::command]
pub async fn agent_model_profile_save(
    id: Option<Uuid>,
    request: ModelConfigureRequest,
    models: State<'_, Arc<ModelGateway>>,
) -> AppResult<Vec<ModelProfile>> {
    models.save_profile(id, request).await
}

#[tauri::command]
pub async fn agent_model_profile_activate(
    id: Uuid,
    models: State<'_, Arc<ModelGateway>>,
) -> AppResult<Vec<ModelProfile>> {
    models.activate_profile(id).await
}

#[tauri::command]
pub async fn agent_model_profile_remove(
    id: Uuid,
    models: State<'_, Arc<ModelGateway>>,
) -> AppResult<Vec<ModelProfile>> {
    models.remove_profile(id).await
}

#[tauri::command]
pub async fn agent_model_get(
    models: State<'_, Arc<ModelGateway>>,
) -> AppResult<ModelProviderStatus> {
    models.status().await
}

#[tauri::command]
pub async fn agent_model_configure(
    request: ModelConfigureRequest,
    models: State<'_, Arc<ModelGateway>>,
) -> AppResult<ModelProviderStatus> {
    models.configure(request).await
}

#[tauri::command]
pub async fn agent_model_clear_api_key(
    models: State<'_, Arc<ModelGateway>>,
) -> AppResult<ModelProviderStatus> {
    models.clear_api_key().await
}

#[tauri::command]
pub async fn agent_model_test(models: State<'_, Arc<ModelGateway>>) -> AppResult<()> {
    models.test().await
}

#[tauri::command]
pub async fn agent_model_oauth_start(
    provider: String,
    models: State<'_, Arc<ModelGateway>>,
) -> AppResult<ModelProviderStatus> {
    let provider = match provider.as_str() {
        "chat-gpt" | "chatgpt" => OauthProvider::ChatGpt,
        "open-router" | "openrouter" => OauthProvider::OpenRouter,
        _ => return Err(crate::domain::AppError::ModelInvalid),
    };
    models.start_oauth(provider).await
}

#[tauri::command]
pub async fn agent_model_oauth_cancel(models: State<'_, Arc<ModelGateway>>) -> AppResult<()> {
    models.cancel_oauth().await;
    Ok(())
}

#[tauri::command]
pub async fn agent_model_disconnect(
    models: State<'_, Arc<ModelGateway>>,
) -> AppResult<ModelProviderStatus> {
    models.disconnect().await
}

#[tauri::command]
pub async fn agent_incident_run(
    request: IncidentRequest,
    incidents: State<'_, IncidentService>,
    tools: State<'_, NativeToolExecutionService>,
    cache: State<'_, ObservationCache>,
    sessions: State<'_, ServerSessionManager>,
) -> AppResult<Incident> {
    incidents.run(&sessions, &tools, &cache, request).await
}

#[tauri::command]
pub async fn agent_incident_get(
    incident_id: Uuid,
    incidents: State<'_, IncidentService>,
) -> AppResult<Incident> {
    incidents.get(incident_id).await
}

#[tauri::command]
pub async fn agent_incident_list(
    incidents: State<'_, IncidentService>,
) -> AppResult<Vec<Incident>> {
    incidents.list().await
}

#[tauri::command]
pub async fn agent_incident_attach_changeset(
    incident_id: Uuid,
    link: IncidentChangeSet,
    incidents: State<'_, IncidentService>,
    changes: State<'_, ChangeSetService>,
    fleets: State<'_, FleetExecutionService>,
) -> AppResult<Incident> {
    if link.kind == "single" {
        let change = changes.get(link.id).await?;
        if change.version != link.version || link.exact_target_ids != vec![change.session_id] {
            return Err(crate::domain::AppError::InvalidOperation);
        }
    } else if link.kind == "fleet" {
        let fleet = fleets.get(link.id).await?;
        let mut expected = fleet
            .targets
            .iter()
            .map(|target| target.target_id)
            .collect::<Vec<_>>();
        let mut actual = link.exact_target_ids.clone();
        expected.sort();
        actual.sort();
        if fleet.version != link.version || expected != actual {
            return Err(crate::domain::AppError::InvalidOperation);
        }
    } else {
        return Err(crate::domain::AppError::InvalidOperation);
    }
    incidents.attach_change_set(incident_id, link).await
}

#[tauri::command]
pub async fn agent_incident_refresh(
    incident_id: Uuid,
    incidents: State<'_, IncidentService>,
    changes: State<'_, ChangeSetService>,
    fleets: State<'_, FleetExecutionService>,
) -> AppResult<Incident> {
    incidents.refresh(incident_id, &changes, &fleets).await
}

#[tauri::command]
pub async fn agent_incident_handoff(
    incident_id: Uuid,
    request: IncidentHandoffRequest,
    incidents: State<'_, IncidentService>,
) -> AppResult<Incident> {
    incidents.handoff(incident_id, request).await
}

#[tauri::command]
pub async fn agent_incident_close(
    incident_id: Uuid,
    request: IncidentClosureRequest,
    incidents: State<'_, IncidentService>,
) -> AppResult<Incident> {
    incidents.close(incident_id, request).await
}

#[tauri::command]
pub async fn agent_incident_export(
    incident_id: Uuid,
    incidents: State<'_, IncidentService>,
) -> AppResult<IncidentAuditExport> {
    incidents.export(incident_id).await
}

#[tauri::command]
pub async fn agent_mcp_configure(
    request: McpConfigureRequest,
    mcp: State<'_, McpGateway>,
    credentials: State<'_, CredentialService>,
) -> AppResult<McpServerConfig> {
    mcp.configure(request, &credentials).await
}

#[tauri::command]
pub async fn agent_mcp_list(mcp: State<'_, McpGateway>) -> AppResult<Vec<McpServerConfig>> {
    mcp.list().await
}

#[tauri::command]
pub async fn agent_mcp_set_tool_enabled(
    server_id: Uuid,
    tool_name: String,
    enabled: bool,
    mcp: State<'_, McpGateway>,
) -> AppResult<McpServerConfig> {
    mcp.set_tool_enabled(server_id, &tool_name, enabled).await
}

#[tauri::command]
pub async fn agent_mcp_set_enabled(
    server_id: Uuid,
    enabled: bool,
    mcp: State<'_, McpGateway>,
) -> AppResult<McpServerConfig> {
    mcp.set_enabled(server_id, enabled).await
}

#[tauri::command]
pub async fn agent_mcp_remove(
    server_id: Uuid,
    mcp: State<'_, McpGateway>,
    credentials: State<'_, CredentialService>,
) -> AppResult<()> {
    mcp.remove(server_id, &credentials).await
}

#[tauri::command]
pub async fn agent_policy_effective(
    server_id: Uuid,
    group_id: Option<Uuid>,
    environment: Option<String>,
    policies: State<'_, AgentPolicyService>,
) -> AppResult<EffectiveAgentPolicy> {
    Ok(policies
        .effective_for_server(PolicyTarget {
            server_id,
            group_id,
            environment,
        })
        .await)
}

#[tauri::command]
pub async fn agent_skills_list(skills: State<'_, SkillRegistry>) -> AppResult<Vec<Skill>> {
    skills.list().await
}

#[tauri::command]
pub async fn agent_skills_refresh(
    skills: State<'_, SkillRegistry>,
    tools: State<'_, NativeToolExecutionService>,
) -> AppResult<Vec<Skill>> {
    skills.refresh(&tools).await
}

#[tauri::command]
pub async fn agent_skill_set_enabled(
    skill_id: String,
    enabled: bool,
    skills: State<'_, SkillRegistry>,
) -> AppResult<Skill> {
    skills.set_enabled(&skill_id, enabled).await
}
