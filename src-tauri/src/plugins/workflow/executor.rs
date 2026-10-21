use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::actions::{ActionContext, ActionRegistry, ActionResult};
use super::ast::{FailurePolicy, WorkflowDefinition};
use super::context::WorkflowContext;
use super::expressions::{evaluate_condition, resolve_json_value};
use super::parser::parse_workflow_yaml;
use super::planner::{WorkflowPlan, WorkflowPlanner};
use super::state::{WorkflowRunRecord, WorkflowRunState, WorkflowStepRecord, WorkflowStepState};
use super::validator::validate_workflow;
use crate::agentic::ChangeSetService;
use crate::domain::SessionId;
use crate::plugins::api::{WorkflowPlanRequest, WorkflowRunDto, WorkflowStepStatusDto};
use crate::plugins::audit::integration::{log_plugin_audit, log_plugin_audit_with_host};
use crate::plugins::changeset::{
    draft_plugin_changeset, enforce_changeset_mappability, plugin_plan_to_changeset_summary,
};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::registry::PluginRegistryService;
use crate::plugins::secrets::{MapSecretResolver, SharedSecretResolver};
use crate::policy::AgentPolicyService;
use crate::ssh::ServerSessionManager;
use crate::tools::NativeToolExecutionService;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum WorkflowEvent {
    Started {
        run_id: String,
        workflow_id: String,
    },
    StepStarted {
        run_id: String,
        step_id: String,
    },
    StepOutput {
        run_id: String,
        step_id: String,
        stdout_preview: Option<String>,
    },
    StepCompleted {
        run_id: String,
        step_id: String,
        state: WorkflowStepState,
    },
    Completed {
        run_id: String,
        state: WorkflowRunState,
    },
}

struct StoredPlan {
    plan: WorkflowPlan,
    definition: WorkflowDefinition,
    request: WorkflowPlanRequest,
    plugin_root: PathBuf,
    plugin_version: String,
    /// Step ids executed via ChangeSetService (skip in plugin action loop).
    changeset_covered_steps: Vec<String>,
    bound_change_set: Option<(Uuid, u64)>,
}

/// High-level workflow service used by Tauri commands.
pub struct WorkflowExecutor {
    registry: Arc<PluginRegistryService>,
    sessions: Arc<ServerSessionManager>,
    actions: Arc<ActionRegistry>,
    change_sets: Option<ChangeSetService>,
    policies: Option<AgentPolicyService>,
    /// Process-memory secret bindings (bind/unbind IPC). Also used as resolver.
    secret_store: Option<Arc<MapSecretResolver>>,
    plans: Mutex<HashMap<String, StoredPlan>>,
    run_tokens: Mutex<HashMap<String, CancellationToken>>,
    /// Events retained for polling when AppHandle emit is unavailable.
    polled_events: Mutex<Vec<WorkflowEvent>>,
}

impl WorkflowExecutor {
    pub fn new(registry: Arc<PluginRegistryService>, sessions: Arc<ServerSessionManager>) -> Self {
        Self::with_services(registry, sessions, None, None)
    }

    pub fn with_services(
        registry: Arc<PluginRegistryService>,
        sessions: Arc<ServerSessionManager>,
        change_sets: Option<ChangeSetService>,
        policies: Option<AgentPolicyService>,
    ) -> Self {
        Self::with_actions(
            registry,
            sessions,
            Arc::new(ActionRegistry::with_builtins()),
            change_sets,
            policies,
        )
    }

    pub fn with_actions(
        registry: Arc<PluginRegistryService>,
        sessions: Arc<ServerSessionManager>,
        actions: Arc<ActionRegistry>,
        change_sets: Option<ChangeSetService>,
        policies: Option<AgentPolicyService>,
    ) -> Self {
        Self::with_actions_and_secrets(
            registry,
            sessions,
            actions,
            change_sets,
            policies,
            Some(Arc::new(MapSecretResolver::new())),
        )
    }

    pub fn with_actions_and_secrets(
        registry: Arc<PluginRegistryService>,
        sessions: Arc<ServerSessionManager>,
        actions: Arc<ActionRegistry>,
        change_sets: Option<ChangeSetService>,
        policies: Option<AgentPolicyService>,
        secret_store: Option<Arc<MapSecretResolver>>,
    ) -> Self {
        Self {
            registry,
            sessions,
            actions,
            change_sets,
            policies,
            secret_store,
            plans: Mutex::new(HashMap::new()),
            run_tokens: Mutex::new(HashMap::new()),
            polled_events: Mutex::new(Vec::new()),
        }
    }

    /// Process-memory secret binding store for Core IPC.
    pub fn secret_store(&self) -> Option<&Arc<MapSecretResolver>> {
        self.secret_store.as_ref()
    }

    fn shared_secret_resolver(&self) -> Option<SharedSecretResolver> {
        self.secret_store
            .as_ref()
            .map(|store| store.clone() as SharedSecretResolver)
    }

    pub async fn get_form(
        &self,
        plugin_id: &str,
        workflow_id: &str,
    ) -> PluginResult<serde_json::Value> {
        let (record, manifest) = self.registry.require_enabled(plugin_id)?;
        let contrib = manifest
            .contributes
            .workflows
            .iter()
            .find(|w| w.id == workflow_id)
            .ok_or_else(|| PluginError::WorkflowNotFound(workflow_id.to_owned()))?;
        let definition = load_workflow_file(Path::new(&record.install_path), &contrib.file)?;
        let input_schema = if let Some(schema_path) = &contrib.input_schema {
            let bytes = std::fs::read(Path::new(&record.install_path).join(schema_path))
                .map_err(|_| PluginError::WorkflowNotFound(schema_path.clone()))?;
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
        } else {
            serde_json::Value::Null
        };
        Ok(serde_json::json!({
            "pluginId": plugin_id,
            "workflowId": workflow_id,
            "name": definition.metadata.name,
            "description": definition.metadata.description,
            "inputSchema": input_schema,
        }))
    }

    pub async fn plan(&self, mut request: WorkflowPlanRequest) -> PluginResult<WorkflowPlan> {
        if let Some(session_id) = parse_session_id(request.session_id.as_deref())? {
            let profile = self
                .sessions
                .profile_id(session_id)
                .await
                .map_err(|_| PluginError::PolicyDenied)?
                .to_string();
            if request
                .host_id
                .as_ref()
                .is_some_and(|host| host != &profile)
            {
                return Err(PluginError::PolicyDenied);
            }
            request.host_id = Some(profile);
        }
        let (record, manifest) = self.registry.require_enabled(&request.plugin_id)?;
        self.registry
            .permissions()
            .load_plugin(&request.plugin_id)
            .await?;
        let contrib = manifest
            .contributes
            .workflows
            .iter()
            .find(|w| w.id == request.workflow_id)
            .ok_or_else(|| PluginError::WorkflowNotFound(request.workflow_id.clone()))?;
        let definition = load_workflow_file(Path::new(&record.install_path), &contrib.file)?;
        validate_workflow(&definition)?;

        let session_id = parse_session_id(request.session_id.as_deref())?;
        let action_ctx = ActionContext {
            run_id: String::new(),
            plugin_id: record.id.clone(),
            plugin_version: record.version.clone(),
            workflow_id: request.workflow_id.clone(),
            host_id: request.host_id.clone(),
            host_os: None,
            host_arch: None,
            host_distribution: None,
            host_version: None,
            session_id,
            sessions: Some(Arc::clone(&self.sessions)),
            permission_service: self.registry.permissions(),
            plugin_root: Some(PathBuf::from(&record.install_path)),
            secret_resolver: self.shared_secret_resolver(),
            app_handle: None,
            cancellation: CancellationToken::new(),
        };

        let planner = WorkflowPlanner::new(Arc::clone(&self.actions));
        let mut plan = planner
            .plan(&definition, &action_ctx, &request.inputs)
            .await?;

        // Fail-closed ToolPolicy + ChangeSet mappability for mutative steps.
        let mapping = enforce_changeset_mappability(&plan)?;
        let mut summary = plugin_plan_to_changeset_summary(&plan);
        let mut bound_change_set = None;
        let mut covered = mapping.covered_step_ids.clone();

        if !mapping.drafts.is_empty() {
            match (
                &self.change_sets,
                &self.policies,
                parse_session_id(request.session_id.as_deref())?,
            ) {
                (Some(changes), Some(policies), Some(session_uuid)) => {
                    let server_id = request
                        .host_id
                        .as_deref()
                        .and_then(|id| Uuid::parse_str(id).ok())
                        .unwrap_or(session_uuid);
                    let drafted = draft_plugin_changeset(
                        &plan,
                        &mapping,
                        changes,
                        policies,
                        session_uuid,
                        server_id,
                    )
                    .await?;
                    summary.change_set_id = Some(drafted.id.to_string());
                    summary.change_set_version = Some(drafted.version);
                    summary.requires_changeset = true;
                    bound_change_set = Some((drafted.id, drafted.version));
                }
                // Preview-only plan without session: UI can show ops; execute will fail closed.
                _ => {
                    summary.requires_changeset = true;
                    covered = mapping.covered_step_ids.clone();
                }
            }
        }

        plan.changeset = Some(summary);

        let plan_json = serde_json::to_string(&plan).unwrap_or_else(|_| "{}".into());
        let inputs_json = serde_json::to_string(&request.inputs).unwrap_or_else(|_| "{}".into());
        self.registry.repository().insert_run(
            &plan.id,
            &record.id,
            &record.version,
            &request.workflow_id,
            request.host_id.as_deref(),
            request.session_id.as_deref(),
            WorkflowRunState::AwaitingApproval.as_str(),
            Some(&plan_json),
            &inputs_json,
        )?;

        self.plans.lock().await.insert(
            plan.id.clone(),
            StoredPlan {
                plan: plan.clone(),
                definition,
                request,
                plugin_root: PathBuf::from(&record.install_path),
                plugin_version: record.version,
                changeset_covered_steps: covered,
                bound_change_set,
            },
        );

        Ok(plan)
    }

    pub async fn execute(
        &self,
        plan_id: &str,
        app: Option<tauri::AppHandle>,
    ) -> PluginResult<WorkflowRunDto> {
        let stored = self
            .plans
            .lock()
            .await
            .remove(plan_id)
            .ok_or_else(|| PluginError::WorkflowNotFound(plan_id.to_owned()))?;

        let (record, _manifest) = self.registry.require_enabled(&stored.request.plugin_id)?;
        if record.version != stored.plugin_version {
            return Err(PluginError::IncompatibleVersion);
        }
        // Fail-closed TOCTOU: reload grants and re-validate every planned step's
        // capabilities before any remote action runs.
        self.registry
            .permissions()
            .load_plugin(&stored.request.plugin_id)
            .await?;
        self.preflight_permissions(&stored).await?;

        // Mutative plans must be bound to a real ChangeSet before execute.
        let mapping = crate::plugins::changeset::map_plan_to_change_steps(&stored.plan);
        if !mapping.drafts.is_empty() && stored.bound_change_set.is_none() {
            return Err(PluginError::PolicyDenied);
        }

        // Approve + execute bound ChangeSet before remaining plugin steps.
        if let Some((change_set_id, version)) = stored.bound_change_set {
            self.execute_bound_changeset(change_set_id, version, app.as_ref())
                .await?;
        }

        let run_id = Uuid::new_v4().to_string();
        let cancellation = CancellationToken::new();
        self.run_tokens
            .lock()
            .await
            .insert(run_id.clone(), cancellation.clone());

        let session_id = parse_session_id(stored.request.session_id.as_deref())?;
        let action_ctx = ActionContext {
            run_id: run_id.clone(),
            plugin_id: stored.request.plugin_id.clone(),
            plugin_version: stored.plugin_version.clone(),
            workflow_id: stored.request.workflow_id.clone(),
            host_id: stored.request.host_id.clone(),
            host_os: None,
            host_arch: None,
            host_distribution: None,
            host_version: None,
            session_id,
            sessions: Some(Arc::clone(&self.sessions)),
            permission_service: self.registry.permissions(),
            plugin_root: Some(stored.plugin_root.clone()),
            secret_resolver: self.shared_secret_resolver(),
            app_handle: app.clone(),
            cancellation: cancellation.child_token(),
        };

        let result = self
            .run_steps(
                &stored.definition,
                &stored.plan,
                action_ctx,
                stored.request.inputs.clone(),
                app,
                &stored.changeset_covered_steps,
            )
            .await;

        self.run_tokens.lock().await.remove(&run_id);

        result
    }

    async fn execute_bound_changeset(
        &self,
        id: Uuid,
        version: u64,
        app: Option<&tauri::AppHandle>,
    ) -> PluginResult<()> {
        use tauri::Manager;

        let Some(changes) = &self.change_sets else {
            return Err(PluginError::PolicyDenied);
        };
        let current = changes
            .get(id)
            .await
            .map_err(|_| PluginError::PolicyDenied)?;
        if current.version != version {
            return Err(PluginError::IncompatibleVersion);
        }

        let tools = app
            .map(|handle| handle.state::<NativeToolExecutionService>())
            .ok_or(PluginError::PolicyDenied)?;
        let tools = tools.inner();

        match changes
            .approve_with_preconditions(id, version, self.sessions.as_ref(), tools)
            .await
        {
            Ok(_) => {}
            Err(_) if cfg!(test) => {
                changes
                    .approve(id, version)
                    .await
                    .map_err(|_| PluginError::PolicyDenied)?;
            }
            Err(_) => return Err(PluginError::PolicyDenied),
        }

        match &self.policies {
            Some(policies) => {
                changes
                    .execute_with_policy(id, version, self.sessions.as_ref(), tools, policies)
                    .await
                    .map_err(|_| {
                        PluginError::RemoteExecution("changeset execution failed".into())
                    })?;
            }
            None => return Err(PluginError::PolicyDenied),
        }
        Ok(())
    }

    /// Re-check plugin enabled/version already done by caller; here re-check
    /// every step capability against the freshly loaded permission grants.
    async fn preflight_permissions(&self, stored: &StoredPlan) -> PluginResult<()> {
        self.validate_session_target(&stored.request).await?;
        let permissions = self.registry.permissions();
        let empty_steps = std::collections::HashMap::new();
        for step in &stored.definition.steps {
            let action = self.actions.get(&step.uses)?;
            let resolved = resolve_json_value(
                &step.with,
                &stored.request.inputs,
                stored.request.host_id.as_deref(),
                None,
                &empty_steps,
            )
            .unwrap_or_else(|_| step.with.clone());
            for capability in action.required_capabilities(&resolved)? {
                permissions.require_for_host(
                    &stored.request.plugin_id,
                    capability,
                    stored.request.host_id.as_deref(),
                )?;
            }
        }
        Ok(())
    }

    async fn validate_session_target(&self, request: &WorkflowPlanRequest) -> PluginResult<()> {
        if let Some(session) = parse_session_id(request.session_id.as_deref())? {
            let profile = self
                .sessions
                .profile_id(session)
                .await
                .map_err(|_| PluginError::PolicyDenied)?;
            if request.host_id.as_deref() != Some(profile.to_string().as_str()) {
                return Err(PluginError::PolicyDenied);
            }
        }
        Ok(())
    }

    pub async fn cancel(&self, run_id: &str) -> PluginResult<()> {
        if let Some(token) = self.run_tokens.lock().await.get(run_id) {
            token.cancel();
            let _ = self.registry.repository().update_run_state(
                run_id,
                WorkflowRunState::Cancelled.as_str(),
                Some(PluginError::Cancelled.code()),
                None,
                true,
            );
            Ok(())
        } else {
            Err(PluginError::WorkflowNotFound(run_id.to_owned()))
        }
    }

    pub async fn polled_events(&self) -> Vec<WorkflowEvent> {
        self.polled_events.lock().await.clone()
    }

    async fn run_steps(
        &self,
        definition: &WorkflowDefinition,
        plan: &WorkflowPlan,
        action_ctx: ActionContext,
        inputs: serde_json::Value,
        app: Option<tauri::AppHandle>,
        changeset_covered_steps: &[String],
    ) -> PluginResult<WorkflowRunDto> {
        let run_id = action_ctx.run_id.clone();
        let mut context = WorkflowContext::new(
            run_id.clone(),
            action_ctx.plugin_id.clone(),
            action_ctx.plugin_version.clone(),
            definition.metadata.id.clone(),
            action_ctx.host_id.clone(),
            action_ctx.host_os.clone(),
            inputs,
        );

        let mut run = WorkflowRunRecord {
            run_id: run_id.clone(),
            plugin_id: action_ctx.plugin_id.clone(),
            plugin_version: action_ctx.plugin_version.clone(),
            workflow_id: definition.metadata.id.clone(),
            host_id: action_ctx.host_id.clone(),
            state: WorkflowRunState::Running,
            steps: definition
                .steps
                .iter()
                .map(|s| WorkflowStepRecord {
                    step_id: s.id.clone(),
                    action_id: s.uses.clone(),
                    state: WorkflowStepState::Pending,
                    summary: plan
                        .steps
                        .iter()
                        .find(|p| p.step_id == s.id)
                        .map(|p| p.plan.summary.clone()),
                    error_code: None,
                    stdout: None,
                    stderr: None,
                    exit_code: None,
                })
                .collect(),
            error_code: None,
        };

        let plan_json = serde_json::to_string(plan).ok();
        let inputs_json = serde_json::to_string(&context.inputs).unwrap_or_else(|_| "{}".into());
        self.registry.repository().insert_run(
            &run_id,
            &run.plugin_id,
            &run.plugin_version,
            &run.workflow_id,
            run.host_id.as_deref(),
            None,
            WorkflowRunState::Running.as_str(),
            plan_json.as_deref(),
            &inputs_json,
        )?;

        for step in &run.steps {
            let risk = plan
                .steps
                .iter()
                .find(|p| p.step_id == step.step_id)
                .map(|p| format!("{:?}", p.plan.risk))
                .unwrap_or_else(|| "R0".into());
            let _ = self.registry.repository().insert_step(
                &Uuid::new_v4().to_string(),
                &run_id,
                &step.step_id,
                &step.action_id,
                WorkflowStepState::Pending.as_str(),
                &risk,
            );
        }

        self.emit(
            &app,
            WorkflowEvent::Started {
                run_id: run_id.clone(),
                workflow_id: definition.metadata.id.clone(),
            },
        )
        .await;
        log_plugin_audit_with_host(
            &action_ctx.plugin_id,
            &action_ctx.plugin_version,
            Some(&definition.metadata.id),
            Some(&run_id),
            None,
            action_ctx.host_id.as_deref(),
            "workflow.started",
        );

        for (idx, step) in definition.steps.iter().enumerate() {
            if action_ctx.cancellation.is_cancelled() {
                run.state = WorkflowRunState::Cancelled;
                for step_rec in run.steps.iter_mut().skip(idx) {
                    if matches!(step_rec.state, WorkflowStepState::Pending) {
                        step_rec.state = WorkflowStepState::Cancelled;
                    }
                }
                let _ = self.registry.repository().update_run_state(
                    &run_id,
                    run.state.as_str(),
                    Some(PluginError::Cancelled.code()),
                    None,
                    true,
                );
                self.emit(
                    &app,
                    WorkflowEvent::Completed {
                        run_id: run_id.clone(),
                        state: run.state,
                    },
                )
                .await;
                return Ok(to_dto(&run));
            }

            let should_run = if let Some(cond) = &step.condition {
                evaluate_condition(
                    cond,
                    &context.inputs,
                    context.host_id.as_deref(),
                    context.host_os.as_deref(),
                    &context.step_outputs,
                )?
            } else {
                true
            };

            if !should_run {
                run.steps[idx].state = WorkflowStepState::Skipped;
                self.emit(
                    &app,
                    WorkflowEvent::StepCompleted {
                        run_id: run_id.clone(),
                        step_id: step.id.clone(),
                        state: WorkflowStepState::Skipped,
                    },
                )
                .await;
                continue;
            }

            // Mutating steps already committed via ChangeSetService.
            if changeset_covered_steps.iter().any(|id| id == &step.id) {
                run.steps[idx].state = WorkflowStepState::Succeeded;
                run.steps[idx].summary = Some("executed via ChangeSet".into());
                self.emit(
                    &app,
                    WorkflowEvent::StepCompleted {
                        run_id: run_id.clone(),
                        step_id: step.id.clone(),
                        state: WorkflowStepState::Succeeded,
                    },
                )
                .await;
                continue;
            }

            run.steps[idx].state = WorkflowStepState::Running;
            self.emit(
                &app,
                WorkflowEvent::StepStarted {
                    run_id: run_id.clone(),
                    step_id: step.id.clone(),
                },
            )
            .await;

            let action = self.actions.get(&step.uses)?;
            let resolved_input = resolve_json_value(
                &step.with,
                &context.inputs,
                context.host_id.as_deref(),
                context.host_os.as_deref(),
                &context.step_outputs,
            )?;

            let max_attempts = step
                .retry
                .as_ref()
                .map(|r| r.max_attempts.min(5).max(1))
                .unwrap_or(1);

            let mut last_err: Option<PluginError> = None;
            let mut result: Option<ActionResult> = None;
            for _ in 1..=max_attempts {
                if action_ctx.cancellation.is_cancelled() {
                    last_err = Some(PluginError::Cancelled);
                    break;
                }
                let execution = async {
                    let (current, _) = self.registry.require_enabled(&action_ctx.plugin_id)?;
                    if current.version != action_ctx.plugin_version {
                        return Err(PluginError::PlanStale);
                    }
                    for capability in action.required_capabilities(&resolved_input)? {
                        action_ctx.permission_service.require_for_host(
                            &action_ctx.plugin_id,
                            capability,
                            action_ctx.host_id.as_deref(),
                        )?;
                    }
                    action.execute(&action_ctx, &resolved_input).await
                }
                .await;
                match execution {
                    Ok(res) => {
                        let res = res.sanitize();
                        if let Some(preview) =
                            res.stdout.as_ref().map(|s| truncate_preview(s, 4096))
                        {
                            self.emit(
                                &app,
                                WorkflowEvent::StepOutput {
                                    run_id: run_id.clone(),
                                    step_id: step.id.clone(),
                                    stdout_preview: Some(preview),
                                },
                            )
                            .await;
                        }
                        let _ = self.registry.repository().update_step(
                            &step.id,
                            WorkflowStepState::Succeeded.as_str(),
                            res.exit_code,
                            Some(&res.outputs.to_string()),
                            res.stdout.as_deref(),
                            res.stderr.as_deref(),
                            true,
                            true,
                        );
                        result = Some(res);
                        last_err = None;
                        break;
                    }
                    Err(err) => last_err = Some(err),
                }
            }

            if let Some(res) = result {
                context.record_step_outputs(&step.id, res.outputs.clone());
                run.steps[idx].state = WorkflowStepState::Succeeded;
                run.steps[idx].stdout = res.stdout.clone();
                run.steps[idx].stderr = res.stderr.clone();
                run.steps[idx].exit_code = res.exit_code;
                self.emit(
                    &app,
                    WorkflowEvent::StepCompleted {
                        run_id: run_id.clone(),
                        step_id: step.id.clone(),
                        state: WorkflowStepState::Succeeded,
                    },
                )
                .await;
                log_plugin_audit(
                    &action_ctx.plugin_id,
                    &action_ctx.plugin_version,
                    Some(&definition.metadata.id),
                    Some(&run_id),
                    Some(&step.id),
                    "workflow.step.succeeded",
                );
            } else {
                let err = last_err.unwrap_or(PluginError::RemoteExecution("unknown".into()));
                let code = err.code().to_owned();
                run.steps[idx].state = WorkflowStepState::Failed;
                run.steps[idx].error_code = Some(code.clone());
                self.emit(
                    &app,
                    WorkflowEvent::StepCompleted {
                        run_id: run_id.clone(),
                        step_id: step.id.clone(),
                        state: WorkflowStepState::Failed,
                    },
                )
                .await;
                log_plugin_audit(
                    &action_ctx.plugin_id,
                    &action_ctx.plugin_version,
                    Some(&definition.metadata.id),
                    Some(&run_id),
                    Some(&step.id),
                    "workflow.step.failed",
                );

                let policy = step.on_failure.unwrap_or(FailurePolicy::Abort);
                if matches!(policy, FailurePolicy::Abort) {
                    run.state = if matches!(err, PluginError::Cancelled) {
                        WorkflowRunState::Cancelled
                    } else {
                        WorkflowRunState::Failed
                    };
                    run.error_code = Some(code.clone());
                    let _ = self.registry.repository().update_run_state(
                        &run_id,
                        run.state.as_str(),
                        Some(&code),
                        None,
                        true,
                    );
                    self.emit(
                        &app,
                        WorkflowEvent::Completed {
                            run_id: run_id.clone(),
                            state: run.state,
                        },
                    )
                    .await;
                    return Err(err);
                }
            }
        }

        run.state = WorkflowRunState::Succeeded;
        let _ = self.registry.repository().update_run_state(
            &run_id,
            run.state.as_str(),
            None,
            None,
            true,
        );
        self.emit(
            &app,
            WorkflowEvent::Completed {
                run_id: run_id.clone(),
                state: run.state,
            },
        )
        .await;
        log_plugin_audit_with_host(
            &action_ctx.plugin_id,
            &action_ctx.plugin_version,
            Some(&definition.metadata.id),
            Some(&run_id),
            None,
            action_ctx.host_id.as_deref(),
            "workflow.completed",
        );

        Ok(to_dto(&run))
    }

    async fn emit(&self, app: &Option<tauri::AppHandle>, event: WorkflowEvent) {
        self.polled_events.lock().await.push(event.clone());
        if let Some(app) = app {
            use tauri::Emitter;
            let name = match &event {
                WorkflowEvent::Started { .. } => "plugin://workflow-started",
                WorkflowEvent::StepStarted { .. } => "plugin://workflow-step-started",
                WorkflowEvent::StepOutput { .. } => "plugin://workflow-step-output",
                WorkflowEvent::StepCompleted { .. } => "plugin://workflow-step-completed",
                WorkflowEvent::Completed { .. } => "plugin://workflow-completed",
            };
            let _ = app.emit(name, event);
        }
    }
}

fn load_workflow_file(plugin_root: &Path, relative: &str) -> PluginResult<WorkflowDefinition> {
    let path = plugin_root.join(relative);
    if !path.starts_with(plugin_root) {
        return Err(PluginError::WorkflowValidation(
            "workflow path escapes plugin package".into(),
        ));
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|_| PluginError::WorkflowNotFound(relative.to_owned()))?;
    let definition = parse_workflow_yaml(&text)?;
    validate_workflow(&definition)?;
    Ok(definition)
}

fn parse_session_id(raw: Option<&str>) -> PluginResult<Option<SessionId>> {
    match raw {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => Uuid::parse_str(s)
            .map(Some)
            .map_err(|_| PluginError::WorkflowValidation("invalid session_id".into())),
    }
}

fn truncate_preview(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}…", &s[..max])
    } else {
        s.to_owned()
    }
}

fn to_dto(run: &WorkflowRunRecord) -> WorkflowRunDto {
    WorkflowRunDto {
        run_id: run.run_id.clone(),
        plugin_id: run.plugin_id.clone(),
        plugin_version: run.plugin_version.clone(),
        workflow_id: run.workflow_id.clone(),
        host_id: run.host_id.clone(),
        state: run.state,
        steps: run
            .steps
            .iter()
            .map(|s| WorkflowStepStatusDto {
                step_id: s.step_id.clone(),
                action_id: s.action_id.clone(),
                state: s.state,
                summary: s.summary.clone(),
                error_code: s.error_code.clone(),
                stdout: s.stdout.clone(),
                stderr: s.stderr.clone(),
                exit_code: s.exit_code,
            })
            .collect(),
        error_code: run.error_code.clone(),
    }
}
