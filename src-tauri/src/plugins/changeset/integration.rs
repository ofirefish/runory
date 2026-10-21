use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agentic::{
    ChangeSet, ChangeSetDraftRequest, ChangeSetService, ChangeStepDraft, PolicyCheckContext,
};
use crate::domain::AppResult;
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::tool_policy_bridge::{
    planned_change_is_mutating, require_tool_policy_for_change,
};
use crate::plugins::workflow::actions::{ActionPlan, PlannedChange};
use crate::plugins::workflow::ast::PrivilegeMode;
use crate::plugins::workflow::planner::WorkflowPlan;
use crate::policy::{
    AgentPolicyService, PolicyExecutionStrategy, PolicyInvocationSource, PolicyTarget,
};
use crate::tools::RiskLevel;

/// UI-facing summary adapted from plugin ActionPlan / WorkflowPlan.
/// R0 plans do not require a Runory ChangeSet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginChangeSetSummary {
    pub plan_id: String,
    pub plugin_id: String,
    pub plugin_version: String,
    pub workflow_id: String,
    pub risk: RiskLevel,
    pub requires_changeset: bool,
    pub requires_approval: bool,
    pub operations: Vec<PluginPlannedOperation>,
    /// Bound when Core drafted a real `ChangeSetService` record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_set_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_set_version: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PluginPlannedOperation {
    RemoteCommand {
        executable: String,
        args: Vec<String>,
        privilege: PrivilegeMode,
        summary: String,
    },
    RemoteFileWrite {
        path: String,
        summary: String,
    },
    RemoteFileDelete {
        path: String,
        summary: String,
    },
    Other {
        summary: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct ChangeSetMapping {
    pub drafts: Vec<ChangeStepDraft>,
    /// Workflow step ids whose mutating changes are covered by `drafts`.
    pub covered_step_ids: Vec<String>,
    pub unmapped_mutating: Vec<String>,
}

pub fn plugin_plan_to_changeset_summary(plan: &WorkflowPlan) -> PluginChangeSetSummary {
    let mut operations = Vec::new();
    for step in &plan.steps {
        operations.extend(action_plan_to_operations(&step.plan));
    }
    let mapping = map_plan_to_change_steps(plan);
    let requires_changeset = plan.requires_approval && !mapping.drafts.is_empty();
    PluginChangeSetSummary {
        plan_id: plan.id.clone(),
        plugin_id: plan.plugin_id.clone(),
        plugin_version: plan.plugin_version.clone(),
        workflow_id: plan.workflow_id.clone(),
        risk: plan.risk,
        requires_changeset,
        requires_approval: plan.requires_approval,
        operations,
        change_set_id: None,
        change_set_version: None,
    }
}

fn action_plan_to_operations(plan: &ActionPlan) -> Vec<PluginPlannedOperation> {
    if plan.changes.is_empty() {
        return vec![PluginPlannedOperation::Other {
            summary: plan.summary.clone(),
        }];
    }
    plan.changes
        .iter()
        .map(|change| match change {
            PlannedChange::RemoteCommand {
                executable,
                args,
                privilege,
            } => PluginPlannedOperation::RemoteCommand {
                executable: executable.clone(),
                args: args.clone(),
                privilege: *privilege,
                summary: plan.summary.clone(),
            },
            PlannedChange::RemoteFileWrite { path } => PluginPlannedOperation::RemoteFileWrite {
                path: path.clone(),
                summary: plan.summary.clone(),
            },
            PlannedChange::RemoteFileDelete { path } => PluginPlannedOperation::RemoteFileDelete {
                path: path.clone(),
                summary: plan.summary.clone(),
            },
            PlannedChange::PackageInstall { packages } => PluginPlannedOperation::Other {
                summary: format!("Install packages: {}", packages.join(", ")),
            },
            PlannedChange::ServiceRestart { service } => PluginPlannedOperation::Other {
                summary: format!("Restart service {service}"),
            },
            PlannedChange::DatabaseMigration { description } => PluginPlannedOperation::Other {
                summary: description.clone(),
            },
        })
        .collect()
}

/// Map mutative planned changes onto existing `ChangeStepDraft` variants.
pub fn map_plan_to_change_steps(plan: &WorkflowPlan) -> ChangeSetMapping {
    let mut mapping = ChangeSetMapping::default();
    for step in &plan.steps {
        let mut step_covered = false;
        for change in &step.plan.changes {
            if !planned_change_is_mutating(change) {
                continue;
            }
            match change {
                // Plugin-owned mutative ops: capability + risk gated; no native draft.
                PlannedChange::RemoteFileWrite { .. }
                | PlannedChange::RemoteFileDelete { .. }
                | PlannedChange::PackageInstall { .. }
                | PlannedChange::DatabaseMigration { .. } => {}
                PlannedChange::RemoteCommand {
                    executable,
                    args,
                    privilege: _,
                } if is_plugin_managed_systemctl_change(executable, args)
                    || is_plugin_managed_python_collectstatic(executable, args)
                    || is_plugin_managed_git_change(executable, args)
                    || is_plugin_managed_python_setup_change(executable, args) => {}
                other => match change_to_draft(other) {
                    Some(draft) => {
                        mapping.drafts.push(draft);
                        step_covered = true;
                    }
                    None => {
                        mapping
                            .unmapped_mutating
                            .push(format!("{}:{}", step.step_id, step.action_id));
                    }
                },
            }
        }
        if step_covered {
            mapping.covered_step_ids.push(step.step_id.clone());
        }
    }
    mapping
}

fn is_plugin_managed_systemctl_change(executable: &str, args: &[String]) -> bool {
    if executable.to_ascii_lowercase() != "systemctl" {
        return false;
    }
    matches!(
        args.first().map(|s| s.as_str()),
        Some("enable" | "daemon-reload" | "disable" | "start")
    )
}

fn is_plugin_managed_python_collectstatic(executable: &str, args: &[String]) -> bool {
    let exe = executable.to_ascii_lowercase();
    (exe.contains("/python") || exe.ends_with("python") || exe.ends_with("python3"))
        && args.first().map(|s| s.as_str()) == Some("manage.py")
        && args.get(1).map(|s| s.as_str()) == Some("collectstatic")
}

fn is_plugin_managed_git_change(executable: &str, args: &[String]) -> bool {
    executable.to_ascii_lowercase() == "git"
        && matches!(
            args.first().map(|s| s.as_str()),
            Some("clone" | "fetch" | "checkout" | "pull")
        )
}

fn is_plugin_managed_python_setup_change(executable: &str, args: &[String]) -> bool {
    let exe = executable.to_ascii_lowercase();
    let is_python =
        exe == "python" || exe == "python3" || exe.starts_with("python") || exe.contains("/python");
    is_python
        && args.first().map(|s| s.as_str()) == Some("-m")
        && matches!(args.get(1).map(|s| s.as_str()), Some("venv" | "pip"))
}

fn change_to_draft(change: &PlannedChange) -> Option<ChangeStepDraft> {
    match change {
        PlannedChange::ServiceRestart { service } => Some(ChangeStepDraft::ServiceRestart {
            service: service.clone(),
        }),
        PlannedChange::RemoteCommand {
            executable,
            args,
            privilege: _,
        } => {
            let exe = executable.to_ascii_lowercase();
            if exe == "systemctl" {
                let action = args.first()?.as_str();
                let service = args.get(1)?.clone();
                match action {
                    "reload" if service == "nginx" => Some(ChangeStepDraft::NginxReload),
                    "reload" => Some(ChangeStepDraft::ServiceReload { service }),
                    "restart" => Some(ChangeStepDraft::ServiceRestart { service }),
                    _ => None,
                }
            } else if exe == "nginx" && args.iter().any(|a| a == "-s") {
                Some(ChangeStepDraft::NginxReload)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Fail-closed: mutative steps must either be ChangeSet-mappable or denied.
pub fn enforce_changeset_mappability(plan: &WorkflowPlan) -> PluginResult<ChangeSetMapping> {
    let mapping = map_plan_to_change_steps(plan);
    for step in &plan.steps {
        for change in &step.plan.changes {
            require_tool_policy_for_change(change, false)?;
        }
    }
    if !mapping.unmapped_mutating.is_empty() {
        return Err(PluginError::PolicyDenied);
    }
    Ok(mapping)
}

/// Draft a real ChangeSet when the plan has mappable writes and a session.
pub async fn draft_plugin_changeset(
    plan: &WorkflowPlan,
    mapping: &ChangeSetMapping,
    changes: &ChangeSetService,
    policies: &AgentPolicyService,
    session_id: Uuid,
    server_id: Uuid,
) -> PluginResult<ChangeSet> {
    if mapping.drafts.is_empty() {
        return Err(PluginError::PolicyDenied);
    }
    let agent_run_id = Uuid::parse_str(&plan.id).unwrap_or_else(|_| Uuid::new_v4());
    let drafted = changes
        .draft(ChangeSetDraftRequest {
            agent_run_id,
            session_id,
            title: format!("plugin:{}:{}", plan.plugin_id, plan.workflow_id),
            steps: mapping.drafts.clone(),
        })
        .await
        .map_err(|_| PluginError::PolicyDenied)?;
    let checked = changes
        .check_policy(
            drafted.id,
            drafted.version,
            policies,
            PolicyCheckContext {
                target: PolicyTarget {
                    server_id,
                    group_id: None,
                    environment: None,
                },
                target_count: 1,
                execution_strategy: PolicyExecutionStrategy::Sequential,
                source: PolicyInvocationSource::AgentRuntime,
            },
        )
        .await
        .map_err(|_| PluginError::PolicyDenied)?;
    if checked
        .policy_evaluation
        .as_ref()
        .is_some_and(|evaluation| !evaluation.permits_execution())
    {
        return Err(PluginError::PolicyDenied);
    }
    Ok(checked)
}

pub fn planned_writes_to_change_step_drafts(plan: &WorkflowPlan) -> Vec<ChangeStepDraft> {
    map_plan_to_change_steps(plan).drafts
}

/// Helper for callers that need AppResult (unused by plugin path directly).
#[allow(dead_code)]
pub async fn get_changeset(changes: &ChangeSetService, id: Uuid) -> AppResult<ChangeSet> {
    changes.get(id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::workflow::actions::ActionPlan;
    use crate::plugins::workflow::planner::WorkflowStepPlan;
    use crate::tools::RiskLevel;

    fn plan_with(changes: Vec<PlannedChange>, risk: RiskLevel) -> WorkflowPlan {
        WorkflowPlan {
            id: Uuid::new_v4().to_string(),
            plugin_id: "runory.nginx-manager".into(),
            plugin_version: "1.0.0".into(),
            workflow_id: "nginx.reload".into(),
            host_id: None,
            risk,
            requires_approval: true,
            steps: vec![WorkflowStepPlan {
                step_id: "reload".into(),
                action_id: "ssh.command".into(),
                plan: ActionPlan {
                    action_id: "ssh.command".into(),
                    risk,
                    summary: "reload".into(),
                    changes,
                    requires_approval: true,
                },
                has_runtime_values: false,
            }],
            changeset: None,
        }
    }

    #[test]
    fn maps_systemctl_reload_nginx() {
        let plan = plan_with(
            vec![PlannedChange::RemoteCommand {
                executable: "systemctl".into(),
                args: vec!["reload".into(), "nginx".into()],
                privilege: PrivilegeMode::Sudo,
            }],
            RiskLevel::R2,
        );
        let mapping = map_plan_to_change_steps(&plan);
        assert_eq!(mapping.drafts.len(), 1);
        assert!(matches!(mapping.drafts[0], ChangeStepDraft::NginxReload));
        assert!(mapping.unmapped_mutating.is_empty());
        enforce_changeset_mappability(&plan).expect("ok");
    }

    #[test]
    fn denies_unmapped_rm() {
        let plan = plan_with(
            vec![PlannedChange::RemoteCommand {
                executable: "rm".into(),
                args: vec!["-rf".into(), "/tmp/x".into()],
                privilege: PrivilegeMode::None,
            }],
            RiskLevel::R4,
        );
        assert!(enforce_changeset_mappability(&plan).is_err());
    }
}
