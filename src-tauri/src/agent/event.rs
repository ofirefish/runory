//! Runtime V2 event contract.
//!
//! `AgentEvent` is a closed, typed enum — not a `String` event type with an
//! untyped JSON payload — so the compiler owns the contract internally. The
//! stable wire/persistence form is produced by serde as
//! `{ "type": "...", "payload": { ... } }` (snake_case), which AR2-D SQLite
//! persistence and the future IPC channel reuse unchanged.
//!
//! Payloads are intentionally minimal for AR2-A: enough to render a timeline
//! entry and correlate follow-up events, without importing tool / approval /
//! ChangeSet domain types that later stages own. Events must never contain
//! secrets or model private chain-of-thought; `ProgressUpdated.summary` and
//! similar fields are user-visible progress summaries only.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Every Runtime V2 event type (35 variants), per `AGENT_RUNTIME_V2.md` §12.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum AgentEvent {
    // Run lifecycle
    RunCreated,
    RunStarted,
    RunResumed,
    RunPaused,
    RunCancelled,
    RunCompleted,
    RunFailed {
        error_code: String,
    },

    // Conversation
    UserMessageAdded {
        content: String,
    },
    AssistantMessageAdded {
        content: String,
    },

    // Reasoning progress (concise summaries only; never chain-of-thought)
    ReasoningStarted,
    ProgressUpdated {
        summary: String,
    },

    // Tool activity
    ToolRequested {
        tool_call_id: Uuid,
        tool_name: String,
    },
    ToolAutoAuthorized {
        tool_call_id: Uuid,
    },
    ToolApprovalRequired {
        tool_call_id: Uuid,
    },
    ToolStarted {
        tool_call_id: Uuid,
    },
    ToolOutputChunk {
        tool_call_id: Uuid,
        chunk: String,
    },
    ToolCompleted {
        tool_call_id: Uuid,
    },
    ToolFailed {
        tool_call_id: Uuid,
        error_code: String,
    },

    // User-reviewed Linux command activity
    CommandProposed {
        command_id: Uuid,
        command: String,
        reason: String,
        risk: String,
        mutability: String,
    },
    CommandApprovalRequired {
        approval_id: Uuid,
        command_id: Uuid,
    },
    CommandStarted {
        command_id: Uuid,
    },
    CommandCompleted {
        command_id: Uuid,
        exit_code: Option<u32>,
        /// Redacted, bounded preview for the command card. This is not the
        /// Terminal stream and must never contain an unbounded transcript.
        #[serde(default)]
        output_preview: String,
        duration_ms: u64,
    },
    CommandFailed {
        command_id: Uuid,
        exit_code: Option<u32>,
        #[serde(default)]
        output_preview: String,
        error_code: String,
        duration_ms: u64,
    },
    CommandAnalysisUpdated {
        command_id: Uuid,
        summary: String,
    },

    // Observations / facts
    ObservationAdded {
        summary: String,
    },
    FactsUpdated {
        fact_keys: Vec<String>,
    },

    // User input interrupt
    UserInputRequired {
        question: String,
    },
    UserInputReceived,

    // Approval outcomes
    ApprovalGranted {
        approval_id: Uuid,
    },
    ApprovalRejected {
        approval_id: Uuid,
    },
    ApprovalInvalidated {
        approval_id: Uuid,
        reason_code: String,
    },

    // Diagnosis
    DiagnosisUpdated {
        summary: String,
    },
    RootCauseIdentified {
        summary: String,
    },

    // ChangeSet
    ChangeSetProposed {
        change_set_id: Uuid,
    },
    ChangeSetApproved {
        change_set_id: Uuid,
        version: u64,
    },
    ChangeSetExecutionStarted {
        change_set_id: Uuid,
    },
    ChangeSetExecutionCompleted {
        change_set_id: Uuid,
        success: bool,
    },

    // Verification / rollback
    VerificationStarted,
    VerificationCompleted {
        success: bool,
    },
    RollbackStarted,
    RollbackCompleted {
        success: bool,
    },
}

impl AgentEvent {
    /// Stable snake_case event type name; identical to the serde `type` tag.
    pub fn event_type(&self) -> &'static str {
        match self {
            AgentEvent::RunCreated => "run_created",
            AgentEvent::RunStarted => "run_started",
            AgentEvent::RunResumed => "run_resumed",
            AgentEvent::RunPaused => "run_paused",
            AgentEvent::RunCancelled => "run_cancelled",
            AgentEvent::RunCompleted => "run_completed",
            AgentEvent::RunFailed { .. } => "run_failed",
            AgentEvent::UserMessageAdded { .. } => "user_message_added",
            AgentEvent::AssistantMessageAdded { .. } => "assistant_message_added",
            AgentEvent::ReasoningStarted => "reasoning_started",
            AgentEvent::ProgressUpdated { .. } => "progress_updated",
            AgentEvent::ToolRequested { .. } => "tool_requested",
            AgentEvent::ToolAutoAuthorized { .. } => "tool_auto_authorized",
            AgentEvent::ToolApprovalRequired { .. } => "tool_approval_required",
            AgentEvent::ToolStarted { .. } => "tool_started",
            AgentEvent::ToolOutputChunk { .. } => "tool_output_chunk",
            AgentEvent::ToolCompleted { .. } => "tool_completed",
            AgentEvent::ToolFailed { .. } => "tool_failed",
            AgentEvent::CommandProposed { .. } => "command_proposed",
            AgentEvent::CommandApprovalRequired { .. } => "command_approval_required",
            AgentEvent::CommandStarted { .. } => "command_started",
            AgentEvent::CommandCompleted { .. } => "command_completed",
            AgentEvent::CommandFailed { .. } => "command_failed",
            AgentEvent::CommandAnalysisUpdated { .. } => "command_analysis_updated",
            AgentEvent::ObservationAdded { .. } => "observation_added",
            AgentEvent::FactsUpdated { .. } => "facts_updated",
            AgentEvent::UserInputRequired { .. } => "user_input_required",
            AgentEvent::UserInputReceived => "user_input_received",
            AgentEvent::ApprovalGranted { .. } => "approval_granted",
            AgentEvent::ApprovalRejected { .. } => "approval_rejected",
            AgentEvent::ApprovalInvalidated { .. } => "approval_invalidated",
            AgentEvent::DiagnosisUpdated { .. } => "diagnosis_updated",
            AgentEvent::RootCauseIdentified { .. } => "root_cause_identified",
            AgentEvent::ChangeSetProposed { .. } => "change_set_proposed",
            AgentEvent::ChangeSetApproved { .. } => "change_set_approved",
            AgentEvent::ChangeSetExecutionStarted { .. } => "change_set_execution_started",
            AgentEvent::ChangeSetExecutionCompleted { .. } => "change_set_execution_completed",
            AgentEvent::VerificationStarted => "verification_started",
            AgentEvent::VerificationCompleted { .. } => "verification_completed",
            AgentEvent::RollbackStarted => "rollback_started",
            AgentEvent::RollbackCompleted { .. } => "rollback_completed",
        }
    }
}

/// Ordered, persistable event record: `seq` is monotonic per run and is the
/// replay/streaming ordering key. Never rely on `timestamp_epoch_ms` for
/// ordering — wall clocks are not monotonic.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEventEnvelope {
    pub run_id: Uuid,
    pub seq: u64,
    pub timestamp_epoch_ms: u64,
    pub event: AgentEvent,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_events() -> Vec<AgentEvent> {
        let id = Uuid::new_v4();
        vec![
            AgentEvent::RunCreated,
            AgentEvent::RunStarted,
            AgentEvent::RunResumed,
            AgentEvent::RunPaused,
            AgentEvent::RunCancelled,
            AgentEvent::RunCompleted,
            AgentEvent::RunFailed {
                error_code: "AGENT_BUDGET_EXCEEDED".into(),
            },
            AgentEvent::UserMessageAdded {
                content: "Check disk usage and diagnose issues.".into(),
            },
            AgentEvent::AssistantMessageAdded {
                content: "Root filesystem usage is high.".into(),
            },
            AgentEvent::ReasoningStarted,
            AgentEvent::ProgressUpdated {
                summary: "Checking nginx service logs".into(),
            },
            AgentEvent::ToolRequested {
                tool_call_id: id,
                tool_name: "system.disk_usage".into(),
            },
            AgentEvent::ToolAutoAuthorized { tool_call_id: id },
            AgentEvent::ToolApprovalRequired { tool_call_id: id },
            AgentEvent::ToolStarted { tool_call_id: id },
            AgentEvent::ToolOutputChunk {
                tool_call_id: id,
                chunk: "/dev/sda2 94%".into(),
            },
            AgentEvent::ToolCompleted { tool_call_id: id },
            AgentEvent::ToolFailed {
                tool_call_id: id,
                error_code: "EXEC_TIMED_OUT".into(),
            },
            AgentEvent::CommandProposed {
                command_id: id,
                command: "df -h".into(),
                reason: "Inspect filesystem usage".into(),
                risk: "low".into(),
                mutability: "read".into(),
            },
            AgentEvent::CommandApprovalRequired {
                approval_id: id,
                command_id: id,
            },
            AgentEvent::CommandStarted { command_id: id },
            AgentEvent::CommandCompleted {
                command_id: id,
                exit_code: Some(0),
                output_preview: "/dev/sda2 12%".into(),
                duration_ms: 10,
            },
            AgentEvent::CommandFailed {
                command_id: id,
                exit_code: Some(1),
                output_preview: "permission denied".into(),
                error_code: "COMMAND_EXIT_NON_ZERO".into(),
                duration_ms: 10,
            },
            AgentEvent::CommandAnalysisUpdated {
                command_id: id,
                summary: "The configured Nginx log directory does not exist.".into(),
            },
            AgentEvent::ObservationAdded {
                summary: "/var/log/nginx does not exist.".into(),
            },
            AgentEvent::FactsUpdated {
                fact_keys: vec!["disk.root.usage".into()],
            },
            AgentEvent::UserInputRequired {
                question: "Which site should I diagnose?".into(),
            },
            AgentEvent::UserInputReceived,
            AgentEvent::ApprovalGranted { approval_id: id },
            AgentEvent::ApprovalRejected { approval_id: id },
            AgentEvent::ApprovalInvalidated {
                approval_id: id,
                reason_code: "PRECONDITION_CHANGED".into(),
            },
            AgentEvent::DiagnosisUpdated {
                summary: "Docker container logs consume most of /var.".into(),
            },
            AgentEvent::RootCauseIdentified {
                summary: "Unrotated container logs filled the root filesystem.".into(),
            },
            AgentEvent::ChangeSetProposed { change_set_id: id },
            AgentEvent::ChangeSetApproved {
                change_set_id: id,
                version: 3,
            },
            AgentEvent::ChangeSetExecutionStarted { change_set_id: id },
            AgentEvent::ChangeSetExecutionCompleted {
                change_set_id: id,
                success: true,
            },
            AgentEvent::VerificationStarted,
            AgentEvent::VerificationCompleted { success: true },
            AgentEvent::RollbackStarted,
            AgentEvent::RollbackCompleted { success: false },
        ]
    }

    #[test]
    fn the_contract_defines_all_event_types() {
        let samples = sample_events();
        assert_eq!(samples.len(), 41);
        let mut names: Vec<&'static str> = samples.iter().map(AgentEvent::event_type).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 41, "event type names must be unique");
    }

    #[test]
    fn every_event_round_trips_through_serde() {
        for event in sample_events() {
            let json = serde_json::to_string(&event).expect("event serializes");
            let back: AgentEvent = serde_json::from_str(&json).expect("event deserializes");
            assert_eq!(back, event);
        }
    }

    #[test]
    fn serde_tag_matches_the_stable_event_type_name() {
        for event in sample_events() {
            let value = serde_json::to_value(&event).expect("event serializes");
            let tag = value
                .get("type")
                .and_then(|item| item.as_str())
                .expect("serialized event carries a type tag");
            assert_eq!(tag, event.event_type());
        }
    }

    #[test]
    fn tagged_payload_shape_is_stable() {
        let event = AgentEvent::ProgressUpdated {
            summary: "Checking nginx service logs".into(),
        };
        let json = serde_json::to_string(&event).expect("event serializes");
        assert_eq!(
            json,
            "{\"type\":\"progress_updated\",\"payload\":{\"summary\":\"Checking nginx service logs\"}}"
        );
        // Unit variants carry the tag only.
        let unit = serde_json::to_string(&AgentEvent::RunStarted).expect("event serializes");
        assert_eq!(unit, "{\"type\":\"run_started\"}");
    }

    #[test]
    fn envelope_round_trips_and_uses_stable_field_names() {
        let envelope = AgentEventEnvelope {
            run_id: Uuid::new_v4(),
            seq: 7,
            timestamp_epoch_ms: 1_725_000_000_000,
            event: AgentEvent::ObservationAdded {
                summary: "/ is 94% full".into(),
            },
        };
        let json = serde_json::to_string(&envelope).expect("envelope serializes");
        let back: AgentEventEnvelope = serde_json::from_str(&json).expect("envelope deserializes");
        assert_eq!(back, envelope);

        let value = serde_json::to_value(&envelope).expect("envelope serializes");
        let object = value.as_object().expect("envelope is a json object");
        for key in ["runId", "seq", "timestampEpochMs", "event"] {
            assert!(object.contains_key(key), "missing stable field {key}");
        }
    }

    #[test]
    fn the_event_protocol_carries_no_private_reasoning_fields() {
        // Contract guard: hidden model reasoning must never appear in the
        // serialized protocol. Only user-visible summaries are allowed.
        let forbidden = [
            "chain_of_thought",
            "reasoning_content",
            "scratchpad",
            "internal_reasoning",
            "hidden_reasoning",
        ];
        for event in sample_events() {
            let json = serde_json::to_string(&event).expect("event serializes");
            for key in forbidden {
                assert!(
                    !json.contains(key),
                    "event {} must not expose {key}",
                    event.event_type()
                );
            }
        }
    }

    #[test]
    fn command_result_events_carry_only_the_bounded_preview_contract() {
        let json = serde_json::to_string(&AgentEvent::CommandCompleted {
            command_id: Uuid::new_v4(),
            exit_code: None,
            output_preview: "safe preview".into(),
            duration_ms: 42,
        })
        .expect("event serializes");
        assert!(!json.contains("stdout"));
        assert!(!json.contains("stderr"));
        assert!(json.contains("output_preview"));
    }
}
