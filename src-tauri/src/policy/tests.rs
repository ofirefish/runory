use super::*;
use crate::agentic::PolicyCheckContext;
use crate::tools::{NativeToolName, RiskLevel};
use uuid::Uuid;

fn rule(id: &str, scope: PolicyScope, decision: PolicyDecision) -> PolicyRule {
    PolicyRule {
        id: id.into(),
        scope,
        condition: PolicyCondition::any(),
        effect: PolicyEffect {
            decision,
            max_targets: None,
            reason: format!("{id}_reason"),
        },
        enabled: true,
    }
}

fn request(server_id: Uuid, tool: NativeToolName) -> super::model::PolicyEvaluationRequest {
    super::model::PolicyEvaluationRequest {
        target: PolicyTarget {
            server_id,
            group_id: None,
            environment: None,
        },
        tool,
        risk_level: RiskLevel::R3,
        target_count: 1,
        execution_strategy: PolicyExecutionStrategy::Sequential,
        source: PolicyInvocationSource::NativeTool,
    }
}

fn evaluate(
    rules: Vec<PolicyRule>,
    request: &super::model::PolicyEvaluationRequest,
) -> PolicyEvaluation {
    let set = PolicyEngine::seal(PolicySet {
        policy_version: 7,
        policy_hash: String::new(),
        rules,
    });
    PolicyEngine::evaluate(&set, request)
}

#[test]
fn global_allow_and_global_deny_are_deterministic() {
    let target = Uuid::new_v4();
    let allow = evaluate(
        vec![rule("global", PolicyScope::Global, PolicyDecision::Allow)],
        &request(target, NativeToolName::FilePatch),
    );
    assert_eq!(allow.decision, PolicyDecision::Allow);
    let deny = evaluate(
        vec![rule("global", PolicyScope::Global, PolicyDecision::Deny)],
        &request(target, NativeToolName::FilePatch),
    );
    assert_eq!(deny.decision, PolicyDecision::Deny);
}

#[test]
fn environment_group_server_and_tool_scopes_override_ordinary_parent_rules() {
    let server = Uuid::new_v4();
    let group = Uuid::new_v4();
    let mut input = request(server, NativeToolName::FilePatch);
    input.target.group_id = Some(group);
    input.target.environment = Some("production".into());
    let scopes = [
        PolicyScope::Environment {
            environment: "production".into(),
        },
        PolicyScope::Group { group_id: group },
        PolicyScope::Server { server_id: server },
        PolicyScope::Tool {
            tool: NativeToolName::FilePatch,
        },
    ];
    for scope in scopes {
        let result = evaluate(
            vec![
                rule(
                    "parent",
                    PolicyScope::Global,
                    PolicyDecision::RequireApproval,
                ),
                rule("specific", scope.clone(), PolicyDecision::Allow),
            ],
            &input,
        );
        assert_eq!(result.decision, PolicyDecision::Allow, "scope={scope:?}");
    }
}

#[test]
fn risk_condition_and_conflicting_rules_use_strongest_same_scope_effect() {
    let target = Uuid::new_v4();
    let mut risk = rule(
        "risk",
        PolicyScope::Global,
        PolicyDecision::RequireStepApproval,
    );
    risk.condition.risk_level = Some(RiskLevel::R3);
    let result = evaluate(
        vec![
            rule("allow", PolicyScope::Global, PolicyDecision::Allow),
            risk,
            rule("deny", PolicyScope::Global, PolicyDecision::Deny),
        ],
        &request(target, NativeToolName::FilePatch),
    );
    assert_eq!(result.decision, PolicyDecision::Deny);
}

#[test]
fn inherited_deny_cannot_be_bypassed_by_server_or_tool_allow() {
    let server = Uuid::new_v4();
    let result = evaluate(
        vec![
            rule("inherited", PolicyScope::Global, PolicyDecision::Deny),
            rule(
                "server",
                PolicyScope::Server { server_id: server },
                PolicyDecision::Allow,
            ),
            rule(
                "tool",
                PolicyScope::Tool {
                    tool: NativeToolName::FilePatch,
                },
                PolicyDecision::Allow,
            ),
        ],
        &request(server, NativeToolName::FilePatch),
    );
    assert_eq!(result.decision, PolicyDecision::Deny);
    assert_eq!(result.reason, "inherited_reason");
}

#[test]
fn target_limit_uses_full_target_count_and_cannot_be_evaded_by_batching() {
    let mut limit = rule("limit", PolicyScope::Global, PolicyDecision::Allow);
    limit.effect.max_targets = Some(3);
    let mut input = request(Uuid::new_v4(), NativeToolName::ServiceRestart);
    input.target_count = 4;
    input.execution_strategy = PolicyExecutionStrategy::Rolling;
    let result = evaluate(vec![limit], &input);
    assert_eq!(result.decision, PolicyDecision::Deny);
    assert_eq!(result.reason, "POLICY_TARGET_LIMIT_EXCEEDED");
}

#[tokio::test]
async fn production_parallel_strategy_is_denied() {
    let service = AgentPolicyService::default();
    let set = service.snapshot().await;
    let mut input = request(Uuid::new_v4(), NativeToolName::ServiceRestart);
    input.target.environment = Some("production".into());
    input.target_count = 2;
    input.execution_strategy = PolicyExecutionStrategy::Parallel;
    let result = PolicyEngine::evaluate(&set, &input);
    assert_eq!(result.decision, PolicyDecision::Deny);
}

#[test]
fn skill_and_mcp_sources_cannot_change_policy_semantics() {
    let target = Uuid::new_v4();
    let rules = vec![rule(
        "deny-write",
        PolicyScope::Tool {
            tool: NativeToolName::FilePatch,
        },
        PolicyDecision::Deny,
    )];
    for source in [PolicyInvocationSource::Skill, PolicyInvocationSource::Mcp] {
        let mut input = request(target, NativeToolName::FilePatch);
        input.source = source;
        assert_eq!(
            evaluate(rules.clone(), &input).decision,
            PolicyDecision::Deny
        );
    }
}

#[tokio::test]
async fn policy_change_after_approval_invalidates_snapshot_identity() {
    let service = AgentPolicyService::default();
    let set = service.snapshot().await;
    let evaluation = PolicyEngine::evaluate(
        &set,
        &request(Uuid::new_v4(), NativeToolName::ServiceRestart),
    );
    let snapshot = PolicySnapshot::from(&evaluation);
    assert!(service.current_matches(&snapshot).await);
    service
        .replace_rules(vec![rule(
            "changed",
            PolicyScope::Global,
            PolicyDecision::Deny,
        )])
        .await
        .expect("replace policy");
    assert!(!service.current_matches(&snapshot).await);
}

#[tokio::test]
async fn changeset_approval_binds_policy_snapshot_and_stops_before_execution_on_change() {
    use crate::agentic::{ApprovalState, ChangeSetDraftRequest, ChangeSetService, ChangeStepDraft};
    use crate::storage::JsonRepository;
    use crate::tools::{NativeToolExecutionService, ToolAuditRepository};

    let directory = tempfile::tempdir().expect("temporary directory");
    let changes = ChangeSetService::default();
    let policies = AgentPolicyService::default();
    let session_id = Uuid::new_v4();
    let draft = changes
        .draft(ChangeSetDraftRequest {
            agent_run_id: Uuid::new_v4(),
            session_id,
            title: "policy snapshot".into(),
            steps: vec![ChangeStepDraft::ServiceRestart {
                service: "nginx".into(),
            }],
        })
        .await
        .expect("draft");
    let checked = changes
        .check_policy(
            draft.id,
            draft.version,
            &policies,
            PolicyCheckContext {
                target: PolicyTarget {
                    server_id: session_id,
                    group_id: None,
                    environment: None,
                },
                target_count: 1,
                execution_strategy: PolicyExecutionStrategy::Sequential,
                source: PolicyInvocationSource::NativeTool,
            },
        )
        .await
        .expect("policy preview");
    assert!(checked.policy_snapshot.is_some());
    changes
        .approve(draft.id, draft.version)
        .await
        .expect("approval");
    policies
        .replace_rules(vec![rule(
            "changed",
            PolicyScope::Global,
            PolicyDecision::Deny,
        )])
        .await
        .expect("change policy");
    let tools = NativeToolExecutionService::approved_repair(ToolAuditRepository::new(
        JsonRepository::new(directory.path().join("tool-audit.json")),
    ));
    assert!(changes
        .execute_with_policy(
            draft.id,
            draft.version,
            &crate::ssh::ServerSessionManager::default(),
            &tools,
            &policies,
        )
        .await
        .is_err());
    assert_eq!(
        changes
            .get(draft.id)
            .await
            .expect("invalidated")
            .approval_state,
        ApprovalState::Invalidated
    );
}

#[tokio::test]
async fn require_step_approval_cannot_be_satisfied_by_blanket_approval() {
    use crate::agentic::{ApprovalState, ChangeSetDraftRequest, ChangeSetService, ChangeStepDraft};

    let changes = ChangeSetService::default();
    let policies = AgentPolicyService::default();
    policies
        .replace_rules(vec![rule(
            "step-approval",
            PolicyScope::Global,
            PolicyDecision::RequireStepApproval,
        )])
        .await
        .expect("step policy");
    let session_id = Uuid::new_v4();
    let draft = changes
        .draft(ChangeSetDraftRequest {
            agent_run_id: Uuid::new_v4(),
            session_id,
            title: "step approval".into(),
            steps: vec![
                ChangeStepDraft::ServiceRestart {
                    service: "nginx".into(),
                },
                ChangeStepDraft::ServiceReload {
                    service: "php-fpm".into(),
                },
            ],
        })
        .await
        .expect("draft");
    let checked = changes
        .check_policy(
            draft.id,
            draft.version,
            &policies,
            PolicyCheckContext {
                target: PolicyTarget {
                    server_id: session_id,
                    group_id: None,
                    environment: None,
                },
                target_count: 1,
                execution_strategy: PolicyExecutionStrategy::Sequential,
                source: PolicyInvocationSource::NativeTool,
            },
        )
        .await
        .expect("check");
    assert!(changes.approve(draft.id, draft.version).await.is_err());
    let first = changes
        .approve_step(draft.id, draft.version, checked.steps[0].id)
        .await
        .expect("first step");
    assert_eq!(first.approval_state, ApprovalState::Draft);
    let second = changes
        .approve_step(draft.id, draft.version, checked.steps[1].id)
        .await
        .expect("second step");
    assert_eq!(second.approval_state, ApprovalState::Approved);
    assert_eq!(second.approved_step_ids.len(), 2);
}
