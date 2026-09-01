use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{Mutability, NativeToolName, RiskLevel, ToolDescriptor, ToolScope};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ToolPolicyDenial {
    NotAllowlisted,
    RiskCeilingExceeded,
    WriteToolsDisabled,
    ApprovalEngineUnavailable,
    ScopeNotAllowed,
    AgentPolicyDenied,
}

impl ToolPolicyDenial {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::NotAllowlisted => "TOOL_NOT_ALLOWLISTED",
            Self::RiskCeilingExceeded => "TOOL_RISK_CEILING_EXCEEDED",
            Self::WriteToolsDisabled => "TOOL_WRITE_DISABLED",
            Self::ApprovalEngineUnavailable => "TOOL_APPROVAL_UNAVAILABLE",
            Self::ScopeNotAllowed => "TOOL_SCOPE_NOT_ALLOWED",
            Self::AgentPolicyDenied => "AGENT_POLICY_DENIED",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "decision")]
pub(crate) enum ToolPolicyDecision {
    Allowed,
    Blocked { reason: ToolPolicyDenial },
}

#[derive(Clone, Debug)]
pub(crate) struct ToolPolicy {
    allowlist: BTreeSet<NativeToolName>,
    max_risk: RiskLevel,
    writes_enabled: bool,
    approval_engine_available: bool,
}

impl ToolPolicy {
    pub(crate) fn foundation() -> Self {
        Self {
            allowlist: NativeToolName::READ_ONLY.into_iter().collect(),
            max_risk: RiskLevel::R1,
            writes_enabled: false,
            approval_engine_available: false,
        }
    }

    /// A restricted policy can only remove tools or lower the Phase 10A risk ceiling.
    pub(crate) fn restricted(
        allowlist: impl IntoIterator<Item = NativeToolName>,
        max_risk: RiskLevel,
    ) -> Self {
        let foundation = Self::foundation();
        Self {
            allowlist: allowlist
                .into_iter()
                .filter(|name| foundation.allowlist.contains(name))
                .collect(),
            max_risk: max_risk.min(foundation.max_risk),
            writes_enabled: false,
            approval_engine_available: false,
        }
    }

    pub(crate) fn approved_repair() -> Self {
        Self {
            allowlist: NativeToolName::ALL.into_iter().collect(),
            max_risk: RiskLevel::R3,
            writes_enabled: true,
            approval_engine_available: true,
        }
    }

    pub(crate) fn evaluate(&self, descriptor: &ToolDescriptor) -> ToolPolicyDecision {
        let denial = if !self.allowlist.contains(&descriptor.name) {
            Some(ToolPolicyDenial::NotAllowlisted)
        } else if descriptor.mutability != Mutability::Read && !self.writes_enabled {
            Some(ToolPolicyDenial::WriteToolsDisabled)
        } else if descriptor.requires_approval && !self.approval_engine_available {
            Some(ToolPolicyDenial::ApprovalEngineUnavailable)
        } else if descriptor.scope != ToolScope::Session {
            Some(ToolPolicyDenial::ScopeNotAllowed)
        } else if descriptor.risk_level > self.max_risk {
            Some(ToolPolicyDenial::RiskCeilingExceeded)
        } else {
            None
        };
        denial.map_or(ToolPolicyDecision::Allowed, |reason| {
            ToolPolicyDecision::Blocked { reason }
        })
    }
}

impl Default for ToolPolicy {
    fn default() -> Self {
        Self::foundation()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::native_descriptors;

    #[test]
    fn foundation_allows_only_the_registered_read_only_r0_r1_set() {
        let policy = ToolPolicy::foundation();
        assert!(native_descriptors()
            .iter()
            .take(NativeToolName::READ_ONLY.len())
            .all(|descriptor| policy.evaluate(descriptor) == ToolPolicyDecision::Allowed));
        assert!(native_descriptors()
            .iter()
            .skip(NativeToolName::READ_ONLY.len())
            .all(|descriptor| matches!(
                policy.evaluate(descriptor),
                ToolPolicyDecision::Blocked { .. }
            )));
    }

    #[test]
    fn workspace_restrictions_can_only_tighten_the_foundation_policy() {
        let policy = ToolPolicy::restricted([NativeToolName::SystemInfo], RiskLevel::R4);
        let descriptors = native_descriptors();
        let system = descriptors
            .iter()
            .find(|item| item.name == NativeToolName::SystemInfo)
            .expect("system descriptor");
        let http = descriptors
            .iter()
            .find(|item| item.name == NativeToolName::HttpRequest)
            .expect("http descriptor");
        assert_eq!(policy.evaluate(system), ToolPolicyDecision::Allowed);
        assert_eq!(
            policy.evaluate(http),
            ToolPolicyDecision::Blocked {
                reason: ToolPolicyDenial::NotAllowlisted
            }
        );
    }

    #[test]
    fn r1_is_blocked_when_workspace_ceiling_is_r0() {
        let policy = ToolPolicy::restricted(NativeToolName::ALL, RiskLevel::R0);
        for name in [NativeToolName::HttpRequest, NativeToolName::NginxTest] {
            let descriptor = native_descriptors()
                .into_iter()
                .find(|item| item.name == name)
                .expect("R1 descriptor");
            assert_eq!(descriptor.risk_level, RiskLevel::R1);
            assert_eq!(
                policy.evaluate(&descriptor),
                ToolPolicyDecision::Blocked {
                    reason: ToolPolicyDenial::RiskCeilingExceeded
                }
            );
        }
    }

    #[test]
    fn write_or_approval_descriptors_fail_closed() {
        let policy = ToolPolicy::foundation();
        let mut descriptor = native_descriptors().remove(0);
        descriptor.mutability = Mutability::Write;
        assert_eq!(
            policy.evaluate(&descriptor),
            ToolPolicyDecision::Blocked {
                reason: ToolPolicyDenial::WriteToolsDisabled
            }
        );
        descriptor.mutability = Mutability::Read;
        descriptor.requires_approval = true;
        assert_eq!(
            policy.evaluate(&descriptor),
            ToolPolicyDecision::Blocked {
                reason: ToolPolicyDenial::ApprovalEngineUnavailable
            }
        );
    }
}
