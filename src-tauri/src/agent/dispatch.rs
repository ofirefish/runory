//! Runtime V2 read-tool dispatch boundary (AR2-B).
//!
//! `ToolDispatcher` is the seam between the controller loop and the existing
//! tool execution stack. The production implementation wraps the real
//! `NativeToolExecutionService` (Policy → Risk → Audit stays inside it) and
//! reuses the Phase 10K `ObservationCache` and dedup semantics — V2 does not
//! build a second cache or a second policy path. Outcomes are sanitized V2
//! DTOs: the legacy `ToolResult` never leaks into the public V2 contract.

use async_trait::async_trait;
use futures_util::future::join_all;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use tokio::sync::watch;
use uuid::Uuid;

use super::command::CommandOutcome;
use super::decision::PreparedCommandProposal;
use super::decision::PreparedToolCall;
use super::run::now_epoch_ms;
use crate::agentic::context::redact_secrets;
use crate::agentic::ObservationCache;
use crate::domain::SessionId;
use crate::ssh::ServerSessionManager;
use crate::tools::{NativeToolExecutionService, NativeToolRequest, ToolResult};

/// Sanitized result of one dispatched read tool call.
#[derive(Clone, Debug)]
pub struct ToolOutcome {
    pub tool_call_id: Uuid,
    pub tool_name: String,
    pub success: bool,
    /// Static, sanitized result summary from the tool layer.
    pub summary: String,
    pub error_code: Option<String>,
    pub duration_ms: u64,
    pub cancelled: bool,
    /// Served from the Phase 10K observation cache (or batch dedup).
    pub from_cache: bool,
    pub untrusted_remote_data: bool,
    /// Redacted JSON rendering of the structured tool data, if any.
    pub sanitized_data: Option<String>,
    /// Full typed result for evidence-bound change proposals (not IPC).
    pub(crate) tool_result: Option<ToolResult>,
}

impl ToolOutcome {
    pub(crate) fn from_result(
        call: &PreparedToolCall,
        result: &ToolResult,
        from_cache: bool,
    ) -> Self {
        let sanitized_data = result
            .data
            .as_ref()
            .and_then(|data| serde_json::to_string(data).ok())
            .map(|json| redact_secrets(&json).0);
        Self {
            tool_call_id: call.tool_call_id,
            tool_name: call.tool_name.clone(),
            success: result.success,
            summary: result.summary.to_owned(),
            error_code: result
                .error_code
                .map(|code| super::tool_error::classify_tool_error(code).to_owned()),
            duration_ms: result.duration_ms,
            cancelled: result.cancelled,
            from_cache,
            untrusted_remote_data: result.untrusted_remote_data,
            sanitized_data,
            tool_result: result.success.then(|| result.clone()),
        }
    }

    pub(crate) fn failure(call: &PreparedToolCall, error_code: &str) -> Self {
        Self {
            tool_call_id: call.tool_call_id,
            tool_name: call.tool_name.clone(),
            success: false,
            summary: "tool execution failed".into(),
            error_code: Some(error_code.to_owned()),
            duration_ms: 0,
            cancelled: false,
            from_cache: false,
            untrusted_remote_data: false,
            sanitized_data: None,
            tool_result: None,
        }
    }
}

/// Executes validated read tool calls. Implementations must return exactly
/// one outcome per call, never panic, and report failures as structured
/// outcomes — a failed tool is an observation, not a run failure.
#[async_trait]
pub trait ToolDispatcher: Send + Sync {
    async fn execute_reads(&self, run_id: Uuid, calls: &[PreparedToolCall]) -> Vec<ToolOutcome>;

    async fn execute_command(
        &self,
        _run_id: Uuid,
        command: &PreparedCommandProposal,
    ) -> CommandOutcome {
        CommandOutcome::failure(command.command_id, "COMMAND_EXECUTOR_UNAVAILABLE")
    }
}

/// Production dispatcher over the existing typed tool stack.
pub(crate) struct NativeReadDispatcher<'a> {
    sessions: &'a ServerSessionManager,
    tools: &'a NativeToolExecutionService,
    cache: &'a ObservationCache,
    session_id: SessionId,
    target_id: Uuid,
    cancellation: watch::Receiver<bool>,
}

impl<'a> NativeReadDispatcher<'a> {
    pub(crate) fn new(
        sessions: &'a ServerSessionManager,
        tools: &'a NativeToolExecutionService,
        cache: &'a ObservationCache,
        session_id: SessionId,
        target_id: Uuid,
        cancellation: watch::Receiver<bool>,
    ) -> Self {
        Self {
            sessions,
            tools,
            cache,
            session_id,
            target_id,
            cancellation,
        }
    }

    async fn execute_one(&self, run_id: Uuid, call: &PreparedToolCall, now: u64) -> ToolOutcome {
        if let Some(cached) = self.cache.get(self.target_id, &call.invocation, now).await {
            return ToolOutcome::from_result(call, &cached, true);
        }
        let request = NativeToolRequest::with_cancellation_for_agent(
            run_id,
            self.session_id,
            call.invocation.clone(),
            self.cancellation.clone(),
        );
        match self.tools.execute(self.sessions, request).await {
            Ok(result) => {
                if result.success && !result.cancelled {
                    self.cache
                        .put(self.target_id, &call.invocation, now, &result)
                        .await;
                }
                ToolOutcome::from_result(call, &result, false)
            }
            Err(error) => ToolOutcome::failure(call, error.code()),
        }
    }
}

#[async_trait]
impl ToolDispatcher for NativeReadDispatcher<'_> {
    async fn execute_reads(&self, run_id: Uuid, calls: &[PreparedToolCall]) -> Vec<ToolOutcome> {
        let now = now_epoch_ms();

        // Phase 10K dedup semantics: identical invocations in one batch
        // execute once. The key is an in-memory deterministic rendering of
        // the typed invocation (the legacy `cache_key` helper is private to
        // `agentic`); it is never persisted.
        let mut first_by_key: HashMap<String, usize> = HashMap::new();
        let mut unique_indices: Vec<usize> = Vec::new();
        let mut duplicate_of: Vec<(usize, usize)> = Vec::new();
        for (index, call) in calls.iter().enumerate() {
            let key = format!("{:?}", call.invocation);
            match first_by_key.entry(key) {
                Entry::Occupied(entry) => duplicate_of.push((index, *entry.get())),
                Entry::Vacant(entry) => {
                    entry.insert(index);
                    unique_indices.push(index);
                }
            }
        }

        let executions = unique_indices
            .iter()
            .map(|&index| self.execute_one(run_id, &calls[index], now));
        let executed = join_all(executions).await;

        let mut outcomes: Vec<Option<ToolOutcome>> = vec![None; calls.len()];
        for (&index, outcome) in unique_indices.iter().zip(executed) {
            outcomes[index] = Some(outcome);
        }
        for (duplicate, source) in duplicate_of {
            if let Some(original) = outcomes[source].clone() {
                outcomes[duplicate] = Some(ToolOutcome {
                    tool_call_id: calls[duplicate].tool_call_id,
                    from_cache: true,
                    ..original
                });
            }
        }
        outcomes.into_iter().flatten().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::decision::ValidatedDecision;
    use crate::agent::decision::{validate_decision, AgentDecision, ToolCallRequest};
    use crate::policy::AgentPolicyService;
    use crate::storage::JsonRepository;
    use crate::tools::{NativeToolName, ToolAuditRepository};
    use serde_json::json;
    use tempfile::TempDir;

    fn prepared(tool_name: &str, arguments: serde_json::Value) -> Vec<PreparedToolCall> {
        let decision = AgentDecision::ToolCalls(vec![ToolCallRequest {
            tool_name: tool_name.into(),
            arguments,
            reason_summary: "Collecting evidence".into(),
        }]);
        match validate_decision(decision).expect("decision validates") {
            ValidatedDecision::ToolCalls(calls) => calls,
            _ => unreachable!("tool call decision"),
        }
    }

    async fn tool_service(directory: &TempDir) -> (NativeToolExecutionService, AgentPolicyService) {
        let policy = AgentPolicyService::at_path(
            directory.path().join("agent-policy.json"),
            directory.path().join("agent-policy-audit.json"),
        );
        policy.load().await.expect("policy loads");
        let service = NativeToolExecutionService::approved_repair_with_agent_policy(
            ToolAuditRepository::new(JsonRepository::new(directory.path().join("audit.json"))),
            policy.clone(),
        );
        (service, policy)
    }

    fn successful_result(invocation_id: Uuid, name: NativeToolName) -> ToolResult {
        ToolResult {
            invocation_id,
            tool_name: name,
            success: true,
            summary: "collected filesystem usage",
            data: None,
            error_code: None,
            warnings: Vec::new(),
            started_at_epoch_ms: now_epoch_ms(),
            duration_ms: 5,
            truncated: false,
            cancelled: false,
            untrusted_remote_data: true,
        }
    }

    #[tokio::test]
    async fn cache_hits_and_batch_duplicates_reuse_one_observation() {
        let directory = TempDir::new().expect("temp dir");
        let (tools, _policy) = tool_service(&directory).await;
        let sessions = ServerSessionManager::default();
        let cache = ObservationCache::default();
        let target_id = Uuid::new_v4();
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        // Pre-populate the Phase 10K cache with a successful observation.
        let mut calls = prepared("system.disk_usage", json!({}));
        calls.extend(prepared("system.disk_usage", json!({})));
        cache
            .put(
                target_id,
                &calls[0].invocation,
                now_epoch_ms(),
                &successful_result(Uuid::new_v4(), NativeToolName::SystemDisk),
            )
            .await;

        let dispatcher = NativeReadDispatcher::new(
            &sessions,
            &tools,
            &cache,
            Uuid::new_v4(),
            target_id,
            cancel_rx,
        );
        let outcomes = dispatcher.execute_reads(Uuid::new_v4(), &calls).await;

        assert_eq!(outcomes.len(), 2, "one outcome per call");
        for (outcome, call) in outcomes.iter().zip(&calls) {
            assert_eq!(outcome.tool_call_id, call.tool_call_id);
            assert!(outcome.success);
            assert!(outcome.from_cache, "cache/dedup must serve both calls");
        }
    }

    #[tokio::test]
    async fn missing_session_is_a_structured_failure_not_a_panic() {
        let directory = TempDir::new().expect("temp dir");
        let (tools, _policy) = tool_service(&directory).await;
        let sessions = ServerSessionManager::default();
        let cache = ObservationCache::default();
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        let calls = prepared("system.info", json!({}));
        let dispatcher = NativeReadDispatcher::new(
            &sessions,
            &tools,
            &cache,
            Uuid::new_v4(),
            Uuid::new_v4(),
            cancel_rx,
        );
        let outcomes = dispatcher.execute_reads(Uuid::new_v4(), &calls).await;

        assert_eq!(outcomes.len(), 1);
        assert!(!outcomes[0].success);
        assert!(
            outcomes[0].error_code.is_some(),
            "failure carries a stable error code"
        );
        assert!(!outcomes[0].from_cache);
    }
}
