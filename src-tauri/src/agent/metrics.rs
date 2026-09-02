//! Runtime V2 per-run observability metrics (AR2-I).
//!
//! Metrics are content-free counters and latencies suitable for persistence,
//! audit export and release-gate benchmarking. They never carry secrets or
//! model private reasoning.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Observability snapshot for one V2 run (aligned with AGENT_RUNTIME_V2.md §38).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunMetrics {
    pub run_id: Uuid,
    pub reasoner_rounds: u32,
    pub model_calls: u32,
    pub tool_calls: u32,
    pub auto_authorized_tools: u32,
    pub approval_interrupts: u32,
    pub rejections: u32,
    pub tool_failures: u32,
    pub recovery_attempts: u32,
    pub context_tokens: u32,
    pub artifact_bytes: u64,
    pub time_to_first_observation_ms: Option<u64>,
    pub time_to_diagnosis_ms: Option<u64>,
    pub time_to_resolution_ms: Option<u64>,
    pub verification_result: Option<bool>,
    pub rollback_result: Option<bool>,
}

impl RunMetrics {
    pub fn new(run_id: Uuid) -> Self {
        Self {
            run_id,
            ..Self::default()
        }
    }

    pub fn record_reasoner_round(&mut self, context_tokens: u32) {
        self.reasoner_rounds = self.reasoner_rounds.saturating_add(1);
        self.model_calls = self.model_calls.saturating_add(1);
        self.context_tokens = self.context_tokens.max(context_tokens);
    }

    pub fn record_tool_call(&mut self, auto_authorized: bool) {
        self.tool_calls = self.tool_calls.saturating_add(1);
        if auto_authorized {
            self.auto_authorized_tools = self.auto_authorized_tools.saturating_add(1);
        }
    }

    pub fn record_tool_failure(&mut self) {
        self.tool_failures = self.tool_failures.saturating_add(1);
    }

    pub fn record_recovery_attempt(&mut self) {
        self.recovery_attempts = self.recovery_attempts.saturating_add(1);
    }

    pub fn record_approval_interrupt(&mut self) {
        self.approval_interrupts = self.approval_interrupts.saturating_add(1);
    }

    pub fn record_rejection(&mut self) {
        self.rejections = self.rejections.saturating_add(1);
    }

    pub fn record_artifact_bytes(&mut self, bytes: u64) {
        self.artifact_bytes = self.artifact_bytes.saturating_add(bytes);
    }

    pub fn maybe_record_first_observation(&mut self, elapsed_ms: u64) {
        if self.time_to_first_observation_ms.is_none() {
            self.time_to_first_observation_ms = Some(elapsed_ms);
        }
    }

    pub fn maybe_record_diagnosis(&mut self, elapsed_ms: u64) {
        if self.time_to_diagnosis_ms.is_none() {
            self.time_to_diagnosis_ms = Some(elapsed_ms);
        }
    }

    pub fn record_resolution(&mut self, elapsed_ms: u64) {
        self.time_to_resolution_ms = Some(elapsed_ms);
    }

    pub fn record_verification(&mut self, success: bool) {
        self.verification_result = Some(success);
    }

    pub fn record_rollback(&mut self, success: bool) {
        self.rollback_result = Some(success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_accumulate_without_overflow_panic() {
        let mut metrics = RunMetrics::new(Uuid::new_v4());
        metrics.record_reasoner_round(512);
        metrics.record_tool_call(true);
        metrics.record_tool_failure();
        metrics.record_approval_interrupt();
        metrics.record_rejection();
        metrics.record_recovery_attempt();
        metrics.record_artifact_bytes(8192);
        metrics.maybe_record_first_observation(42);
        metrics.record_verification(true);
        metrics.record_rollback(false);
        assert_eq!(metrics.reasoner_rounds, 1);
        assert_eq!(metrics.auto_authorized_tools, 1);
        assert_eq!(metrics.tool_failures, 1);
        assert_eq!(metrics.verification_result, Some(true));
    }

    #[test]
    fn metrics_round_trip_through_json() {
        let mut metrics = RunMetrics::new(Uuid::new_v4());
        metrics.record_reasoner_round(100);
        let json = serde_json::to_string(&metrics).expect("serialize");
        assert!(!json.contains("password"));
        let back: RunMetrics = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, metrics);
    }
}
