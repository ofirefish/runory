use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::future::join_all;
use tauri::ipc::Channel;
use tokio::sync::{watch, Mutex};
use uuid::Uuid;

use super::changes::{ChangeSetDraftRequest, ChangeSetService, PolicyCheckContext};
use super::context::{
    compact, skill_context, snapshot, user_context, ContextFact, ContextSource, ContextTrust,
};
use super::model_gateway::ModelGateway;
use super::optimization::{cache_key, project_result, ObservationCache, ToolResultProjection};
use super::planning::{
    change_proposal_is_evidence_bound, evidence_answer, invocations, AgentDecision, ModelToolCall,
    PlanningHints,
};
use super::state::{
    validate_request, AgentBudget, AgentDoctorRequest, AgentProgress, AgentProgressStage, AgentRun,
    AgentRunMetrics, AgentRunState, AgentToolActivity, Evidence, ExternalEvidence,
};
use crate::credentials::CredentialService;
use crate::domain::{AppError, AppResult};
use crate::mcp::McpGateway;
use crate::policy::{
    AgentPolicyService, PolicyExecutionStrategy, PolicyInvocationSource, PolicyTarget,
};
use crate::skills::SkillRegistry;
use crate::ssh::ServerSessionManager;
use crate::tools::{
    NativeToolExecutionService, NativeToolInvocation, NativeToolRequest, RiskLevel, ToolResult,
};

const MCP_TIMEOUT_MS: u64 = 20_000;
const MODEL_TIMEOUT_MS: u64 = 30_000;

pub(crate) struct AgentRuntimeService {
    cancellations: Mutex<HashMap<Uuid, watch::Sender<bool>>>,
}

impl Default for AgentRuntimeService {
    fn default() -> Self {
        Self {
            cancellations: Mutex::new(HashMap::new()),
        }
    }
}

impl AgentRuntimeService {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_doctor(
        &self,
        sessions: &ServerSessionManager,
        tools: &NativeToolExecutionService,
        cache: &ObservationCache,
        skills: &SkillRegistry,
        mcp: &McpGateway,
        credentials: &CredentialService,
        models: &ModelGateway,
        changes: &ChangeSetService,
        policies: &AgentPolicyService,
        policy_target: PolicyTarget,
        allow_change_proposal: bool,
        progress: Option<&Channel<AgentProgress>>,
        request: AgentDoctorRequest,
    ) -> AppResult<AgentRun> {
        if !validate_request(&request) {
            return Err(AppError::InvalidOperation);
        }
        let budget = request.budget.unwrap_or_default();
        let model_id = models.id().await;
        let started_at_epoch_ms = now_ms();
        let (sender, receiver) = watch::channel(false);
        if self
            .cancellations
            .lock()
            .await
            .insert(request.run_id, sender)
            .is_some()
        {
            return Err(AppError::InvalidOperation);
        }
        let future = self.run_with_budget(
            sessions,
            tools,
            cache,
            skills,
            mcp,
            credentials,
            models,
            changes,
            policies,
            policy_target,
            allow_change_proposal,
            progress,
            request.clone(),
            budget,
            started_at_epoch_ms,
            receiver,
        );
        let outcome =
            tokio::time::timeout(Duration::from_millis(budget.time_budget_ms), future).await;
        self.cancellations.lock().await.remove(&request.run_id);
        match outcome {
            Ok(result) => result,
            Err(_) => Ok(empty_run(
                model_id,
                &request,
                budget,
                started_at_epoch_ms,
                AgentRunState::TimedOut,
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_with_budget(
        &self,
        sessions: &ServerSessionManager,
        tools: &NativeToolExecutionService,
        cache: &ObservationCache,
        skills: &SkillRegistry,
        mcp: &McpGateway,
        credentials: &CredentialService,
        models: &ModelGateway,
        changes: &ChangeSetService,
        policies: &AgentPolicyService,
        policy_target: PolicyTarget,
        allow_change_proposal: bool,
        progress: Option<&Channel<AgentProgress>>,
        request: AgentDoctorRequest,
        budget: AgentBudget,
        started_at_epoch_ms: u64,
        receiver: watch::Receiver<bool>,
    ) -> AppResult<AgentRun> {
        let mut context = vec![user_context(&request.user_request, started_at_epoch_ms)];
        send_progress(progress, AgentProgressStage::GatheringContext, Vec::new());
        if let Some(skill_id) = &request.skill_id {
            let instructions = skills.enabled_instructions(skill_id).await?;
            context.push(skill_context(skill_id, &instructions, now_ms()));
        }
        let context_snapshot = snapshot(context.clone(), budget.context, now_ms());
        let mut run = AgentRun {
            id: request.run_id,
            model: models.id().await,
            session_id: request.session_id,
            state: AgentRunState::GatheringContext,
            context,
            context_snapshot,
            activities: Vec::new(),
            evidence: Vec::new(),
            external_evidence: Vec::new(),
            diagnosis: None,
            answer: None,
            answer_evidence_ids: Vec::new(),
            goal_achieved: false,
            clarification_question: None,
            failure_code: None,
            change_set: None,
            max_tool_calls: budget.max_tool_calls,
            used_tool_calls: 0,
            max_model_tokens: budget
                .max_input_tokens
                .saturating_add(budget.max_output_tokens),
            used_model_tokens: 0,
            timeout_ms: budget.time_budget_ms,
            started_at_epoch_ms,
            completed_at_epoch_ms: 0,
            metrics: AgentRunMetrics::new(request.run_id, request.incident_id),
        };
        if let Some(external) = &request.mcp_context {
            run.metrics.mcp_calls += 1;
            let data = tokio::time::timeout(
                Duration::from_millis(MCP_TIMEOUT_MS),
                mcp.read_context(
                    external.server_id,
                    &external.tool_name,
                    external.arguments.clone(),
                    credentials,
                ),
            )
            .await
            .map_err(|_| AppError::InvalidOperation)??;
            run.external_evidence.push(ExternalEvidence {
                id: Uuid::new_v4(),
                source: format!("mcp:{}:{}", external.server_id, external.tool_name),
                trust: ContextTrust::UntrustedExternalData,
                data,
            });
        }

        let available_tools = tools.descriptors();
        let planning_hints = PlanningHints {
            service: request.service.clone(),
            http_url: request.http_url.clone(),
            include_nginx_test: request.include_nginx_test,
        };
        let mut observed_calls = HashSet::new();
        let mut executed_tool_calls: Vec<ModelToolCall> = Vec::new();
        run.state = AgentRunState::Investigating;
        for _ in 0..budget.max_model_calls {
            if *receiver.borrow() {
                run.state = AgentRunState::Cancelled;
                break;
            }
            if run.context_snapshot.estimated_tokens > models.max_context_tokens().await {
                run.state = AgentRunState::BudgetExceeded;
                break;
            }
            run.state = AgentRunState::Diagnosing;
            send_progress(progress, AgentProgressStage::Planning, Vec::new());
            let turn = match tokio::time::timeout(
                Duration::from_millis(MODEL_TIMEOUT_MS),
                models.decide(
                    &request.user_request,
                    &run.evidence,
                    &planning_hints,
                    &executed_tool_calls,
                    &available_tools,
                    credentials,
                ),
            )
            .await
            {
                Ok(Ok(turn)) => turn,
                Ok(Err(error)) => {
                    run.failure_code = Some(error.code());
                    run.state = AgentRunState::Failed;
                    break;
                }
                Err(_) => {
                    run.failure_code = Some("MODEL_TIMEOUT");
                    run.state = AgentRunState::TimedOut;
                    break;
                }
            };
            run.metrics.model_calls = run.metrics.model_calls.saturating_add(1);
            run.metrics.input_tokens = run.metrics.input_tokens.saturating_add(turn.input_tokens);
            run.metrics.output_tokens =
                run.metrics.output_tokens.saturating_add(turn.output_tokens);
            run.metrics.estimated_cost_microusd = match (
                run.metrics.estimated_cost_microusd,
                turn.estimated_cost_microusd,
            ) {
                (Some(total), Some(turn)) => Some(total.saturating_add(turn)),
                (None, value) => value,
                (value, None) => value,
            };
            run.used_model_tokens = run
                .metrics
                .input_tokens
                .saturating_add(run.metrics.output_tokens);
            let cost_exceeded = budget.max_cost_microusd.is_some_and(|maximum| {
                run.metrics
                    .estimated_cost_microusd
                    .is_some_and(|cost| cost > maximum)
            });
            if run.metrics.input_tokens > budget.max_input_tokens
                || run.metrics.output_tokens > budget.max_output_tokens
                || cost_exceeded
            {
                run.state = AgentRunState::BudgetExceeded;
                break;
            }
            match turn.decision {
                AgentDecision::ToolCalls(calls) => {
                    let proposed = match invocations(calls.clone()) {
                        Ok(invocations) => invocations,
                        Err(_) => {
                            run.failure_code = Some("MODEL_RESPONSE_INVALID");
                            run.state = AgentRunState::PolicyBlocked;
                            break;
                        }
                    };
                    let remaining = budget
                        .max_tool_calls
                        .saturating_sub(observed_calls.len().min(u32::MAX as usize) as u32)
                        as usize;
                    if proposed.len() > remaining {
                        run.failure_code = Some("BUDGET_EXCEEDED");
                        run.state = AgentRunState::BudgetExceeded;
                        break;
                    }
                    let mut next = Vec::new();
                    for (call, invocation) in calls.into_iter().zip(proposed) {
                        let Some(key) = cache_key(request.session_id, &invocation) else {
                            run.failure_code = Some("POLICY_BLOCKED");
                            run.state = AgentRunState::PolicyBlocked;
                            break;
                        };
                        if observed_calls.insert(key) {
                            next.push(invocation);
                            executed_tool_calls.push(call);
                        } else {
                            run.metrics.duplicate_calls =
                                run.metrics.duplicate_calls.saturating_add(1);
                        }
                    }
                    if next.is_empty() {
                        let AgentDecision::Answer {
                            text,
                            evidence_ids,
                            goal_achieved,
                        } = evidence_answer(&request.user_request, &run.evidence)
                        else {
                            unreachable!("evidence_answer always returns an answer")
                        };
                        run.answer = Some(text);
                        run.answer_evidence_ids = evidence_ids;
                        run.goal_achieved = goal_achieved;
                        run.failure_code = None;
                        run.state = AgentRunState::Succeeded;
                        run.metrics.diagnosis_latency_ms =
                            Some(now_ms().saturating_sub(started_at_epoch_ms));
                        break;
                    }
                    run.state = AgentRunState::Investigating;
                    send_progress(
                        progress,
                        AgentProgressStage::RunningTools,
                        next.iter().map(NativeToolInvocation::name).collect(),
                    );
                    let results = execute_reads(
                        sessions,
                        tools,
                        cache,
                        request.run_id,
                        request.session_id,
                        receiver.clone(),
                        next,
                        &mut run.metrics,
                    )
                    .await?;
                    for result in results {
                        record_result(&mut run, result);
                    }
                    run.used_tool_calls = observed_calls.len().min(u32::MAX as usize) as u32;
                    if run.evidence.iter().any(|item| item.result.cancelled) {
                        run.state = AgentRunState::Cancelled;
                        break;
                    }
                }
                AgentDecision::Answer {
                    text,
                    evidence_ids,
                    goal_achieved,
                } => {
                    run.answer = Some(text);
                    run.answer_evidence_ids = evidence_ids;
                    run.goal_achieved = goal_achieved;
                    run.state = AgentRunState::Succeeded;
                    run.metrics.diagnosis_latency_ms =
                        Some(now_ms().saturating_sub(started_at_epoch_ms));
                    break;
                }
                AgentDecision::Clarify(question) => {
                    run.clarification_question = Some(question);
                    run.state = AgentRunState::NeedsInput;
                    break;
                }
                AgentDecision::ProposeChange {
                    title,
                    summary,
                    evidence_ids,
                    steps,
                } => {
                    if !allow_change_proposal {
                        run.failure_code = Some("FLEET_CHANGESET_REQUIRED");
                        run.state = AgentRunState::PolicyBlocked;
                        break;
                    }
                    send_progress(progress, AgentProgressStage::DraftingChangeSet, Vec::new());
                    if !change_proposal_is_evidence_bound(
                        &request.user_request,
                        &run.evidence,
                        &evidence_ids,
                        &steps,
                    ) {
                        run.failure_code = Some("CHANGE_PROPOSAL_NOT_EVIDENCE_BOUND");
                        run.state = AgentRunState::PolicyBlocked;
                        break;
                    }
                    let draft = match changes
                        .draft(ChangeSetDraftRequest {
                            agent_run_id: request.run_id,
                            session_id: request.session_id,
                            title,
                            steps,
                        })
                        .await
                    {
                        Ok(draft) => draft,
                        Err(error) => {
                            run.failure_code = Some(error.code());
                            run.state = AgentRunState::Failed;
                            break;
                        }
                    };
                    let checked = match changes
                        .check_policy(
                            draft.id,
                            draft.version,
                            policies,
                            PolicyCheckContext {
                                target: policy_target.clone(),
                                target_count: 1,
                                execution_strategy: PolicyExecutionStrategy::Sequential,
                                source: PolicyInvocationSource::AgentRuntime,
                            },
                        )
                        .await
                    {
                        Ok(checked) => checked,
                        Err(error) => {
                            run.failure_code = Some(error.code());
                            run.state = AgentRunState::Failed;
                            break;
                        }
                    };
                    run.answer = Some(summary);
                    run.answer_evidence_ids = evidence_ids;
                    run.goal_achieved = false;
                    run.change_set = Some(checked);
                    run.state = AgentRunState::Succeeded;
                    break;
                }
            }
        }
        if matches!(
            run.state,
            AgentRunState::Investigating | AgentRunState::Diagnosing
        ) {
            run.failure_code = Some("BUDGET_EXCEEDED");
            run.state = AgentRunState::BudgetExceeded;
        }
        compact_context(&mut run, request.session_id);
        finish_metrics(&mut run);
        send_progress(progress, AgentProgressStage::Complete, Vec::new());
        Ok(run)
    }

    pub(crate) async fn cancel(&self, run_id: Uuid) -> AppResult<()> {
        let cancellations = self.cancellations.lock().await;
        cancellations
            .get(&run_id)
            .ok_or(AppError::InvalidOperation)?
            .send(true)
            .map_err(|_| AppError::InvalidOperation)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_reads(
    sessions: &ServerSessionManager,
    tools: &NativeToolExecutionService,
    cache: &ObservationCache,
    run_id: Uuid,
    session_id: Uuid,
    receiver: watch::Receiver<bool>,
    invocations: Vec<NativeToolInvocation>,
    metrics: &mut AgentRunMetrics,
) -> AppResult<Vec<ToolResult>> {
    let mut unique = HashSet::new();
    let mut groups: BTreeMap<RiskLevel, Vec<NativeToolInvocation>> = BTreeMap::new();
    let mut results = Vec::new();
    for invocation in invocations {
        let Some(key) = cache_key(session_id, &invocation) else {
            continue;
        };
        if !unique.insert(key) {
            metrics.duplicate_calls += 1;
            continue;
        }
        if let Some(cached) = cache.get(session_id, &invocation, now_ms()).await {
            metrics.cache_hits += 1;
            results.push(cached);
            continue;
        }
        let risk = tools
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.name == invocation.name())
            .map_or(RiskLevel::R1, |descriptor| descriptor.risk_level);
        groups.entry(risk).or_default().push(invocation);
    }
    for (_, group) in groups {
        if *receiver.borrow() {
            break;
        }
        metrics.parallel_read_batches += u32::from(group.len() > 1);
        let calls = group.into_iter().map(|invocation| async {
            let result = tools
                .execute(
                    sessions,
                    NativeToolRequest::with_cancellation_for_agent(
                        run_id,
                        session_id,
                        invocation.clone(),
                        receiver.clone(),
                    ),
                )
                .await;
            (invocation, result)
        });
        for (invocation, result) in join_all(calls).await {
            let result = result?;
            metrics.tool_calls += 1;
            cache
                .put(session_id, &invocation, result.started_at_epoch_ms, &result)
                .await;
            results.push(result);
        }
    }
    Ok(results)
}

fn record_result(run: &mut AgentRun, result: ToolResult) {
    run.activities.push(AgentToolActivity {
        invocation_id: result.invocation_id,
        tool_name: result.tool_name,
        success: result.success,
        error_code: result.error_code,
        duration_ms: result.duration_ms,
    });
    run.evidence.push(Evidence {
        id: Uuid::new_v4(),
        source: format!("tool.{}", result.tool_name.as_str()),
        invocation_id: result.invocation_id,
        trust: ContextTrust::UntrustedRemoteData,
        summary: result.summary,
        result,
    });
}

fn compact_context(run: &mut AgentRun, target_id: Uuid) {
    let projection = ToolResultProjection::default();
    let facts = run
        .evidence
        .iter()
        .map(|item| {
            let projected = project_result(&item.result, &projection);
            ContextFact {
                source: ContextSource::NativeTools,
                target_id: Some(target_id),
                evidence_id: Some(item.id),
                key: item.source.clone(),
                value: format!(
                    "summary={};success={};projectedBytes={}",
                    item.summary, item.result.success, projected.projected_bytes
                ),
            }
        })
        .collect();
    compact(&mut run.context_snapshot, facts);
}

fn finish_metrics(run: &mut AgentRun) {
    run.completed_at_epoch_ms = now_ms();
    run.metrics.context_size_bytes = run.context_snapshot.total_bytes;
    run.metrics.compaction_count = run.context_snapshot.compaction_count;
    run.metrics.duration_ms = run
        .completed_at_epoch_ms
        .saturating_sub(run.started_at_epoch_ms);
}

fn empty_run(
    model: String,
    request: &AgentDoctorRequest,
    budget: AgentBudget,
    started_at_epoch_ms: u64,
    state: AgentRunState,
) -> AgentRun {
    let context_snapshot = snapshot(Vec::new(), budget.context, now_ms());
    let mut metrics = AgentRunMetrics::new(request.run_id, request.incident_id);
    metrics.duration_ms = now_ms().saturating_sub(started_at_epoch_ms);
    AgentRun {
        id: request.run_id,
        model,
        session_id: request.session_id,
        state,
        context: Vec::new(),
        context_snapshot,
        activities: Vec::new(),
        evidence: Vec::new(),
        external_evidence: Vec::new(),
        diagnosis: None,
        answer: None,
        answer_evidence_ids: Vec::new(),
        goal_achieved: false,
        clarification_question: None,
        failure_code: match state {
            AgentRunState::TimedOut => Some("AGENT_TIMEOUT"),
            AgentRunState::BudgetExceeded => Some("BUDGET_EXCEEDED"),
            _ => None,
        },
        change_set: None,
        max_tool_calls: budget.max_tool_calls,
        used_tool_calls: 0,
        max_model_tokens: budget
            .max_input_tokens
            .saturating_add(budget.max_output_tokens),
        used_model_tokens: 0,
        timeout_ms: budget.time_budget_ms,
        started_at_epoch_ms,
        completed_at_epoch_ms: now_ms(),
        metrics,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn send_progress(
    channel: Option<&Channel<AgentProgress>>,
    stage: AgentProgressStage,
    tool_names: Vec<crate::tools::NativeToolName>,
) {
    if let Some(channel) = channel {
        let _ = channel.send(AgentProgress { stage, tool_names });
    }
}
