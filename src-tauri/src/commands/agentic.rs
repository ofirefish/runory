use tauri::{ipc::Channel, State};
use uuid::Uuid;

use crate::agentic::{
    AgentDoctorRequest, AgentProgress, AgentRun, AgentRuntimeService, ChangeSet,
    ChangeSetDraftRequest, ChangeSetService, ExecutionStrategy, FleetExecutionService, Incident,
    IncidentAuditExport, IncidentChangeSet, IncidentClosureRequest, IncidentHandoffRequest,
    IncidentRequest, IncidentService, ModelConfigureRequest, ModelGateway, ModelProviderStatus,
    MultiChangeSet, MultiChangeSetDraftRequest, MultiServerDoctorRequest, MultiServerRun,
    ObservationCache, PolicyCheckContext,
};
use crate::credentials::CredentialService;
use crate::domain::AppResult;
use crate::mcp::{McpConfigureRequest, McpGateway, McpServerConfig};
use crate::policy::{
    AgentPolicyService, EffectiveAgentPolicy, PolicyEvaluation, PolicyExecutionStrategy,
    PolicyInvocationSource, PolicyMatchedRule, PolicyTarget,
};
use crate::profiles::ProfileService;
use crate::skills::{Skill, SkillRegistry};
use crate::ssh::ServerSessionManager;
use crate::tools::NativeToolExecutionService;

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn agent_doctor_run(
    request: AgentDoctorRequest,
    on_event: Channel<AgentProgress>,
    runtime: State<'_, AgentRuntimeService>,
    tools: State<'_, NativeToolExecutionService>,
    cache: State<'_, ObservationCache>,
    sessions: State<'_, ServerSessionManager>,
    skills: State<'_, SkillRegistry>,
    mcp: State<'_, McpGateway>,
    credentials: State<'_, CredentialService>,
    models: State<'_, ModelGateway>,
    changes: State<'_, ChangeSetService>,
    policies: State<'_, AgentPolicyService>,
    profiles: State<'_, ProfileService>,
) -> AppResult<AgentRun> {
    let target = policy_target(request.session_id, &sessions, &profiles, None).await?;
    runtime
        .run_doctor(
            &sessions,
            &tools,
            &cache,
            &skills,
            &mcp,
            &credentials,
            &models,
            &changes,
            &policies,
            target,
            true,
            Some(&on_event),
            request,
        )
        .await
}

#[tauri::command]
pub async fn agent_model_get(models: State<'_, ModelGateway>) -> AppResult<ModelProviderStatus> {
    Ok(models.status().await)
}

#[tauri::command]
pub async fn agent_model_configure(
    request: ModelConfigureRequest,
    models: State<'_, ModelGateway>,
    credentials: State<'_, CredentialService>,
) -> AppResult<ModelProviderStatus> {
    models.configure(request, &credentials).await
}

#[tauri::command]
pub async fn agent_model_clear_api_key(
    models: State<'_, ModelGateway>,
    credentials: State<'_, CredentialService>,
) -> AppResult<ModelProviderStatus> {
    models.clear_api_key(&credentials).await
}

#[tauri::command]
pub async fn agent_model_test(
    models: State<'_, ModelGateway>,
    credentials: State<'_, CredentialService>,
) -> AppResult<()> {
    models.test(&credentials).await
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
pub async fn agent_run_cancel(
    run_id: Uuid,
    runtime: State<'_, AgentRuntimeService>,
) -> AppResult<()> {
    runtime.cancel(run_id).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn agent_multi_doctor_run(
    request: MultiServerDoctorRequest,
    runtime: State<'_, AgentRuntimeService>,
    tools: State<'_, NativeToolExecutionService>,
    cache: State<'_, ObservationCache>,
    sessions: State<'_, ServerSessionManager>,
    skills: State<'_, SkillRegistry>,
    mcp: State<'_, McpGateway>,
    credentials: State<'_, CredentialService>,
    models: State<'_, ModelGateway>,
    changes: State<'_, ChangeSetService>,
    policies: State<'_, AgentPolicyService>,
    profiles: State<'_, ProfileService>,
) -> AppResult<MultiServerRun> {
    runtime
        .run_multi_doctor(
            &sessions,
            &tools,
            &cache,
            &skills,
            &mcp,
            &credentials,
            &models,
            &changes,
            &policies,
            &profiles,
            request,
        )
        .await
}

#[tauri::command]
pub async fn agent_changeset_draft(
    request: ChangeSetDraftRequest,
    changes: State<'_, ChangeSetService>,
    policies: State<'_, AgentPolicyService>,
    sessions: State<'_, ServerSessionManager>,
    profiles: State<'_, ProfileService>,
) -> AppResult<ChangeSet> {
    let draft = changes.draft(request).await?;
    let target = policy_target(draft.session_id, &sessions, &profiles, None).await?;
    changes
        .check_policy(
            draft.id,
            draft.version,
            &policies,
            PolicyCheckContext {
                target,
                target_count: 1,
                execution_strategy: PolicyExecutionStrategy::Sequential,
                source: PolicyInvocationSource::NativeTool,
            },
        )
        .await
}

#[tauri::command]
pub async fn agent_multi_changeset_draft(
    request: MultiChangeSetDraftRequest,
    fleets: State<'_, FleetExecutionService>,
    changes: State<'_, ChangeSetService>,
    policies: State<'_, AgentPolicyService>,
    sessions: State<'_, ServerSessionManager>,
    profiles: State<'_, ProfileService>,
) -> AppResult<MultiChangeSet> {
    let target_count = request.targets.len();
    let strategy = policy_strategy(request.execution_strategy);
    let environment = request.production.then(|| "production".to_owned());
    let fleet = fleets.draft(&changes, request).await?;
    let mut evaluations = Vec::with_capacity(fleet.targets.len());
    for target in &fleet.targets {
        let checked = changes
            .check_policy(
                target.change_set_id,
                target.change_set_version,
                &policies,
                PolicyCheckContext {
                    target: policy_target(
                        target.target_id,
                        &sessions,
                        &profiles,
                        environment.clone(),
                    )
                    .await?,
                    target_count,
                    execution_strategy: strategy,
                    source: PolicyInvocationSource::MultiServerChangeSet,
                },
            )
            .await?;
        if let Some(evaluation) = checked.policy_evaluation {
            evaluations.push(evaluation);
        }
    }
    fleets
        .bind_policy(fleet.id, combine_policy_evaluations(evaluations)?)
        .await
}

#[tauri::command]
pub async fn agent_fleet_changeset_get(
    fleet_id: Uuid,
    fleets: State<'_, FleetExecutionService>,
) -> AppResult<MultiChangeSet> {
    fleets.get(fleet_id).await
}

#[tauri::command]
pub async fn agent_fleet_changeset_list(
    fleets: State<'_, FleetExecutionService>,
) -> AppResult<Vec<MultiChangeSet>> {
    fleets.list().await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn agent_fleet_changeset_approve(
    fleet_id: Uuid,
    version: u64,
    exact_target_ids: Vec<Uuid>,
    fleets: State<'_, FleetExecutionService>,
    changes: State<'_, ChangeSetService>,
    tools: State<'_, NativeToolExecutionService>,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, AgentPolicyService>,
    profiles: State<'_, ProfileService>,
) -> AppResult<MultiChangeSet> {
    let fleet = fleets.get(fleet_id).await?;
    let mut evaluations = Vec::with_capacity(fleet.targets.len());
    for target in &fleet.targets {
        let checked = changes
            .check_policy(
                target.change_set_id,
                target.change_set_version,
                &policies,
                PolicyCheckContext {
                    target: policy_target(
                        target.target_id,
                        &sessions,
                        &profiles,
                        fleet.production.then(|| "production".to_owned()),
                    )
                    .await?,
                    target_count: fleet.targets.len(),
                    execution_strategy: policy_strategy(fleet.execution_strategy),
                    source: PolicyInvocationSource::MultiServerChangeSet,
                },
            )
            .await?;
        if !checked
            .policy_evaluation
            .as_ref()
            .is_some_and(PolicyEvaluation::permits_execution)
        {
            return Err(crate::domain::AppError::InvalidOperation);
        }
        if let Some(evaluation) = checked.policy_evaluation {
            evaluations.push(evaluation);
        }
        changes
            .approve_with_preconditions(
                target.change_set_id,
                target.change_set_version,
                &sessions,
                &tools,
            )
            .await?;
    }
    fleets
        .bind_policy(fleet_id, combine_policy_evaluations(evaluations)?)
        .await?;
    fleets
        .approve(&changes, fleet_id, version, exact_target_ids)
        .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn agent_fleet_changeset_execute(
    fleet_id: Uuid,
    version: u64,
    fleets: State<'_, FleetExecutionService>,
    changes: State<'_, ChangeSetService>,
    tools: State<'_, NativeToolExecutionService>,
    sessions: State<'_, ServerSessionManager>,
    cache: State<'_, ObservationCache>,
    policies: State<'_, AgentPolicyService>,
) -> AppResult<MultiChangeSet> {
    let fleet = fleets.get(fleet_id).await?;
    let snapshot = fleet
        .policy_snapshot
        .as_ref()
        .ok_or(crate::domain::AppError::InvalidOperation)?;
    if !policies.current_matches(snapshot).await {
        fleets.invalidate_policy_change(fleet_id).await?;
        return Err(crate::domain::AppError::InvalidOperation);
    }
    let target_ids = fleet
        .targets
        .into_iter()
        .map(|target| target.target_id)
        .collect::<Vec<_>>();
    let result = fleets
        .execute(&changes, &sessions, &tools, &policies, fleet_id, version)
        .await;
    for target_id in target_ids {
        cache.invalidate_target(target_id).await;
    }
    result
}

#[tauri::command]
pub async fn agent_changeset_get(
    change_set_id: Uuid,
    changes: State<'_, ChangeSetService>,
) -> AppResult<ChangeSet> {
    changes.get(change_set_id).await
}

#[tauri::command]
pub async fn agent_changeset_list(
    changes: State<'_, ChangeSetService>,
) -> AppResult<Vec<ChangeSet>> {
    changes.list().await
}

#[tauri::command]
pub async fn agent_changeset_revise(
    change_set_id: Uuid,
    request: ChangeSetDraftRequest,
    changes: State<'_, ChangeSetService>,
    fleets: State<'_, FleetExecutionService>,
) -> AppResult<ChangeSet> {
    let revised = changes.revise(change_set_id, request).await?;
    fleets
        .invalidate_target(change_set_id, revised.session_id, revised.version)
        .await?;
    Ok(revised)
}

#[tauri::command]
pub async fn agent_changeset_approve(
    change_set_id: Uuid,
    version: u64,
    changes: State<'_, ChangeSetService>,
    tools: State<'_, NativeToolExecutionService>,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, AgentPolicyService>,
    profiles: State<'_, ProfileService>,
) -> AppResult<ChangeSet> {
    let current = changes.get(change_set_id).await?;
    let checked = changes
        .check_policy(
            change_set_id,
            version,
            &policies,
            PolicyCheckContext {
                target: policy_target(current.session_id, &sessions, &profiles, None).await?,
                target_count: 1,
                execution_strategy: PolicyExecutionStrategy::Sequential,
                source: PolicyInvocationSource::NativeTool,
            },
        )
        .await?;
    if !checked
        .policy_evaluation
        .as_ref()
        .is_some_and(PolicyEvaluation::permits_execution)
    {
        return Err(crate::domain::AppError::InvalidOperation);
    }
    changes
        .approve_with_preconditions(change_set_id, version, &sessions, &tools)
        .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn agent_changeset_approve_step(
    change_set_id: Uuid,
    version: u64,
    step_id: Uuid,
    changes: State<'_, ChangeSetService>,
    tools: State<'_, NativeToolExecutionService>,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, AgentPolicyService>,
    profiles: State<'_, ProfileService>,
) -> AppResult<ChangeSet> {
    let current = changes.get(change_set_id).await?;
    let checked = changes
        .check_policy(
            change_set_id,
            version,
            &policies,
            PolicyCheckContext {
                target: policy_target(current.session_id, &sessions, &profiles, None).await?,
                target_count: 1,
                execution_strategy: PolicyExecutionStrategy::Sequential,
                source: PolicyInvocationSource::NativeTool,
            },
        )
        .await?;
    if checked
        .policy_evaluation
        .as_ref()
        .map(|evaluation| evaluation.decision)
        != Some(crate::policy::PolicyDecision::RequireStepApproval)
    {
        return Err(crate::domain::AppError::InvalidOperation);
    }
    if checked.preconditions.is_empty() {
        changes
            .capture_preconditions(change_set_id, version, &sessions, &tools)
            .await?;
    }
    changes.approve_step(change_set_id, version, step_id).await
}

#[tauri::command]
pub async fn agent_changeset_reject(
    change_set_id: Uuid,
    version: u64,
    changes: State<'_, ChangeSetService>,
) -> AppResult<ChangeSet> {
    changes.reject(change_set_id, version).await
}

#[tauri::command]
pub async fn agent_changeset_execute(
    change_set_id: Uuid,
    version: u64,
    changes: State<'_, ChangeSetService>,
    tools: State<'_, NativeToolExecutionService>,
    sessions: State<'_, ServerSessionManager>,
    cache: State<'_, ObservationCache>,
    policies: State<'_, AgentPolicyService>,
) -> AppResult<ChangeSet> {
    let target_id = changes.get(change_set_id).await?.session_id;
    let result = changes
        .execute_with_policy(change_set_id, version, &sessions, &tools, &policies)
        .await;
    cache.invalidate_target(target_id).await;
    result
}

#[tauri::command]
pub async fn agent_changeset_rollback(
    change_set_id: Uuid,
    version: u64,
    changes: State<'_, ChangeSetService>,
    tools: State<'_, NativeToolExecutionService>,
    sessions: State<'_, ServerSessionManager>,
    cache: State<'_, ObservationCache>,
) -> AppResult<ChangeSet> {
    let target_id = changes.get(change_set_id).await?.session_id;
    let result = changes
        .rollback(change_set_id, version, &sessions, &tools)
        .await;
    cache.invalidate_target(target_id).await;
    result
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

fn policy_strategy(strategy: ExecutionStrategy) -> PolicyExecutionStrategy {
    match strategy {
        ExecutionStrategy::Sequential => PolicyExecutionStrategy::Sequential,
        ExecutionStrategy::Parallel => PolicyExecutionStrategy::Parallel,
        ExecutionStrategy::Canary => PolicyExecutionStrategy::Canary,
        ExecutionStrategy::RollingBatch => PolicyExecutionStrategy::Rolling,
    }
}

fn combine_policy_evaluations(evaluations: Vec<PolicyEvaluation>) -> AppResult<PolicyEvaluation> {
    let first = evaluations
        .first()
        .ok_or(crate::domain::AppError::InvalidOperation)?;
    if evaluations.iter().any(|evaluation| {
        evaluation.policy_version != first.policy_version
            || evaluation.policy_hash != first.policy_hash
    }) {
        return Err(crate::domain::AppError::InvalidOperation);
    }
    let decisive = evaluations
        .iter()
        .max_by_key(|evaluation| evaluation.decision.precedence())
        .ok_or(crate::domain::AppError::InvalidOperation)?;
    let mut matched_rules = evaluations
        .iter()
        .flat_map(|evaluation| evaluation.matched_rules.clone())
        .collect::<Vec<PolicyMatchedRule>>();
    matched_rules.sort_by(|left, right| left.rule_id.cmp(&right.rule_id));
    matched_rules.dedup_by(|left, right| left.rule_id == right.rule_id);
    Ok(PolicyEvaluation {
        decision: decisive.decision,
        matched_rules,
        reason: decisive.reason.clone(),
        scope: decisive.scope.clone(),
        policy_version: first.policy_version,
        policy_hash: first.policy_hash.clone(),
    })
}

async fn policy_target(
    session_id: Uuid,
    sessions: &ServerSessionManager,
    profiles: &ProfileService,
    environment: Option<String>,
) -> AppResult<PolicyTarget> {
    let server_id = sessions.profile_id(session_id).await?;
    let profile = profiles.get(server_id).await?;
    Ok(PolicyTarget {
        server_id,
        group_id: profile.group_id,
        environment,
    })
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
