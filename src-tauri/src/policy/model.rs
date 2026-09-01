use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::tools::{NativeToolName, RiskLevel};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum PolicyDecision {
    Allow,
    RequireApproval,
    RequireStepApproval,
    Deny,
}

impl PolicyDecision {
    pub(crate) const fn precedence(self) -> u8 {
        match self {
            Self::Allow => 0,
            Self::RequireApproval => 1,
            Self::RequireStepApproval => 2,
            Self::Deny => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PolicyExecutionStrategy {
    Sequential,
    Parallel,
    Canary,
    Rolling,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PolicyInvocationSource {
    NativeTool,
    AgentRuntime,
    OperationsPack,
    Skill,
    Mcp,
    MultiServerChangeSet,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub(crate) enum PolicyScope {
    Global,
    Environment { environment: String },
    Group { group_id: Uuid },
    Server { server_id: Uuid },
    Tool { tool: NativeToolName },
}

impl PolicyScope {
    pub(crate) const fn specificity(&self) -> u8 {
        match self {
            Self::Global => 0,
            Self::Environment { .. } => 1,
            Self::Group { .. } => 2,
            Self::Server { .. } => 3,
            Self::Tool { .. } => 4,
        }
    }

    pub(crate) fn matches(&self, target: &PolicyTarget, tool: NativeToolName) -> bool {
        match self {
            Self::Global => true,
            Self::Environment { environment } => target
                .environment
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(environment)),
            Self::Group { group_id } => target.group_id == Some(*group_id),
            Self::Server { server_id } => target.server_id == *server_id,
            Self::Tool { tool: expected } => *expected == tool,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PolicyCondition {
    #[serde(default)]
    pub tool: Option<NativeToolName>,
    #[serde(default)]
    pub risk_level: Option<RiskLevel>,
    #[serde(default)]
    pub minimum_target_count: Option<usize>,
    #[serde(default)]
    pub execution_strategy: Option<PolicyExecutionStrategy>,
}

impl PolicyCondition {
    #[cfg(test)]
    pub(crate) const fn any() -> Self {
        Self {
            tool: None,
            risk_level: None,
            minimum_target_count: None,
            execution_strategy: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PolicyEffect {
    pub decision: PolicyDecision,
    #[serde(default)]
    pub max_targets: Option<usize>,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PolicyRule {
    pub id: String,
    pub scope: PolicyScope,
    pub condition: PolicyCondition,
    pub effect: PolicyEffect,
    #[serde(default = "enabled")]
    pub enabled: bool,
}

const fn enabled() -> bool {
    true
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PolicySet {
    pub policy_version: u64,
    pub policy_hash: String,
    pub rules: Vec<PolicyRule>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PolicyTarget {
    pub server_id: Uuid,
    #[serde(default)]
    pub group_id: Option<Uuid>,
    #[serde(default)]
    pub environment: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct PolicyEvaluationRequest {
    pub target: PolicyTarget,
    pub tool: NativeToolName,
    pub risk_level: RiskLevel,
    /// Always the complete requested target set, never the current execution batch.
    pub target_count: usize,
    pub execution_strategy: PolicyExecutionStrategy,
    pub source: PolicyInvocationSource,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PolicyMatchedRule {
    pub rule_id: String,
    pub scope: PolicyScope,
    pub decision: PolicyDecision,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PolicyEvaluation {
    pub decision: PolicyDecision,
    pub matched_rules: Vec<PolicyMatchedRule>,
    pub reason: String,
    pub scope: PolicyScope,
    pub policy_version: u64,
    pub policy_hash: String,
}

impl PolicyEvaluation {
    pub(crate) fn permits_execution(&self) -> bool {
        self.decision != PolicyDecision::Deny
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PolicySnapshot {
    pub policy_version: u64,
    pub policy_hash: String,
    pub decision: PolicyDecision,
    pub matched_rule_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EffectiveAgentPolicy {
    pub target: PolicyTarget,
    pub policy_version: u64,
    pub policy_hash: String,
    pub evaluations: Vec<PolicyEvaluation>,
}

impl From<&PolicyEvaluation> for PolicySnapshot {
    fn from(value: &PolicyEvaluation) -> Self {
        Self {
            policy_version: value.policy_version,
            policy_hash: value.policy_hash.clone(),
            decision: value.decision,
            matched_rule_ids: value
                .matched_rules
                .iter()
                .map(|rule| rule.rule_id.clone())
                .collect(),
        }
    }
}
