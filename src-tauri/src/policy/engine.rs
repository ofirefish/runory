use ring::digest::{digest, SHA256};

use super::model::{
    PolicyDecision, PolicyEvaluation, PolicyEvaluationRequest, PolicyMatchedRule, PolicyRule,
    PolicyScope, PolicySet,
};

pub(crate) struct PolicyEngine;

impl PolicyEngine {
    pub(crate) fn seal(mut set: PolicySet) -> PolicySet {
        set.policy_hash.clear();
        set.rules.sort_by(|left, right| left.id.cmp(&right.id));
        let encoded = serde_json::to_vec(&set.rules).unwrap_or_default();
        set.policy_hash = digest(&SHA256, &encoded)
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        set
    }

    pub(crate) fn evaluate(set: &PolicySet, request: &PolicyEvaluationRequest) -> PolicyEvaluation {
        let mut matches = set
            .rules
            .iter()
            .filter(|rule| rule.enabled && matches_rule(rule, request))
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| {
            left.scope
                .specificity()
                .cmp(&right.scope.specificity())
                .then_with(|| left.id.cmp(&right.id))
        });

        let target_limit = matches
            .iter()
            .filter_map(|rule| rule.effect.max_targets.map(|limit| (*rule, limit)))
            .min_by_key(|(_, limit)| *limit);
        let limit_denied = target_limit
            .filter(|(_, limit)| request.target_count > *limit)
            .map(|(rule, _)| rule);
        let inherited_deny = matches
            .iter()
            .copied()
            .find(|rule| rule.effect.decision == PolicyDecision::Deny);

        let decisive = limit_denied.or(inherited_deny).or_else(|| {
            let max_specificity = matches.last().map(|rule| rule.scope.specificity())?;
            matches
                .iter()
                .copied()
                .filter(|rule| rule.scope.specificity() == max_specificity)
                .max_by_key(|rule| rule.effect.decision.precedence())
        });

        let (decision, reason, scope) = if let Some(rule) = limit_denied {
            (
                PolicyDecision::Deny,
                "POLICY_TARGET_LIMIT_EXCEEDED".to_owned(),
                rule.scope.clone(),
            )
        } else if let Some(rule) = decisive {
            (
                rule.effect.decision,
                rule.effect.reason.clone(),
                rule.scope.clone(),
            )
        } else {
            (
                PolicyDecision::Deny,
                "POLICY_NO_MATCH".to_owned(),
                PolicyScope::Global,
            )
        };

        PolicyEvaluation {
            decision,
            matched_rules: matches
                .into_iter()
                .map(|rule| PolicyMatchedRule {
                    rule_id: rule.id.clone(),
                    scope: rule.scope.clone(),
                    decision: rule.effect.decision,
                    reason: rule.effect.reason.clone(),
                })
                .collect(),
            reason,
            scope,
            policy_version: set.policy_version,
            policy_hash: set.policy_hash.clone(),
        }
    }
}

fn matches_rule(rule: &PolicyRule, request: &PolicyEvaluationRequest) -> bool {
    rule.scope.matches(&request.target, request.tool)
        && rule.condition.tool.is_none_or(|tool| tool == request.tool)
        && rule
            .condition
            .risk_level
            .is_none_or(|risk| risk == request.risk_level)
        && rule
            .condition
            .minimum_target_count
            .is_none_or(|count| request.target_count >= count)
        && rule
            .condition
            .execution_strategy
            .is_none_or(|strategy| strategy == request.execution_strategy)
        && rule
            .condition
            .resource_impact
            .is_none_or(|impact| impact == request.resource_impact)
}
