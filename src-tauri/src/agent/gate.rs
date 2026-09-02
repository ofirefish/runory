//! Runtime V2 authorization gate (AR2-C).
//!
//! Whether a validated tool call runs automatically, requires approval, or
//! is blocked is a deterministic Rust decision — never the model's and never
//! React's. The production gate consults the existing `AgentPolicyService`
//! (Phase 10 policy engine, sealed version + hash); the tool execution stack
//! keeps enforcing policy independently at execute time, so the gate can only
//! add an interrupt, never widen permissions.

use async_trait::async_trait;
use uuid::Uuid;

use super::decision::{PreparedCommandProposal, PreparedToolCall};
use crate::policy::{
    AgentPolicyService, PolicyDecision, PolicyEvaluationRequest, PolicyExecutionStrategy,
    PolicyInvocationSource, PolicyTarget,
};
use crate::tools::native_descriptors;

/// Stable code when the policy layer denies a call outright.
pub const AGENT_POLICY_DENIED: &str = "AGENT_POLICY_DENIED";

/// Rust-side authorization for one validated read call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Authorization {
    /// Safe to execute without user interaction (policy allows).
    Auto,
    /// Execution must pause for explicit user approval.
    RequireApproval {
        /// Rust descriptor risk (`R0`…`R4`).
        risk: String,
        policy_version: u64,
        policy_hash: String,
    },
    /// Policy denies the call; it becomes a failed observation.
    Blocked { reason_code: String },
}

/// Commands are never auto-authorized. Rust supplies the sealed policy
/// identity and the deterministic risk label used for exact approval binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandAuthorization {
    RequireApproval {
        risk: String,
        policy_version: u64,
        policy_hash: String,
    },
    Blocked {
        reason_code: String,
    },
}

/// Deterministic authorization decision point for the controller loop.
#[async_trait]
pub trait AuthorizationGate: Send + Sync {
    async fn authorize(&self, call: &PreparedToolCall) -> Authorization;

    async fn authorize_command(&self, command: &PreparedCommandProposal) -> CommandAuthorization {
        CommandAuthorization::RequireApproval {
            risk: command.risk.as_str().into(),
            policy_version: 1,
            policy_hash: "runtime-command-policy-v1".into(),
        }
    }
}

/// Whether a sealed policy snapshot still matches the live policy engine.
#[async_trait]
pub trait PolicySnapshotMatcher: Send + Sync {
    async fn matches(&self, policy_version: u64, policy_hash: &str) -> bool;
}

/// Fixed matcher for unit tests and deterministic resume validation.
#[derive(Clone, Copy, Debug)]
pub struct FixedPolicyMatcher {
    pub matches: bool,
}

#[async_trait]
impl PolicySnapshotMatcher for FixedPolicyMatcher {
    async fn matches(&self, _policy_version: u64, _policy_hash: &str) -> bool {
        self.matches
    }
}

/// Wraps the live policy service for resume-time binding checks.
pub(crate) struct LivePolicyMatcher<'a> {
    policy: &'a AgentPolicyService,
}

#[allow(dead_code)]
impl<'a> LivePolicyMatcher<'a> {
    pub(crate) fn new(policy: &'a AgentPolicyService) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl PolicySnapshotMatcher for LivePolicyMatcher<'_> {
    async fn matches(&self, policy_version: u64, policy_hash: &str) -> bool {
        self.policy
            .current_matches(&crate::policy::PolicySnapshot {
                policy_version,
                policy_hash: policy_hash.to_owned(),
                decision: PolicyDecision::Allow,
                matched_rule_ids: Vec::new(),
            })
            .await
    }
}

/// Test gate driven by a closure.
pub struct FnAuthorizationGate<F>(pub F);

#[async_trait]
impl<F> AuthorizationGate for FnAuthorizationGate<F>
where
    F: Fn(&PreparedToolCall) -> Authorization + Send + Sync,
{
    async fn authorize(&self, call: &PreparedToolCall) -> Authorization {
        (self.0)(call)
    }
}

/// Auto-authorizes every validated call. This reproduces the AR2-B behavior
/// (all calls are structurally read-only) and is the fallback until the IPC
/// stage wires runs to a real policy target.
#[derive(Clone, Copy, Debug, Default)]
pub struct AutoAuthorizationGate;

#[async_trait]
impl AuthorizationGate for AutoAuthorizationGate {
    async fn authorize(&self, _call: &PreparedToolCall) -> Authorization {
        Authorization::Auto
    }
}

/// Production gate over the existing Phase 10 policy engine.
pub(crate) struct PolicyAuthorizationGate<'a> {
    policy: &'a AgentPolicyService,
    server_id: Uuid,
}

#[allow(dead_code)]
impl<'a> PolicyAuthorizationGate<'a> {
    pub(crate) fn new(policy: &'a AgentPolicyService, server_id: Uuid) -> Self {
        Self { policy, server_id }
    }
}

#[async_trait]
impl AuthorizationGate for PolicyAuthorizationGate<'_> {
    async fn authorize(&self, call: &PreparedToolCall) -> Authorization {
        let Some(descriptor) = native_descriptors()
            .into_iter()
            .find(|descriptor| descriptor.name.as_str() == call.tool_name)
        else {
            // Validation already guarantees registry membership; an unknown
            // name here is a regression and must fail closed.
            return Authorization::Blocked {
                reason_code: AGENT_POLICY_DENIED.into(),
            };
        };
        let request = PolicyEvaluationRequest {
            target: PolicyTarget {
                server_id: self.server_id,
                group_id: None,
                environment: None,
            },
            tool: descriptor.name,
            risk_level: descriptor.risk_level,
            resource_impact: descriptor.resource_impact,
            target_count: 1,
            execution_strategy: PolicyExecutionStrategy::Sequential,
            source: PolicyInvocationSource::AgentRuntime,
        };
        match self.policy.evaluate(&request).await {
            Ok(evaluation) => match evaluation.decision {
                PolicyDecision::Allow => Authorization::Auto,
                PolicyDecision::RequireApproval | PolicyDecision::RequireStepApproval => {
                    Authorization::RequireApproval {
                        risk: format!("{:?}", descriptor.risk_level),
                        policy_version: evaluation.policy_version,
                        policy_hash: evaluation.policy_hash,
                    }
                }
                PolicyDecision::Deny => Authorization::Blocked {
                    reason_code: AGENT_POLICY_DENIED.into(),
                },
            },
            Err(error) => Authorization::Blocked {
                reason_code: error.code().into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::decision::ValidatedDecision;
    use crate::agent::decision::{validate_decision, AgentDecision, ToolCallRequest};
    use crate::policy::{PolicyCondition, PolicyEffect, PolicyRule, PolicyScope};
    use serde_json::{json, Value};

    fn call(tool_name: &str) -> PreparedToolCall {
        call_with_args(tool_name, json!({}))
    }

    fn call_with_args(tool_name: &str, arguments: Value) -> PreparedToolCall {
        let decision = AgentDecision::ToolCalls(vec![ToolCallRequest {
            tool_name: tool_name.into(),
            arguments,
            reason_summary: "Collecting evidence".into(),
        }]);
        match validate_decision(decision).expect("decision validates") {
            ValidatedDecision::ToolCalls(mut calls) => calls.remove(0),
            _ => unreachable!("tool call decision"),
        }
    }

    fn global_rule(id: &str, decision: PolicyDecision) -> PolicyRule {
        PolicyRule {
            id: id.into(),
            scope: PolicyScope::Global,
            condition: PolicyCondition::any(),
            effect: PolicyEffect {
                decision,
                max_targets: None,
                reason: format!("{id}_reason"),
            },
            enabled: true,
        }
    }

    async fn gate_decision(rule_decision: PolicyDecision) -> Authorization {
        let policy = AgentPolicyService::default();
        policy
            .replace_rules(vec![global_rule("test-rule", rule_decision)])
            .await
            .expect("rules replace");
        let gate = PolicyAuthorizationGate::new(&policy, Uuid::new_v4());
        gate.authorize(&call("system.disk_usage")).await
    }

    #[tokio::test]
    async fn policy_allow_maps_to_auto() {
        assert_eq!(
            gate_decision(PolicyDecision::Allow).await,
            Authorization::Auto
        );
    }

    #[tokio::test]
    async fn policy_require_approval_maps_to_interrupt_with_sealed_stamp() {
        let authorization = gate_decision(PolicyDecision::RequireApproval).await;
        match authorization {
            Authorization::RequireApproval {
                risk, policy_hash, ..
            } => {
                assert!(!policy_hash.is_empty(), "sealed hash must be present");
                assert!(risk.starts_with('R'), "risk is the Rust descriptor level");
            }
            other => panic!("expected approval requirement, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn policy_deny_maps_to_blocked() {
        assert_eq!(
            gate_decision(PolicyDecision::Deny).await,
            Authorization::Blocked {
                reason_code: AGENT_POLICY_DENIED.into()
            }
        );
    }

    #[tokio::test]
    async fn default_policy_auto_authorizes_low_impact_reads() {
        let policy = AgentPolicyService::default();
        let gate = PolicyAuthorizationGate::new(&policy, Uuid::new_v4());
        assert_eq!(
            gate.authorize(&call("system.disk_usage")).await,
            Authorization::Auto
        );
    }

    #[tokio::test]
    async fn default_policy_requires_approval_for_high_io_scans() {
        let policy = AgentPolicyService::default();
        let gate = PolicyAuthorizationGate::new(&policy, Uuid::new_v4());
        match gate
            .authorize(&call_with_args(
                "system.directory_usage",
                json!({"path":"/var"}),
            ))
            .await
        {
            Authorization::RequireApproval { .. } => {}
            other => panic!("expected high-io approval requirement, got {other:?}"),
        }
    }
}
