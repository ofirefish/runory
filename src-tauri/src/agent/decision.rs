//! Runtime V2 `AgentDecision` protocol and structural validation (AR2-B).
//!
//! The Reasoner (model or heuristic) proposes decisions; Rust owns the
//! authority. Model-declared risk / safety / approval claims are never
//! trusted: validation resolves every tool name against the closed
//! `NativeToolRegistry`, requires `Mutability::Read` from the Rust-side
//! descriptor, and re-parses arguments through
//! `NativeToolInvocation::from_model_read_call`, which structurally cannot
//! produce a write invocation. AR2-B deliberately has no
//! `ProposeChangeSet` variant — write proposals arrive with the ChangeSet
//! integration stage and keep flowing through `ChangeSetService`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::agentic::context::redact_secrets;
use crate::agentic::ChangeStepDraft;
use crate::tools::{native_descriptors, Mutability, NativeToolInvocation, NativeToolName};

/// Stable error code for structurally rejected Reasoner decisions.
pub const AGENT_DECISION_INVALID: &str = "AGENT_DECISION_INVALID";

/// Upper bound of tool calls a single reasoning turn may request.
pub const MAX_TOOL_CALLS_PER_TURN: usize = 4;

const MAX_REASON_SUMMARY_BYTES: usize = 300;
const MAX_QUESTION_BYTES: usize = 2 * 1024;
const MAX_FINAL_SUMMARY_BYTES: usize = 16 * 1024;

const MAX_CHANGE_TITLE_BYTES: usize = 256;
const MAX_CHANGE_SUMMARY_BYTES: usize = 4_096;
const MAX_CHANGE_STEPS: usize = 4;

/// Raw decision as proposed by a Reasoner. Untrusted until validated.
///
/// Serialization uses the same stable `type`/`payload` snake_case shape as
/// the event contract, so a future model protocol can emit it directly.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum AgentDecision {
    ToolCalls(Vec<ToolCallRequest>),
    CommandProposal(CommandProposalRequest),
    AskUser { question: String },
    Final { summary: String },
    ProposeChangeSet(ChangeProposalRequest),
}

/// One shell command proposed by the Reasoner. The model supplies only the
/// exact command and a concise user-visible purpose; Rust derives risk and
/// mutability and the controller always interrupts for explicit approval.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CommandProposalRequest {
    pub command: String,
    pub reason_summary: String,
    /// Concise interpretation of the preceding command result, when this is
    /// not the first reasoning round. It is user-visible and never contains
    /// private model reasoning or a raw output dump.
    #[serde(default)]
    pub observation_analysis: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandRisk {
    Low,
    Medium,
    High,
    Critical,
}

impl CommandRisk {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandMutability {
    ReadIntent,
    Mutating,
    Unknown,
}

impl CommandMutability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadIntent => "read",
            Self::Mutating => "mutating",
            Self::Unknown => "unknown",
        }
    }
}

/// Evidence-bound remediation proposal. Execution still requires ChangeSet
/// approval — this only drafts the proposal.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChangeProposalRequest {
    pub title: String,
    pub summary: String,
    pub evidence_ids: Vec<Uuid>,
    pub steps: Vec<ChangeStepDraft>,
}

/// One proposed tool call. `reason_summary` is a user-visible progress
/// summary ("Checking nginx service logs"), never model chain-of-thought.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ToolCallRequest {
    pub tool_name: String,
    pub arguments: Value,
    pub reason_summary: String,
}

/// Decision that passed structural validation and may enter the loop.
#[derive(Clone, Debug)]
pub enum ValidatedDecision {
    ToolCalls(Vec<PreparedToolCall>),
    CommandProposal(PreparedCommandProposal),
    AskUser { question: String },
    Final { summary: String },
    ProposeChangeSet(ValidatedChangeProposal),
}

/// A bounded command proposal ready to be displayed and bound to approval.
/// This is not a ToolCall and cannot execute without the controller's
/// approval interrupt.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PreparedCommandProposal {
    pub command_id: Uuid,
    pub command: String,
    pub reason_summary: String,
    #[serde(default)]
    pub observation_analysis: Option<String>,
    pub risk: CommandRisk,
    pub mutability: CommandMutability,
}

/// A structurally validated change proposal ready for ChangeSet drafting.
#[derive(Clone, Debug)]
pub struct ValidatedChangeProposal {
    pub proposal_id: Uuid,
    pub title: String,
    pub summary: String,
    pub evidence_ids: Vec<Uuid>,
    pub steps: Vec<ChangeStepDraft>,
}

/// A validated, dispatch-ready read tool call. The typed invocation is
/// crate-internal so external callers cannot fabricate one around validation.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct PreparedToolCall {
    pub tool_call_id: Uuid,
    /// Stable registry name, e.g. `system.disk_usage`.
    pub tool_name: String,
    /// Redacted, bounded, user-visible progress summary.
    pub reason_summary: String,
    pub(crate) invocation: NativeToolInvocation,
}

/// Structural rejection reasons. Carries no model-controlled text, so the
/// error is safe for events and logs; the stable code is uniform because the
/// runtime treats every rejection the same way (observation + retry).
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum DecisionError {
    #[error("decision contained no tool calls")]
    EmptyToolCalls,
    #[error("decision requested more than {MAX_TOOL_CALLS_PER_TURN} tool calls")]
    TooManyToolCalls,
    #[error("tool name is not in the native tool registry")]
    UnknownTool,
    #[error("write tools cannot be requested by the reasoner")]
    WriteToolRejected,
    #[error("tool arguments failed structural validation")]
    InvalidArguments,
    #[error("command proposal is empty, multiline, or exceeds the allowed size")]
    InvalidCommand,
    #[error("reason summary is empty or exceeds the allowed size")]
    InvalidReasonSummary,
    #[error("question is empty or exceeds the allowed size")]
    InvalidQuestion,
    #[error("final summary is empty or exceeds the allowed size")]
    InvalidFinalSummary,
    #[error("change proposal summary is empty or exceeds the allowed size")]
    InvalidChangeSummary,
    #[error("change proposal title is empty or exceeds the allowed size")]
    InvalidChangeTitle,
    #[error("change proposal has no steps or exceeds the allowed size")]
    InvalidChangeSteps,
    #[error("change proposal evidence list is invalid")]
    InvalidChangeEvidence,
}

impl DecisionError {
    pub fn code(&self) -> &'static str {
        AGENT_DECISION_INVALID
    }
}

/// Validates a raw Reasoner decision into a dispatch-ready one.
///
/// Read-only enforcement is double-layered on purpose: the Rust descriptor
/// must declare `Mutability::Read`, and `from_model_read_call` (which has no
/// write arm) must accept the arguments. Either layer alone would suffice;
/// together a registry regression cannot silently open a write path.
pub fn validate_decision(decision: AgentDecision) -> Result<ValidatedDecision, DecisionError> {
    match decision {
        AgentDecision::ToolCalls(requests) => {
            if requests.is_empty() {
                return Err(DecisionError::EmptyToolCalls);
            }
            if requests.len() > MAX_TOOL_CALLS_PER_TURN {
                return Err(DecisionError::TooManyToolCalls);
            }
            let mut prepared = Vec::with_capacity(requests.len());
            for request in requests {
                prepared.push(prepare_tool_call(request)?);
            }
            Ok(ValidatedDecision::ToolCalls(prepared))
        }
        AgentDecision::CommandProposal(proposal) => Ok(ValidatedDecision::CommandProposal(
            prepare_command_proposal(proposal)?,
        )),
        AgentDecision::AskUser { question } => {
            let question = bounded_user_text(&question, MAX_QUESTION_BYTES)
                .ok_or(DecisionError::InvalidQuestion)?;
            Ok(ValidatedDecision::AskUser { question })
        }
        AgentDecision::Final { summary } => {
            let summary = bounded_user_text(&summary, MAX_FINAL_SUMMARY_BYTES)
                .ok_or(DecisionError::InvalidFinalSummary)?;
            Ok(ValidatedDecision::Final { summary })
        }
        AgentDecision::ProposeChangeSet(proposal) => Ok(ValidatedDecision::ProposeChangeSet(
            prepare_change_proposal(proposal)?,
        )),
    }
}

const MAX_COMMAND_BYTES: usize = 16 * 1024;

fn prepare_command_proposal(
    proposal: CommandProposalRequest,
) -> Result<PreparedCommandProposal, DecisionError> {
    let command = proposal.command.trim();
    if command.is_empty()
        || command.len() > MAX_COMMAND_BYTES
        || command.contains('\0')
        || command.lines().count() != 1
    {
        return Err(DecisionError::InvalidCommand);
    }
    let (redacted_command, command_contained_secret) = redact_secrets(command);
    if command_contained_secret || redacted_command != command {
        return Err(DecisionError::InvalidCommand);
    }
    let reason_summary = bounded_user_text(&proposal.reason_summary, MAX_REASON_SUMMARY_BYTES)
        .ok_or(DecisionError::InvalidReasonSummary)?;
    let observation_analysis = match proposal.observation_analysis.as_deref() {
        Some(summary) => Some(
            bounded_user_text(summary, MAX_FINAL_SUMMARY_BYTES)
                .ok_or(DecisionError::InvalidFinalSummary)?,
        ),
        None => None,
    };
    let (risk, mutability) = classify_command(command);
    Ok(PreparedCommandProposal {
        command_id: Uuid::new_v4(),
        command: command.to_owned(),
        reason_summary,
        observation_analysis,
        risk,
        mutability,
    })
}

/// Conservative display classification. It is deliberately not an
/// authorization decision: every command still requires approval. Shell is
/// too expressive to prove read-only statically, so ambiguous composition is
/// labelled `Unknown` instead of being presented as safe.
fn classify_command(command: &str) -> (CommandRisk, CommandMutability) {
    let lower = command.to_ascii_lowercase();
    if contains_any(
        &lower,
        &[
            "rm -rf", "mkfs", "wipefs", "dd if=", "shutdown", "poweroff", "reboot", ":(){",
        ],
    ) {
        return (CommandRisk::Critical, CommandMutability::Mutating);
    }
    if contains_any(
        &lower,
        &[
            "sudo ",
            " rm ",
            " mv ",
            " cp ",
            " chmod ",
            " chown ",
            " mkdir ",
            " touch ",
            "sed -i",
            "systemctl start",
            "systemctl stop",
            "systemctl restart",
            "systemctl reload",
            "systemctl enable",
            "systemctl disable",
            "docker rm",
            "docker stop",
            "docker restart",
            "docker prune",
            "apt install",
            "apt-get install",
            "dnf install",
            "yum install",
            "pacman -s",
            " tee ",
        ],
    ) || starts_with_any(
        lower.trim_start(),
        &["rm ", "mv ", "cp ", "chmod ", "chown ", "mkdir ", "touch "],
    ) {
        return (CommandRisk::High, CommandMutability::Mutating);
    }

    let without_fd_redirection = lower
        .replace("2>&1", "")
        .replace("1>&2", "")
        .replace("2>/dev/null", "")
        .replace(">/dev/null", "");
    if contains_any(
        &without_fd_redirection,
        &[
            "&&", "||", ";", "$(", "`", " -exec ", " xargs ", "sh -c", "bash -c",
        ],
    ) {
        return (CommandRisk::High, CommandMutability::Unknown);
    }
    if without_fd_redirection.contains('>') {
        return (CommandRisk::Medium, CommandMutability::Mutating);
    }
    if contains_any(&lower, &["curl ", "wget "]) {
        return (CommandRisk::Medium, CommandMutability::Unknown);
    }
    (CommandRisk::Low, CommandMutability::ReadIntent)
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn starts_with_any(value: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| value.starts_with(prefix))
}

fn prepare_change_proposal(
    proposal: ChangeProposalRequest,
) -> Result<ValidatedChangeProposal, DecisionError> {
    let title = bounded_user_text(&proposal.title, MAX_CHANGE_TITLE_BYTES)
        .ok_or(DecisionError::InvalidChangeTitle)?;
    let summary = bounded_user_text(&proposal.summary, MAX_CHANGE_SUMMARY_BYTES)
        .ok_or(DecisionError::InvalidChangeSummary)?;
    if proposal.steps.is_empty() || proposal.steps.len() > MAX_CHANGE_STEPS {
        return Err(DecisionError::InvalidChangeSteps);
    }
    if proposal.evidence_ids.is_empty()
        || proposal.evidence_ids.len() > MAX_CHANGE_STEPS
        || proposal
            .evidence_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != proposal.evidence_ids.len()
    {
        return Err(DecisionError::InvalidChangeEvidence);
    }
    Ok(ValidatedChangeProposal {
        proposal_id: Uuid::new_v4(),
        title,
        summary,
        evidence_ids: proposal.evidence_ids,
        steps: proposal.steps,
    })
}

fn prepare_tool_call(request: ToolCallRequest) -> Result<PreparedToolCall, DecisionError> {
    let name = parse_tool_name(&request.tool_name).ok_or(DecisionError::UnknownTool)?;
    // Authoritative read-only check: the Rust descriptor decides, never the model.
    let descriptor_is_read = native_descriptors()
        .iter()
        .find(|descriptor| descriptor.name == name)
        .map(|descriptor| descriptor.mutability == Mutability::Read)
        .ok_or(DecisionError::UnknownTool)?;
    if !descriptor_is_read {
        return Err(DecisionError::WriteToolRejected);
    }
    let invocation = NativeToolInvocation::from_model_read_call(name, request.arguments)
        .map_err(|_| DecisionError::InvalidArguments)?;
    let reason_summary = bounded_user_text(&request.reason_summary, MAX_REASON_SUMMARY_BYTES)
        .ok_or(DecisionError::InvalidReasonSummary)?;
    Ok(PreparedToolCall {
        tool_call_id: Uuid::new_v4(),
        tool_name: name.as_str().to_owned(),
        reason_summary,
        invocation,
    })
}

fn parse_tool_name(raw: &str) -> Option<NativeToolName> {
    NativeToolName::ALL
        .into_iter()
        .find(|name| name.as_str() == raw)
}

/// Trims, bounds and redacts user-visible text from the model.
fn bounded_user_text(raw: &str, max_bytes: usize) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > max_bytes {
        return None;
    }
    let (redacted, _) = redact_secrets(trimmed);
    Some(redacted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool_call(tool_name: &str, arguments: Value) -> AgentDecision {
        AgentDecision::ToolCalls(vec![ToolCallRequest {
            tool_name: tool_name.into(),
            arguments,
            reason_summary: "Collecting read-only evidence".into(),
        }])
    }

    #[test]
    fn read_tool_with_valid_arguments_is_prepared() {
        let validated = validate_decision(tool_call("system.disk_usage", json!({})))
            .expect("read tool call validates");
        let ValidatedDecision::ToolCalls(calls) = validated else {
            panic!("expected tool calls");
        };
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool_name, "system.disk_usage");
        assert!(!calls[0].reason_summary.is_empty());
    }

    #[test]
    fn write_tools_are_rejected_even_if_the_model_claims_they_are_safe() {
        for write_tool in [
            "service.restart",
            "service.reload",
            "nginx.reload",
            "docker.restart",
            "file.patch",
        ] {
            let error = validate_decision(tool_call(write_tool, json!({})))
                .expect_err("write tool must be rejected");
            assert_eq!(error, DecisionError::WriteToolRejected);
            assert_eq!(error.code(), AGENT_DECISION_INVALID);
        }
    }

    #[test]
    fn unknown_tools_are_rejected() {
        let error = validate_decision(tool_call("shell.exec", json!({})))
            .expect_err("unknown tool must be rejected");
        assert_eq!(error, DecisionError::UnknownTool);
    }

    #[test]
    fn invalid_arguments_are_rejected() {
        let error = validate_decision(tool_call("system.disk_usage", json!({"unexpected": true})))
            .expect_err("unexpected arguments must be rejected");
        assert_eq!(error, DecisionError::InvalidArguments);

        let error = validate_decision(tool_call("service.logs", json!({"service": "x"})))
            .expect_err("missing bounded field must be rejected");
        assert_eq!(error, DecisionError::InvalidArguments);
    }

    #[test]
    fn tool_call_batch_size_is_bounded() {
        let request = ToolCallRequest {
            tool_name: "system.disk_usage".into(),
            arguments: json!({}),
            reason_summary: "Checking disks".into(),
        };
        let too_many = AgentDecision::ToolCalls(vec![request; MAX_TOOL_CALLS_PER_TURN + 1]);
        assert_eq!(
            validate_decision(too_many).expect_err("over-limit batch"),
            DecisionError::TooManyToolCalls
        );
        assert_eq!(
            validate_decision(AgentDecision::ToolCalls(Vec::new())).expect_err("empty batch"),
            DecisionError::EmptyToolCalls
        );
    }

    #[test]
    fn ask_user_and_final_are_bounded_and_trimmed() {
        let validated = validate_decision(AgentDecision::AskUser {
            question: "  Which site should I diagnose?  ".into(),
        })
        .expect("question validates");
        let ValidatedDecision::AskUser { question } = validated else {
            panic!("expected ask_user");
        };
        assert_eq!(question, "Which site should I diagnose?");

        assert_eq!(
            validate_decision(AgentDecision::AskUser {
                question: "   ".into()
            })
            .expect_err("blank question"),
            DecisionError::InvalidQuestion
        );
        assert_eq!(
            validate_decision(AgentDecision::Final {
                summary: String::new()
            })
            .expect_err("empty final"),
            DecisionError::InvalidFinalSummary
        );
    }

    #[test]
    fn decision_protocol_round_trips_through_serde() {
        let decision = tool_call("service.logs", json!({"service": "nginx", "lines": 100}));
        let json = serde_json::to_string(&decision).expect("decision serializes");
        assert!(json.contains("\"type\":\"tool_calls\""));
        let back: AgentDecision = serde_json::from_str(&json).expect("decision deserializes");
        assert_eq!(back, decision);
    }

    #[test]
    fn command_proposal_is_bounded_and_classified_by_rust() {
        let validated = validate_decision(AgentDecision::CommandProposal(CommandProposalRequest {
            command: "df -h 2>&1".into(),
            reason_summary: "Inspect filesystem usage".into(),
            observation_analysis: None,
        }))
        .expect("command validates");
        let ValidatedDecision::CommandProposal(command) = validated else {
            panic!("expected command proposal");
        };
        assert_eq!(command.risk, CommandRisk::Low);
        assert_eq!(command.mutability, CommandMutability::ReadIntent);

        let validated = validate_decision(AgentDecision::CommandProposal(CommandProposalRequest {
            command: "systemctl restart nginx".into(),
            reason_summary: "Restart nginx".into(),
            observation_analysis: None,
        }))
        .expect("mutating command remains reviewable");
        let ValidatedDecision::CommandProposal(command) = validated else {
            panic!("expected command proposal");
        };
        assert_eq!(command.risk, CommandRisk::High);
        assert_eq!(command.mutability, CommandMutability::Mutating);

        let validated = validate_decision(AgentDecision::CommandProposal(CommandProposalRequest {
            command: "rm -rf /var/lib/example".into(),
            reason_summary: "Remove data".into(),
            observation_analysis: None,
        }))
        .expect("critical command is classified before policy blocks it");
        let ValidatedDecision::CommandProposal(command) = validated else {
            panic!("expected command proposal");
        };
        assert_eq!(command.risk, CommandRisk::Critical);
        assert_eq!(command.mutability, CommandMutability::Mutating);
    }

    #[test]
    fn command_proposal_rejects_multiline_and_secret_payloads() {
        for command in [
            "df -h\nrm -rf /",
            "curl -H 'Authorization: Bearer secret-token' example.com",
        ] {
            let error = validate_decision(AgentDecision::CommandProposal(CommandProposalRequest {
                command: command.into(),
                reason_summary: "Inspect server".into(),
                observation_analysis: None,
            }))
            .expect_err("unsafe payload must fail structural validation");
            assert_eq!(error, DecisionError::InvalidCommand);
        }
    }
}
