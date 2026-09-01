use serde::{Deserialize, Serialize};
use uuid::Uuid;

const MAX_USER_REQUEST_BYTES: usize = 8 * 1024;
const DEFAULT_CONTEXT_TTL_MS: u64 = 30_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ContextSource {
    Server,
    Terminal,
    Incident,
    Files,
    Logs,
    NativeTools,
    Mcp,
    User,
    Skill,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContextFreshness {
    pub observed_at_epoch_ms: u64,
    pub ttl_ms: u64,
}

impl ContextFreshness {
    pub(crate) fn is_fresh(self, now_epoch_ms: u64) -> bool {
        now_epoch_ms.saturating_sub(self.observed_at_epoch_ms) <= self.ttl_ms
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContextBudget {
    pub max_items: usize,
    pub max_bytes: usize,
    pub max_tokens: u32,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_items: 32,
            max_bytes: 64 * 1024,
            max_tokens: 8_192,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContextFact {
    pub source: ContextSource,
    pub target_id: Option<Uuid>,
    pub evidence_id: Option<Uuid>,
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContextSnapshot {
    pub items: Vec<AgentContextItem>,
    pub facts: Vec<ContextFact>,
    pub estimated_tokens: u32,
    pub total_bytes: usize,
    pub compaction_count: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ContextTrust {
    TrustedUserInstruction,
    UntrustedRemoteData,
    UntrustedExternalData,
    ConstrainedSkillGuidance,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentContextItem {
    pub id: Uuid,
    pub source: String,
    pub source_kind: ContextSource,
    pub trust: ContextTrust,
    pub redacted: bool,
    pub content: String,
    pub freshness: ContextFreshness,
    pub estimated_tokens: u32,
}

pub(crate) fn user_context(input: &str, observed_at_epoch_ms: u64) -> AgentContextItem {
    let bounded = if input.len() > MAX_USER_REQUEST_BYTES {
        let mut boundary = MAX_USER_REQUEST_BYTES;
        while !input.is_char_boundary(boundary) {
            boundary -= 1;
        }
        &input[..boundary]
    } else {
        input
    };
    let (content, redacted) = redact_secrets(bounded);
    AgentContextItem {
        id: Uuid::new_v4(),
        source: "user".into(),
        source_kind: ContextSource::User,
        trust: ContextTrust::TrustedUserInstruction,
        redacted,
        content,
        freshness: ContextFreshness {
            observed_at_epoch_ms,
            ttl_ms: u64::MAX,
        },
        estimated_tokens: estimate_tokens(bounded.len()),
    }
}

pub(crate) fn skill_context(
    id: &str,
    instructions: &str,
    observed_at_epoch_ms: u64,
) -> AgentContextItem {
    let (content, redacted) = redact_secrets(instructions);
    AgentContextItem {
        id: Uuid::new_v4(),
        source: format!("skill:{id}"),
        source_kind: ContextSource::Skill,
        trust: ContextTrust::ConstrainedSkillGuidance,
        redacted,
        content,
        freshness: ContextFreshness {
            observed_at_epoch_ms,
            ttl_ms: DEFAULT_CONTEXT_TTL_MS * 10,
        },
        estimated_tokens: estimate_tokens(instructions.len()),
    }
}

pub(crate) fn snapshot(
    mut items: Vec<AgentContextItem>,
    budget: ContextBudget,
    now_epoch_ms: u64,
) -> ContextSnapshot {
    items.retain(|item| item.freshness.is_fresh(now_epoch_ms));
    items.sort_by_key(|item| match item.source_kind {
        ContextSource::User => 0,
        ContextSource::Incident => 1,
        ContextSource::NativeTools => 2,
        _ => 3,
    });
    let mut bytes = 0usize;
    let mut tokens = 0u32;
    items.retain(|item| {
        let next_bytes = bytes.saturating_add(item.content.len());
        let next_tokens = tokens.saturating_add(item.estimated_tokens);
        let keep = next_bytes <= budget.max_bytes
            && next_tokens <= budget.max_tokens
            && bytes < budget.max_bytes;
        if keep {
            bytes = next_bytes;
            tokens = next_tokens;
        }
        keep
    });
    items.truncate(budget.max_items);
    bytes = items.iter().map(|item| item.content.len()).sum();
    tokens = items.iter().map(|item| item.estimated_tokens).sum();
    ContextSnapshot {
        items,
        facts: Vec::new(),
        estimated_tokens: tokens,
        total_bytes: bytes,
        compaction_count: 0,
    }
}

pub(crate) fn compact(snapshot: &mut ContextSnapshot, facts: Vec<ContextFact>) {
    snapshot.facts.extend(facts);
    snapshot.items.retain(|item| {
        matches!(
            item.source_kind,
            ContextSource::User | ContextSource::Incident
        )
    });
    snapshot.total_bytes = snapshot.items.iter().map(|item| item.content.len()).sum();
    snapshot.estimated_tokens = snapshot
        .items
        .iter()
        .map(|item| item.estimated_tokens)
        .sum::<u32>()
        + snapshot
            .facts
            .iter()
            .map(|fact| estimate_tokens(fact.key.len() + fact.value.len()))
            .sum::<u32>();
    snapshot.compaction_count = snapshot.compaction_count.saturating_add(1);
}

fn estimate_tokens(bytes: usize) -> u32 {
    bytes.div_ceil(4).min(u32::MAX as usize) as u32
}

pub(crate) fn redact_secrets(input: &str) -> (String, bool) {
    let mut changed = false;
    let mut output = Vec::new();
    for line in input.lines() {
        let upper = line.to_ascii_uppercase();
        let sensitive = [
            "PASSWORD",
            "PASSWD",
            "SECRET",
            "TOKEN",
            "API_KEY",
            "PRIVATE_KEY",
            "AUTHORIZATION",
            "BEARER ",
            "COOKIE",
            "DATABASE_URL",
        ]
        .iter()
        .any(|marker| upper.contains(marker));
        if sensitive {
            changed = true;
            let key = line
                .split(['=', ':'])
                .next()
                .filter(|value| value.len() <= 64)
                .unwrap_or("SENSITIVE_VALUE")
                .trim();
            output.push(format!("{key}=[REDACTED]"));
        } else {
            output.push(line.to_owned());
        }
    }
    (output.join("\n"), changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_common_secret_shapes_before_model_context() {
        let input = "check nginx\nAuthorization: Bearer abc\nDATABASE_URL=postgres://u:p@host/db";
        let (output, changed) = redact_secrets(input);
        assert!(changed);
        assert!(output.contains("Authorization=[REDACTED]"));
        assert!(!output.contains("abc"));
        assert!(!output.contains("postgres://"));
    }

    #[test]
    fn snapshot_budget_and_compaction_preserve_evidence_references() {
        let mut snapshot = snapshot(
            vec![user_context("investigate nginx", 100)],
            ContextBudget {
                max_items: 1,
                max_bytes: 64,
                max_tokens: 16,
            },
            100,
        );
        let evidence_id = Uuid::new_v4();
        compact(
            &mut snapshot,
            vec![ContextFact {
                source: ContextSource::NativeTools,
                target_id: Some(Uuid::nil()),
                evidence_id: Some(evidence_id),
                key: "nginx.test".into(),
                value: "valid=false".into(),
            }],
        );
        assert!(snapshot.total_bytes <= 64);
        assert_eq!(snapshot.facts[0].evidence_id, Some(evidence_id));
        assert_eq!(snapshot.compaction_count, 1);
    }

    #[test]
    fn freshness_expires_without_changing_source_identity() {
        let freshness = ContextFreshness {
            observed_at_epoch_ms: 1_000,
            ttl_ms: 500,
        };
        assert!(freshness.is_fresh(1_500));
        assert!(!freshness.is_fresh(1_501));
    }
}
