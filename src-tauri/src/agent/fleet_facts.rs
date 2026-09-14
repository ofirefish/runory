//! Bounded, target-bound Fleet investigation facts.
//!
//! The only ingestion surface accepts `WorkingFactSet`, which is produced from
//! successful typed Tool results. Terminal transcripts, command previews and
//! generic Observation detail intentionally have no aggregation path here.

use std::collections::BTreeMap;

use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

use super::facts::{FactProvenance, WorkingFact, WorkingFactSet};
use super::fleet_target::FleetTargetBinding;
use crate::agentic::context::redact_secrets;

pub const MAX_FLEET_FACTS: usize = 512;
pub const MAX_FLEET_FACTS_PER_TARGET: usize = 96;
pub const MAX_FLEET_FACT_KEY_BYTES: usize = 192;
pub const MAX_FLEET_FACT_VALUE_CHARS: usize = 512;
pub const FLEET_CHILD_TOOL_CALL_BUDGET: u32 = 12;

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum FleetFactError {
    #[error("fleet fact target is outside the exact target binding")]
    TargetMismatch,
    #[error("fleet fact is not backed by typed tool evidence")]
    EvidenceRequired,
    #[error("fleet fact exceeds bounded aggregation limits")]
    LimitExceeded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetFactView {
    pub target_id: Uuid,
    pub role: Option<String>,
    pub key: String,
    pub value: String,
    pub evidence_id: Uuid,
    pub observed_at_epoch_ms: u64,
    pub expires_at_epoch_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FleetComparisonStatus {
    Uniform,
    Divergent,
    Missing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FleetComparisonKind {
    Role,
    Version,
    Service,
    ConfigurationDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetComparisonValue {
    pub target_id: Uuid,
    pub role: Option<String>,
    pub value: Option<String>,
    pub evidence_id: Option<Uuid>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetFactComparison {
    pub kind: FleetComparisonKind,
    pub key: String,
    pub status: FleetComparisonStatus,
    pub targets: Vec<FleetComparisonValue>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetInvestigationView {
    pub facts: Vec<FleetFactView>,
    pub comparisons: Vec<FleetFactComparison>,
}

#[derive(Clone, Debug)]
pub struct FleetFactSet {
    targets: Vec<FleetTargetBinding>,
    facts: BTreeMap<(Uuid, String), FleetFactView>,
}

impl FleetFactSet {
    pub fn new(targets: Vec<FleetTargetBinding>) -> Self {
        Self {
            targets,
            facts: BTreeMap::new(),
        }
    }

    pub fn merge(
        &mut self,
        expected_target_id: Uuid,
        source: &WorkingFactSet,
        now_epoch_ms: u64,
    ) -> Result<usize, FleetFactError> {
        let binding = self
            .targets
            .iter()
            .find(|target| target.profile_id == expected_target_id)
            .ok_or(FleetFactError::TargetMismatch)?;
        let mut staged = BTreeMap::new();
        for fact in source.all() {
            if !fact.is_fresh(now_epoch_ms) {
                continue;
            }
            let view = validated_view(binding, expected_target_id, fact)?;
            staged.insert((expected_target_id, view.key.clone()), view);
        }
        let new_keys = staged
            .keys()
            .filter(|key| !self.facts.contains_key(*key))
            .count();
        let target_existing = self
            .facts
            .keys()
            .filter(|(target, _)| *target == expected_target_id)
            .count();
        if self.facts.len().saturating_add(new_keys) > MAX_FLEET_FACTS
            || target_existing.saturating_add(new_keys) > MAX_FLEET_FACTS_PER_TARGET
        {
            return Err(FleetFactError::LimitExceeded);
        }
        let accepted = staged.len();
        for (key, view) in staged {
            self.facts.insert(key, view);
        }
        Ok(accepted)
    }

    pub fn view(&self, now_epoch_ms: u64) -> FleetInvestigationView {
        let facts = self
            .facts
            .values()
            .filter(|fact| fact.expires_at_epoch_ms > now_epoch_ms)
            .cloned()
            .collect::<Vec<_>>();
        let mut keys = facts
            .iter()
            .filter_map(|fact| comparison_kind(&fact.key).map(|kind| (kind, fact.key.clone())))
            .collect::<Vec<_>>();
        keys.sort_by(|left, right| left.1.cmp(&right.1));
        keys.dedup();

        let mut comparisons = vec![self.role_comparison()];
        comparisons.extend(keys.into_iter().map(|(kind, key)| {
            let targets = self
                .targets
                .iter()
                .map(|binding| {
                    let fact = facts
                        .iter()
                        .find(|fact| fact.target_id == binding.profile_id && fact.key == key);
                    FleetComparisonValue {
                        target_id: binding.profile_id,
                        role: binding.role.clone(),
                        value: fact.map(|fact| fact.value.clone()),
                        evidence_id: fact.map(|fact| fact.evidence_id),
                    }
                })
                .collect::<Vec<_>>();
            FleetFactComparison {
                kind,
                key,
                status: comparison_status(&targets),
                targets,
            }
        }));
        FleetInvestigationView { facts, comparisons }
    }

    fn role_comparison(&self) -> FleetFactComparison {
        let targets = self
            .targets
            .iter()
            .map(|target| FleetComparisonValue {
                target_id: target.profile_id,
                role: target.role.clone(),
                value: target.role.clone(),
                evidence_id: None,
            })
            .collect::<Vec<_>>();
        FleetFactComparison {
            kind: FleetComparisonKind::Role,
            key: "fleet.role".into(),
            status: comparison_status(&targets),
            targets,
        }
    }
}

fn validated_view(
    binding: &FleetTargetBinding,
    expected_target_id: Uuid,
    fact: &WorkingFact,
) -> Result<FleetFactView, FleetFactError> {
    if fact.target_id != Some(expected_target_id) {
        return Err(FleetFactError::TargetMismatch);
    }
    if fact.provenance != FactProvenance::ToolObservation
        || fact.source_tool.as_deref().unwrap_or_default().is_empty()
        || fact.tool_call_id.is_none()
    {
        return Err(FleetFactError::EvidenceRequired);
    }
    if fact.key.is_empty() || fact.key.len() > MAX_FLEET_FACT_KEY_BYTES {
        return Err(FleetFactError::LimitExceeded);
    }
    let (redacted, _) = redact_secrets(&fact.value);
    let value = redacted.chars().take(MAX_FLEET_FACT_VALUE_CHARS).collect();
    Ok(FleetFactView {
        target_id: expected_target_id,
        role: binding.role.clone(),
        key: fact.key.clone(),
        value,
        evidence_id: fact.tool_call_id.ok_or(FleetFactError::EvidenceRequired)?,
        observed_at_epoch_ms: fact.observed_at_epoch_ms,
        expires_at_epoch_ms: fact.observed_at_epoch_ms.saturating_add(fact.ttl_ms),
    })
}

fn comparison_kind(key: &str) -> Option<FleetComparisonKind> {
    if key == "os" || key == "kernel" || key.ends_with(".version") {
        Some(FleetComparisonKind::Version)
    } else if key.starts_with("service.") && key.ends_with(".status") {
        Some(FleetComparisonKind::Service)
    } else if key.ends_with(".config_digest") {
        Some(FleetComparisonKind::ConfigurationDigest)
    } else {
        None
    }
}

fn comparison_status(targets: &[FleetComparisonValue]) -> FleetComparisonStatus {
    if targets.iter().any(|target| target.value.is_none()) {
        return FleetComparisonStatus::Missing;
    }
    let mut values = targets.iter().filter_map(|target| target.value.as_deref());
    let first = values.next();
    if values.all(|value| Some(value) == first) {
        FleetComparisonStatus::Uniform
    } else {
        FleetComparisonStatus::Divergent
    }
}

#[cfg(test)]
mod tests {
    use super::super::facts::DEFAULT_FACT_TTL_MS;
    use super::*;

    fn targets() -> Vec<FleetTargetBinding> {
        vec![
            FleetTargetBinding {
                profile_id: Uuid::new_v4(),
                session_id: Uuid::new_v4(),
                role: Some("source".into()),
                ordinal: 0,
            },
            FleetTargetBinding {
                profile_id: Uuid::new_v4(),
                session_id: Uuid::new_v4(),
                role: Some("replica".into()),
                ordinal: 1,
            },
        ]
    }

    fn fact(target_id: Uuid, key: &str, value: &str, now: u64) -> WorkingFact {
        WorkingFact {
            id: Uuid::new_v4(),
            key: key.into(),
            value: value.into(),
            source_event_seq: Some(1),
            source_tool: Some("system.info".into()),
            tool_call_id: Some(Uuid::new_v4()),
            target_id: Some(target_id),
            observed_at_epoch_ms: now,
            ttl_ms: DEFAULT_FACT_TTL_MS,
            provenance: FactProvenance::ToolObservation,
        }
    }

    fn set(facts: Vec<WorkingFact>) -> WorkingFactSet {
        WorkingFactSet::from_snapshot_json(&serde_json::to_string(&facts).expect("serialize"))
            .expect("facts")
    }

    #[test]
    fn aggregates_exact_target_facts_and_compares_values() {
        let targets = targets();
        let mut fleet = FleetFactSet::new(targets.clone());
        fleet
            .merge(
                targets[0].profile_id,
                &set(vec![
                    fact(targets[0].profile_id, "kernel", "6.8", 100),
                    fact(targets[0].profile_id, "service.mysql.status", "active", 100),
                    fact(targets[0].profile_id, "mysql.config_digest", "aaa", 100),
                ]),
                101,
            )
            .expect("source facts");
        fleet
            .merge(
                targets[1].profile_id,
                &set(vec![
                    fact(targets[1].profile_id, "kernel", "6.8", 100),
                    fact(
                        targets[1].profile_id,
                        "service.mysql.status",
                        "inactive",
                        100,
                    ),
                    fact(targets[1].profile_id, "mysql.config_digest", "bbb", 100),
                ]),
                101,
            )
            .expect("replica facts");

        let view = fleet.view(102);
        assert_eq!(view.facts.len(), 6);
        assert!(view
            .comparisons
            .iter()
            .any(|item| { item.key == "kernel" && item.status == FleetComparisonStatus::Uniform }));
        assert!(view.comparisons.iter().any(|item| {
            item.key == "service.mysql.status" && item.status == FleetComparisonStatus::Divergent
        }));
        assert!(view.comparisons.iter().any(|item| {
            item.key == "mysql.config_digest"
                && item.kind == FleetComparisonKind::ConfigurationDigest
        }));
    }

    #[test]
    fn rejects_targetless_user_and_cross_target_facts() {
        let targets = targets();
        let mut fleet = FleetFactSet::new(targets.clone());
        let mut user = fact(targets[0].profile_id, "kernel", "6.8", 100);
        user.provenance = FactProvenance::UserInput;
        assert_eq!(
            fleet.merge(targets[0].profile_id, &set(vec![user]), 101),
            Err(FleetFactError::EvidenceRequired)
        );
        assert_eq!(
            fleet.merge(
                targets[0].profile_id,
                &set(vec![fact(targets[1].profile_id, "kernel", "6.8", 100)]),
                101,
            ),
            Err(FleetFactError::TargetMismatch)
        );
        assert!(fleet.view(101).facts.is_empty());
    }

    #[test]
    fn stale_facts_are_not_exposed_and_missing_is_explicit() {
        let targets = targets();
        let mut fleet = FleetFactSet::new(targets.clone());
        fleet
            .merge(
                targets[0].profile_id,
                &set(vec![fact(targets[0].profile_id, "kernel", "6.8", 100)]),
                101,
            )
            .expect("facts");
        let partial = fleet.view(102);
        assert!(partial
            .comparisons
            .iter()
            .any(|item| { item.key == "kernel" && item.status == FleetComparisonStatus::Missing }));
        let expired = fleet.view(100 + DEFAULT_FACT_TTL_MS);
        assert!(expired.facts.is_empty());
    }
}
