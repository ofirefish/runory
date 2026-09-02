//! Working facts for Runtime V2 (AR2-H).
//!
//! Compact, evidence-backed facts derived from tool observations. They feed
//! model context via Phase 10K `ContextFact`, but never silently satisfy
//! Verification or ChangeSet preconditions — those always re-query live state.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agentic::context::redact_secrets;
use crate::agentic::context::{ContextFact, ContextFreshness, ContextSource};
use crate::tools::{ToolData, ToolResult};

/// Default TTL for tool-derived facts (30s), matching ObservationCache.
pub const DEFAULT_FACT_TTL_MS: u64 = 30_000;

/// How a fact was obtained.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactProvenance {
    ToolObservation,
    UserInput,
    Runtime,
}

/// One compact, evidence-backed working fact.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkingFact {
    pub id: Uuid,
    pub key: String,
    pub value: String,
    pub source_event_seq: Option<u64>,
    pub source_tool: Option<String>,
    pub tool_call_id: Option<Uuid>,
    pub target_id: Option<Uuid>,
    pub observed_at_epoch_ms: u64,
    pub ttl_ms: u64,
    pub provenance: FactProvenance,
}

impl WorkingFact {
    pub fn is_fresh(&self, now_epoch_ms: u64) -> bool {
        ContextFreshness {
            observed_at_epoch_ms: self.observed_at_epoch_ms,
            ttl_ms: self.ttl_ms,
        }
        .is_fresh(now_epoch_ms)
    }

    pub fn to_context_fact(&self) -> ContextFact {
        ContextFact {
            source: ContextSource::NativeTools,
            target_id: self.target_id,
            evidence_id: self.tool_call_id,
            key: self.key.clone(),
            value: self.value.clone(),
        }
    }
}

/// Upsert-by-key set of working facts for one run.
#[derive(Clone, Debug, Default)]
pub struct WorkingFactSet {
    facts: Vec<WorkingFact>,
}

impl WorkingFactSet {
    pub fn len(&self) -> usize {
        self.facts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }

    pub fn all(&self) -> &[WorkingFact] {
        &self.facts
    }

    pub fn fresh(&self, now_epoch_ms: u64) -> Vec<&WorkingFact> {
        self.facts
            .iter()
            .filter(|fact| fact.is_fresh(now_epoch_ms))
            .collect()
    }

    /// Inserts or replaces by `(target_id, key)`. Returns keys that changed.
    pub fn upsert_many(&mut self, incoming: Vec<WorkingFact>) -> Vec<String> {
        let mut changed = Vec::new();
        for fact in incoming {
            if let Some(existing) = self
                .facts
                .iter_mut()
                .find(|item| item.key == fact.key && item.target_id == fact.target_id)
            {
                if existing.value != fact.value
                    || existing.observed_at_epoch_ms != fact.observed_at_epoch_ms
                {
                    changed.push(fact.key.clone());
                    *existing = fact;
                }
            } else {
                changed.push(fact.key.clone());
                self.facts.push(fact);
            }
        }
        changed
    }

    /// Drops facts for a target after a write (cache/fact invalidation).
    pub fn invalidate_target(&mut self, target_id: Uuid) -> usize {
        let before = self.facts.len();
        self.facts.retain(|fact| fact.target_id != Some(target_id));
        before.saturating_sub(self.facts.len())
    }

    pub fn snapshot_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.facts)
    }

    pub fn from_snapshot_json(raw: &str) -> Result<Self, serde_json::Error> {
        let facts: Vec<WorkingFact> = serde_json::from_str(raw)?;
        Ok(Self { facts })
    }
}

/// Extracts compact facts from a successful typed tool result.
pub fn extract_facts_from_result(
    result: &ToolResult,
    tool_call_id: Uuid,
    target_id: Option<Uuid>,
    observed_at_epoch_ms: u64,
    source_event_seq: Option<u64>,
) -> Vec<WorkingFact> {
    if !result.success {
        return Vec::new();
    }
    let Some(data) = result.data.as_ref() else {
        return Vec::new();
    };
    let tool_name = result.tool_name.as_str().to_owned();
    let mut facts = Vec::new();
    match data {
        ToolData::SystemInfo(info) => {
            push_fact(
                &mut facts,
                "os",
                &info.operating_system,
                &tool_name,
                tool_call_id,
                target_id,
                observed_at_epoch_ms,
                source_event_seq,
            );
            push_fact(
                &mut facts,
                "hostname",
                &info.hostname,
                &tool_name,
                tool_call_id,
                target_id,
                observed_at_epoch_ms,
                source_event_seq,
            );
            push_fact(
                &mut facts,
                "kernel",
                &info.kernel_release,
                &tool_name,
                tool_call_id,
                target_id,
                observed_at_epoch_ms,
                source_event_seq,
            );
        }
        ToolData::SystemDisk(disk) => {
            for entry in &disk.disks {
                let key = format!("disk.usage.{}", entry.mount);
                let value = format!("{:.0}%", entry.usage_percent);
                push_fact(
                    &mut facts,
                    &key,
                    &value,
                    &tool_name,
                    tool_call_id,
                    target_id,
                    observed_at_epoch_ms,
                    source_event_seq,
                );
            }
        }
        ToolData::ServiceStatus(status) => {
            let key = format!("service.{}.status", status.service.name);
            let value = format!("{:?}", status.service.status).to_ascii_lowercase();
            push_fact(
                &mut facts,
                &key,
                &value,
                &tool_name,
                tool_call_id,
                target_id,
                observed_at_epoch_ms,
                source_event_seq,
            );
        }
        ToolData::ServiceLogs(logs) => {
            let key = format!("service.{}.logs_available", logs.service);
            push_fact(
                &mut facts,
                &key,
                &(!logs.entries.is_empty()).to_string(),
                &tool_name,
                tool_call_id,
                target_id,
                observed_at_epoch_ms,
                source_event_seq,
            );
            push_fact(
                &mut facts,
                &format!("service.{}.log_lines", logs.service),
                &logs.entries.len().to_string(),
                &tool_name,
                tool_call_id,
                target_id,
                observed_at_epoch_ms,
                source_event_seq,
            );
        }
        ToolData::NetworkPortCheck(port) => {
            let key = format!("port.{}.{}", port.host, port.port);
            push_fact(
                &mut facts,
                &key,
                if port.reachable { "open" } else { "closed" },
                &tool_name,
                tool_call_id,
                target_id,
                observed_at_epoch_ms,
                source_event_seq,
            );
        }
        ToolData::HttpResponse(http) => {
            push_fact(
                &mut facts,
                "http.status",
                &http.status_code.to_string(),
                &tool_name,
                tool_call_id,
                target_id,
                observed_at_epoch_ms,
                source_event_seq,
            );
        }
        ToolData::NginxTest(nginx) => {
            push_fact(
                &mut facts,
                "nginx.config_valid",
                &nginx.valid.to_string(),
                &tool_name,
                tool_call_id,
                target_id,
                observed_at_epoch_ms,
                source_event_seq,
            );
            if let Some(path) = &nginx.config_file {
                push_fact(
                    &mut facts,
                    "nginx.config_file",
                    path,
                    &tool_name,
                    tool_call_id,
                    target_id,
                    observed_at_epoch_ms,
                    source_event_seq,
                );
            }
        }
        ToolData::Diagnostic(diag) => {
            if diag.category == "docker-inspect" {
                if let Some(container) = diag.fields.get("container").and_then(|v| v.as_str()) {
                    let running = diag
                        .fields
                        .get("running")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    push_fact(
                        &mut facts,
                        &format!("docker.{container}.running"),
                        &running.to_string(),
                        &tool_name,
                        tool_call_id,
                        target_id,
                        observed_at_epoch_ms,
                        source_event_seq,
                    );
                }
            } else if diag.category == "file" {
                if let Some(path) = diag.fields.get("path").and_then(|v| v.as_str()) {
                    push_fact(
                        &mut facts,
                        &format!("file.exists.{path}"),
                        "true",
                        &tool_name,
                        tool_call_id,
                        target_id,
                        observed_at_epoch_ms,
                        source_event_seq,
                    );
                }
            }
        }
        ToolData::FilePatch(_) | ToolData::ServiceChange(_) => {
            // Write results invalidate facts elsewhere; do not seed from writes.
        }
    }
    facts
}

fn push_fact(
    facts: &mut Vec<WorkingFact>,
    key: &str,
    value: &str,
    tool_name: &str,
    tool_call_id: Uuid,
    target_id: Option<Uuid>,
    observed_at_epoch_ms: u64,
    source_event_seq: Option<u64>,
) {
    let (value, _) = redact_secrets(value);
    if key.is_empty() || value.is_empty() {
        return;
    }
    facts.push(WorkingFact {
        id: Uuid::new_v4(),
        key: key.to_owned(),
        value,
        source_event_seq,
        source_tool: Some(tool_name.to_owned()),
        tool_call_id: Some(tool_call_id),
        target_id,
        observed_at_epoch_ms,
        ttl_ms: DEFAULT_FACT_TTL_MS,
        provenance: FactProvenance::ToolObservation,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{NativeToolName, NginxTestData};

    fn nginx_result(valid: bool) -> ToolResult {
        ToolResult {
            invocation_id: Uuid::new_v4(),
            tool_name: NativeToolName::NginxTest,
            success: true,
            summary: "ok",
            data: Some(ToolData::NginxTest(NginxTestData {
                valid,
                config_file: Some("/etc/nginx/nginx.conf".into()),
                error_file: None,
                error_line: None,
                error_message: None,
                raw_summary: "syntax ok".into(),
            })),
            error_code: None,
            warnings: Vec::new(),
            started_at_epoch_ms: 0,
            duration_ms: 1,
            truncated: false,
            cancelled: false,
            untrusted_remote_data: true,
        }
    }

    #[test]
    fn extracts_nginx_facts_and_upserts_by_key() {
        let call_id = Uuid::new_v4();
        let target = Uuid::new_v4();
        let extracted =
            extract_facts_from_result(&nginx_result(true), call_id, Some(target), 100, Some(7));
        assert!(extracted
            .iter()
            .any(|f| f.key == "nginx.config_valid" && f.value == "true"));
        let mut set = WorkingFactSet::default();
        let changed = set.upsert_many(extracted);
        assert!(changed.contains(&"nginx.config_valid".to_owned()));
        let changed_again = set.upsert_many(extract_facts_from_result(
            &nginx_result(true),
            call_id,
            Some(target),
            100,
            Some(8),
        ));
        assert!(changed_again.is_empty());
    }

    #[test]
    fn stale_facts_are_filtered_and_writes_invalidate_target() {
        let target = Uuid::new_v4();
        let mut set = WorkingFactSet::default();
        set.upsert_many(vec![WorkingFact {
            id: Uuid::new_v4(),
            key: "service.nginx.status".into(),
            value: "active".into(),
            source_event_seq: Some(1),
            source_tool: Some("service.status".into()),
            tool_call_id: Some(Uuid::new_v4()),
            target_id: Some(target),
            observed_at_epoch_ms: 0,
            ttl_ms: 10,
            provenance: FactProvenance::ToolObservation,
        }]);
        assert!(set.fresh(5).len() == 1);
        assert!(set.fresh(20).is_empty());
        assert_eq!(set.invalidate_target(target), 1);
        assert!(set.is_empty());
    }
}
