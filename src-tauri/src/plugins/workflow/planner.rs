use std::sync::Arc;

use uuid::Uuid;

use super::actions::{ActionContext, ActionPlan, ActionRegistry};
use super::ast::WorkflowDefinition;
use super::expressions::{evaluate_condition, resolve_json_value};
use super::validator::validate_workflow;
use crate::plugins::error::PluginResult;
use crate::tools::RiskLevel;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepPlan {
    pub step_id: String,
    pub action_id: String,
    pub plan: ActionPlan,
    /// True when step `with` still contains unresolved runtime expressions.
    pub has_runtime_values: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowPlan {
    pub id: String,
    pub plugin_id: String,
    pub plugin_version: String,
    pub workflow_id: String,
    pub host_id: Option<String>,
    pub risk: RiskLevel,
    pub requires_approval: bool,
    pub steps: Vec<WorkflowStepPlan>,
    /// Present for R1+ plans so UI can render ChangeSet-style approval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changeset: Option<crate::plugins::changeset::PluginChangeSetSummary>,
}

pub struct WorkflowPlanner {
    action_registry: Arc<ActionRegistry>,
}

impl WorkflowPlanner {
    pub fn new(action_registry: Arc<ActionRegistry>) -> Self {
        Self { action_registry }
    }

    pub async fn plan(
        &self,
        definition: &WorkflowDefinition,
        ctx: &ActionContext,
        inputs: &serde_json::Value,
    ) -> PluginResult<WorkflowPlan> {
        validate_workflow(definition)?;

        let empty_steps = std::collections::HashMap::new();
        let mut step_plans = Vec::with_capacity(definition.steps.len());
        let mut max_risk = RiskLevel::R0;
        let mut requires_approval = false;

        for step in &definition.steps {
            // Skip planning capability/input validation for steps that are
            // already known to be skipped with the provided inputs.
            if let Some(cond) = &step.condition {
                if let Ok(false) = evaluate_condition(
                    cond,
                    inputs,
                    ctx.host_id.as_deref(),
                    ctx.host_os.as_deref(),
                    &empty_steps,
                ) {
                    step_plans.push(WorkflowStepPlan {
                        step_id: step.id.clone(),
                        action_id: step.uses.clone(),
                        plan: ActionPlan {
                            action_id: step.uses.clone(),
                            risk: RiskLevel::R0,
                            summary: format!("{} (skipped by condition)", step.uses),
                            changes: Vec::new(),
                            requires_approval: false,
                        },
                        has_runtime_values: false,
                    });
                    continue;
                }
            }

            let action = self.action_registry.get(&step.uses)?;
            let resolved_input = match resolve_json_value(
                &step.with,
                inputs,
                ctx.host_id.as_deref(),
                ctx.host_os.as_deref(),
                &empty_steps,
            ) {
                Ok(value) => value,
                Err(_) => {
                    // Plan-time: leave dynamic values marked rather than faking them.
                    step.with.clone()
                }
            };
            let has_runtime_values = value_has_unresolved(&resolved_input);

            for cap in action.required_capabilities(&resolved_input)? {
                ctx.permission_service.require_for_host(
                    &ctx.plugin_id,
                    cap,
                    ctx.host_id.as_deref(),
                )?;
            }

            let plan = if has_runtime_values {
                // Static plan stub when inputs depend on prior step outputs.
                let risk = action.risk(&resolved_input).unwrap_or(RiskLevel::R3);
                ActionPlan {
                    action_id: step.uses.clone(),
                    risk,
                    summary: format!("{} (<resolved at runtime>)", step.uses),
                    changes: Vec::new(),
                    requires_approval: !matches!(risk, RiskLevel::R0),
                }
            } else {
                action.plan(ctx, &resolved_input).await?
            };

            if plan.risk > max_risk {
                max_risk = plan.risk;
            }
            requires_approval |= plan.requires_approval;

            step_plans.push(WorkflowStepPlan {
                step_id: step.id.clone(),
                action_id: step.uses.clone(),
                plan,
                has_runtime_values,
            });
        }

        Ok(WorkflowPlan {
            id: Uuid::new_v4().to_string(),
            plugin_id: ctx.plugin_id.clone(),
            plugin_version: ctx.plugin_version.clone(),
            workflow_id: definition.metadata.id.clone(),
            host_id: ctx.host_id.clone(),
            risk: max_risk,
            requires_approval,
            steps: step_plans,
            changeset: None,
        })
    }
}

fn value_has_unresolved(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(s) => s.contains("{{"),
        serde_json::Value::Array(items) => items.iter().any(value_has_unresolved),
        serde_json::Value::Object(map) => map.values().any(value_has_unresolved),
        _ => false,
    }
}
