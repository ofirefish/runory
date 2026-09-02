use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::domain::AppResult;
use crate::storage::JsonRepository;
use crate::tools::ResourceImpact;

use super::engine::PolicyEngine;
use super::model::{
    EffectiveAgentPolicy, PolicyCondition, PolicyDecision, PolicyEffect, PolicyEvaluation,
    PolicyEvaluationRequest, PolicyExecutionStrategy, PolicyRule, PolicyScope, PolicySet,
    PolicySnapshot, PolicyTarget,
};

const MAX_POLICY_AUDIT_RECORDS: usize = 2_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PolicyAuditRecord {
    id: Uuid,
    decision: PolicyDecision,
    matched_rule_ids: Vec<String>,
    reason: String,
    policy_version: u64,
    policy_hash: String,
    target: Uuid,
    tool: String,
    source: super::model::PolicyInvocationSource,
    timestamp_epoch_ms: u64,
}

#[derive(Clone)]
pub(crate) struct AgentPolicyService {
    set: Arc<RwLock<PolicySet>>,
    repository: Option<JsonRepository<PolicySet>>,
    audit_repository: Option<JsonRepository<Vec<PolicyAuditRecord>>>,
}

impl Default for AgentPolicyService {
    fn default() -> Self {
        Self::new(default_policy())
    }
}

impl AgentPolicyService {
    pub(crate) fn new(set: PolicySet) -> Self {
        Self {
            set: Arc::new(RwLock::new(PolicyEngine::seal(set))),
            repository: None,
            audit_repository: None,
        }
    }

    pub(crate) fn at_path(
        policy_path: impl Into<std::path::PathBuf>,
        audit_path: impl Into<std::path::PathBuf>,
    ) -> Self {
        Self {
            set: Arc::new(RwLock::new(PolicyEngine::seal(default_policy()))),
            repository: Some(JsonRepository::new(policy_path)),
            audit_repository: Some(JsonRepository::new(audit_path)),
        }
    }

    pub(crate) async fn load(&self) -> AppResult<()> {
        let Some(repository) = &self.repository else {
            return Ok(());
        };
        let loaded = repository.load_or_default().await?;
        let sealed = if loaded.rules.is_empty() {
            PolicyEngine::seal(default_policy())
        } else {
            PolicyEngine::seal(loaded)
        };
        repository.save_atomic(&sealed).await?;
        *self.set.write().await = sealed;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn snapshot(&self) -> PolicySet {
        self.set.read().await.clone()
    }

    pub(crate) async fn current_matches(&self, snapshot: &PolicySnapshot) -> bool {
        let set = self.set.read().await;
        set.policy_version == snapshot.policy_version && set.policy_hash == snapshot.policy_hash
    }

    /// Sealed policy identity used to bind a user-approved Runtime V2 command.
    /// Command execution is always approval-gated; this snapshot ensures a
    /// policy change between display and execution invalidates the approval.
    pub(crate) async fn command_approval_identity(&self) -> (u64, String) {
        let set = self.set.read().await;
        (set.policy_version, set.policy_hash.clone())
    }

    pub(crate) async fn evaluate(
        &self,
        request: &PolicyEvaluationRequest,
    ) -> AppResult<PolicyEvaluation> {
        let set = self.set.read().await;
        let evaluation = PolicyEngine::evaluate(&set, request);
        drop(set);
        if let Some(repository) = &self.audit_repository {
            let mut records = repository.load_or_default().await?;
            records.push(PolicyAuditRecord {
                id: Uuid::new_v4(),
                decision: evaluation.decision,
                matched_rule_ids: evaluation
                    .matched_rules
                    .iter()
                    .map(|rule| rule.rule_id.clone())
                    .collect(),
                reason: evaluation.reason.clone(),
                policy_version: evaluation.policy_version,
                policy_hash: evaluation.policy_hash.clone(),
                target: request.target.server_id,
                tool: request.tool.as_str().to_owned(),
                source: request.source,
                timestamp_epoch_ms: now_epoch_ms(),
            });
            if records.len() > MAX_POLICY_AUDIT_RECORDS {
                records.drain(..records.len() - MAX_POLICY_AUDIT_RECORDS);
            }
            repository.save_atomic(&records).await?;
        }
        Ok(evaluation)
    }

    pub(crate) async fn effective_for_server(&self, target: PolicyTarget) -> EffectiveAgentPolicy {
        let set = self.set.read().await;
        let evaluations = crate::tools::native_descriptors()
            .into_iter()
            .map(|descriptor| {
                PolicyEngine::evaluate(
                    &set,
                    &PolicyEvaluationRequest {
                        target: target.clone(),
                        tool: descriptor.name,
                        risk_level: descriptor.risk_level,
                        resource_impact: descriptor.resource_impact,
                        target_count: 1,
                        execution_strategy: PolicyExecutionStrategy::Sequential,
                        source: super::model::PolicyInvocationSource::NativeTool,
                    },
                )
            })
            .collect();
        EffectiveAgentPolicy {
            target,
            policy_version: set.policy_version,
            policy_hash: set.policy_hash.clone(),
            evaluations,
        }
    }

    #[cfg(test)]
    pub(crate) async fn replace_rules(&self, rules: Vec<PolicyRule>) -> AppResult<PolicySet> {
        let version = self.set.read().await.policy_version.saturating_add(1);
        let sealed = PolicyEngine::seal(PolicySet {
            policy_version: version,
            policy_hash: String::new(),
            rules,
        });
        if let Some(repository) = &self.repository {
            repository.save_atomic(&sealed).await?;
        }
        *self.set.write().await = sealed.clone();
        Ok(sealed)
    }
}

fn default_policy() -> PolicySet {
    let global =
        |id: &str, risk_level, resource_impact, decision, reason: &str, max_targets| PolicyRule {
            id: id.to_owned(),
            scope: PolicyScope::Global,
            condition: PolicyCondition {
                tool: None,
                risk_level,
                minimum_target_count: None,
                execution_strategy: None,
                resource_impact,
            },
            effect: PolicyEffect {
                decision,
                max_targets,
                reason: reason.to_owned(),
            },
            enabled: true,
        };
    PolicySet {
        policy_version: 1,
        policy_hash: String::new(),
        rules: vec![
            global(
                "global-agent-baseline",
                None,
                None,
                PolicyDecision::Allow,
                "POLICY_GLOBAL_ALLOW",
                Some(10),
            ),
            global(
                "high-io-scan-approval",
                None,
                Some(ResourceImpact::HighIo),
                PolicyDecision::RequireApproval,
                "POLICY_HIGH_IO_APPROVAL_REQUIRED",
                None,
            ),
            global(
                "risk-r2-approval",
                Some(crate::tools::RiskLevel::R2),
                None,
                PolicyDecision::RequireApproval,
                "POLICY_RISK_APPROVAL_REQUIRED",
                None,
            ),
            global(
                "risk-r3-approval",
                Some(crate::tools::RiskLevel::R3),
                None,
                PolicyDecision::RequireApproval,
                "POLICY_RISK_APPROVAL_REQUIRED",
                None,
            ),
            global(
                "risk-r4-step-approval",
                Some(crate::tools::RiskLevel::R4),
                None,
                PolicyDecision::RequireStepApproval,
                "POLICY_STEP_APPROVAL_REQUIRED",
                None,
            ),
            PolicyRule {
                id: "production-parallel-all-denied".into(),
                scope: PolicyScope::Environment {
                    environment: "production".into(),
                },
                condition: PolicyCondition {
                    tool: None,
                    risk_level: None,
                    minimum_target_count: Some(2),
                    execution_strategy: Some(PolicyExecutionStrategy::Parallel),
                    resource_impact: None,
                },
                effect: PolicyEffect {
                    decision: PolicyDecision::Deny,
                    max_targets: None,
                    reason: "POLICY_PRODUCTION_PARALLEL_DENIED".into(),
                },
                enabled: true,
            },
        ],
    }
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
