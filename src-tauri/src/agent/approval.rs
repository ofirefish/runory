//! Runtime V2 `ApprovalRequest` — exact action binding for the approval
//! interrupt (AR2-C).
//!
//! An approval authorizes exactly one pending action, identified by the full
//! binding tuple (run, tool call, tool name, canonical arguments hash,
//! targets, risk, sealed policy version + hash). Any drift — mutated
//! arguments, changed targets, changed policy — must invalidate the approval
//! instead of executing something the user never saw. The binding carries no
//! raw arguments and no secrets; the hash is enough to detect drift.

use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::decision::{PreparedCommandProposal, PreparedToolCall};
use super::run::now_epoch_ms;
use crate::domain::SessionId;

/// Stable invalidation reason codes (safe for events, logs and IPC).
pub const APPROVAL_REASON_ARGUMENTS_CHANGED: &str = "APPROVAL_ARGUMENTS_CHANGED";
pub const APPROVAL_REASON_TARGETS_CHANGED: &str = "APPROVAL_TARGETS_CHANGED";
pub const APPROVAL_REASON_POLICY_CHANGED: &str = "APPROVAL_POLICY_CHANGED";
pub const APPROVAL_REASON_SESSION_CHANGED: &str = "APPROVAL_SESSION_CHANGED";
pub const COMMAND_APPROVAL_NAME: &str = "agent.command";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalRequestState {
    Pending,
    Granted,
    Rejected,
    Invalidated,
}

/// The durable approval record. Serialization is the stable snake_case
/// protocol reused by AR2-D persistence.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ApprovalRequest {
    pub id: Uuid,
    pub run_id: Uuid,
    pub tool_call_id: Uuid,
    pub tool_name: String,
    /// SHA-256 over the canonical typed invocation; detects argument drift.
    pub arguments_hash: String,
    pub target_ids: Vec<Uuid>,
    /// Rust-side risk classification (`R0`…`R4`), never the model's claim.
    pub risk: String,
    pub policy_version: u64,
    pub policy_hash: String,
    /// ChangeSet precondition digest where available (write approvals in a
    /// later stage); read tool approvals have none.
    pub precondition_ref: Option<String>,
    #[serde(default)]
    pub change_set_id: Option<Uuid>,
    #[serde(default)]
    pub change_set_version: Option<u64>,
    pub state: ApprovalRequestState,
    pub created_at_epoch_ms: u64,
    pub decided_at_epoch_ms: Option<u64>,
}

impl ApprovalRequest {
    /// Binds a pending approval to the exact validated call.
    pub(crate) fn bind(
        run_id: Uuid,
        call: &PreparedToolCall,
        target_ids: Vec<Uuid>,
        risk: String,
        policy_version: u64,
        policy_hash: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            run_id,
            tool_call_id: call.tool_call_id,
            tool_name: call.tool_name.clone(),
            arguments_hash: arguments_hash(call),
            target_ids,
            risk,
            policy_version,
            policy_hash,
            precondition_ref: None,
            change_set_id: None,
            change_set_version: None,
            state: ApprovalRequestState::Pending,
            created_at_epoch_ms: now_epoch_ms(),
            decided_at_epoch_ms: None,
        }
    }

    pub(crate) fn decide(&mut self, state: ApprovalRequestState) {
        self.state = state;
        self.decided_at_epoch_ms = Some(now_epoch_ms());
    }

    /// Binds approval to the exact command bytes the user reviewed. The
    /// existing durable columns retain their historical names, but for this
    /// action `tool_call_id` is the command id and `arguments_hash` is the
    /// canonical command hash.
    pub(crate) fn bind_command(
        run_id: Uuid,
        command: &PreparedCommandProposal,
        target_ids: Vec<Uuid>,
        session_id: SessionId,
        policy_version: u64,
        policy_hash: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            run_id,
            tool_call_id: command.command_id,
            tool_name: COMMAND_APPROVAL_NAME.into(),
            arguments_hash: command_hash(command),
            target_ids,
            risk: command.risk.as_str().into(),
            policy_version,
            policy_hash,
            precondition_ref: Some(command_session_ref(session_id)),
            change_set_id: None,
            change_set_version: None,
            state: ApprovalRequestState::Pending,
            created_at_epoch_ms: now_epoch_ms(),
            decided_at_epoch_ms: None,
        }
    }

    pub(crate) fn bind_change_set(
        run_id: Uuid,
        proposal_id: Uuid,
        change_set_id: Uuid,
        version: u64,
        title: &str,
        target_ids: Vec<Uuid>,
        risk: String,
        policy_version: u64,
        policy_hash: String,
        precondition_ref: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            run_id,
            tool_call_id: proposal_id,
            tool_name: title.to_owned(),
            arguments_hash: format!("changeset:{change_set_id}:{version}"),
            target_ids,
            risk,
            policy_version,
            policy_hash,
            precondition_ref,
            change_set_id: Some(change_set_id),
            change_set_version: Some(version),
            state: ApprovalRequestState::Pending,
            created_at_epoch_ms: now_epoch_ms(),
            decided_at_epoch_ms: None,
        }
    }

    pub(crate) fn is_change_set(&self) -> bool {
        self.change_set_id.is_some()
    }

    pub(crate) fn is_command(&self) -> bool {
        self.change_set_id.is_none() && self.tool_name == COMMAND_APPROVAL_NAME
    }
}

/// Why a pending approval can no longer authorize execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalInvalidationReason {
    NotPending,
    ToolCallMismatch,
    ArgumentsChanged,
    TargetsChanged,
    SessionChanged,
    PolicyChanged,
}

impl ApprovalInvalidationReason {
    pub fn reason_code(self) -> &'static str {
        match self {
            Self::NotPending => "APPROVAL_NOT_PENDING",
            Self::ToolCallMismatch => APPROVAL_REASON_ARGUMENTS_CHANGED,
            Self::ArgumentsChanged => APPROVAL_REASON_ARGUMENTS_CHANGED,
            Self::TargetsChanged => APPROVAL_REASON_TARGETS_CHANGED,
            Self::SessionChanged => APPROVAL_REASON_SESSION_CHANGED,
            Self::PolicyChanged => APPROVAL_REASON_POLICY_CHANGED,
        }
    }
}

pub fn validate_pending_change_set_approval(
    approval: &ApprovalRequest,
    change_set_id: Uuid,
    version: u64,
    target_ids: &[Uuid],
    policy_matches: bool,
) -> Result<(), ApprovalInvalidationReason> {
    if approval.state != ApprovalRequestState::Pending {
        return Err(ApprovalInvalidationReason::NotPending);
    }
    if approval.change_set_id != Some(change_set_id) || approval.change_set_version != Some(version)
    {
        return Err(ApprovalInvalidationReason::ArgumentsChanged);
    }
    if approval.target_ids != target_ids {
        return Err(ApprovalInvalidationReason::TargetsChanged);
    }
    if !policy_matches {
        return Err(ApprovalInvalidationReason::PolicyChanged);
    }
    Ok(())
}

/// Validates that the pending approval still authorizes the exact action the
/// user saw. Any drift invalidates instead of executing.
pub fn validate_pending_approval(
    approval: &ApprovalRequest,
    call: &PreparedToolCall,
    target_ids: &[Uuid],
    policy_matches: bool,
) -> Result<(), ApprovalInvalidationReason> {
    if approval.state != ApprovalRequestState::Pending {
        return Err(ApprovalInvalidationReason::NotPending);
    }
    if approval.tool_call_id != call.tool_call_id {
        return Err(ApprovalInvalidationReason::ToolCallMismatch);
    }
    if approval.arguments_hash != arguments_hash(call) {
        return Err(ApprovalInvalidationReason::ArgumentsChanged);
    }
    if approval.target_ids != target_ids {
        return Err(ApprovalInvalidationReason::TargetsChanged);
    }
    if !policy_matches {
        return Err(ApprovalInvalidationReason::PolicyChanged);
    }
    Ok(())
}

pub fn validate_pending_command_approval(
    approval: &ApprovalRequest,
    command: &PreparedCommandProposal,
    target_ids: &[Uuid],
    session_id: SessionId,
    policy_matches: bool,
) -> Result<(), ApprovalInvalidationReason> {
    if approval.state != ApprovalRequestState::Pending {
        return Err(ApprovalInvalidationReason::NotPending);
    }
    if !approval.is_command() || approval.tool_call_id != command.command_id {
        return Err(ApprovalInvalidationReason::ToolCallMismatch);
    }
    if approval.arguments_hash != command_hash(command) {
        return Err(ApprovalInvalidationReason::ArgumentsChanged);
    }
    if approval.target_ids != target_ids {
        return Err(ApprovalInvalidationReason::TargetsChanged);
    }
    if approval.precondition_ref.as_deref() != Some(command_session_ref(session_id).as_str()) {
        return Err(ApprovalInvalidationReason::SessionChanged);
    }
    if !policy_matches {
        return Err(ApprovalInvalidationReason::PolicyChanged);
    }
    Ok(())
}

/// Canonical hash of a validated call: SHA-256 over the stable tool name and
/// the typed invocation's deterministic rendering. The typed enum is the
/// canonical form (already validated and bounded), so equal invocations hash
/// equally and any argument change alters the hash.
pub(crate) fn arguments_hash(call: &PreparedToolCall) -> String {
    let canonical = format!("{}\n{:?}", call.tool_name, call.invocation);
    digest(&SHA256, canonical.as_bytes())
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn command_hash(command: &PreparedCommandProposal) -> String {
    digest(&SHA256, command.command.as_bytes())
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn command_session_ref(session_id: SessionId) -> String {
    format!("session:{session_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::decision::ValidatedDecision;
    use crate::agent::decision::{
        validate_decision, AgentDecision, CommandProposalRequest, ToolCallRequest,
    };
    use serde_json::json;

    fn call(tool_name: &str, arguments: serde_json::Value) -> PreparedToolCall {
        let decision = AgentDecision::ToolCalls(vec![ToolCallRequest {
            tool_name: tool_name.into(),
            arguments,
            reason_summary: "Collecting evidence".into(),
        }]);
        match validate_decision(decision).expect("decision validates") {
            ValidatedDecision::ToolCalls(mut calls) => calls.remove(0),
            _ => unreachable!("tool call decision"),
        }
    }

    fn command(value: &str) -> PreparedCommandProposal {
        match validate_decision(AgentDecision::CommandProposal(CommandProposalRequest {
            command: value.into(),
            reason_summary: "Inspect the server".into(),
            observation_analysis: None,
        }))
        .expect("command validates")
        {
            ValidatedDecision::CommandProposal(command) => command,
            _ => unreachable!("command proposal decision"),
        }
    }

    #[test]
    fn arguments_hash_is_deterministic_and_argument_sensitive() {
        let first = call("service.logs", json!({"service": "nginx", "lines": 100}));
        let second = call("service.logs", json!({"service": "nginx", "lines": 100}));
        assert_eq!(
            arguments_hash(&first),
            arguments_hash(&second),
            "equal invocations must hash equally"
        );

        let different = call("service.logs", json!({"service": "nginx", "lines": 200}));
        assert_ne!(
            arguments_hash(&first),
            arguments_hash(&different),
            "changed arguments must change the hash"
        );
    }

    #[test]
    fn binding_captures_the_exact_action_and_serializes_cleanly() {
        let run_id = Uuid::new_v4();
        let target = Uuid::new_v4();
        let bound_call = call("system.disk_usage", json!({}));
        let approval = ApprovalRequest::bind(
            run_id,
            &bound_call,
            vec![target],
            "R1".into(),
            7,
            "abc123".into(),
        );

        assert_eq!(approval.run_id, run_id);
        assert_eq!(approval.tool_call_id, bound_call.tool_call_id);
        assert_eq!(approval.tool_name, "system.disk_usage");
        assert_eq!(approval.target_ids, vec![target]);
        assert_eq!(approval.state, ApprovalRequestState::Pending);
        assert!(approval.decided_at_epoch_ms.is_none());

        let json = serde_json::to_string(&approval).expect("approval serializes");
        let back: ApprovalRequest = serde_json::from_str(&json).expect("approval deserializes");
        assert_eq!(back, approval);
        // The binding never carries raw arguments or secrets.
        assert!(!json.contains("arguments\":"));
    }

    #[test]
    fn validate_pending_approval_rejects_drift() {
        let run_id = Uuid::new_v4();
        let target = Uuid::new_v4();
        let bound_call = call("service.logs", json!({"service": "nginx", "lines": 100}));
        let approval = ApprovalRequest::bind(
            run_id,
            &bound_call,
            vec![target],
            "R1".into(),
            1,
            "hash".into(),
        );
        assert!(validate_pending_approval(&approval, &bound_call, &[target], true).is_ok());

        let mut wrong_targets = approval.clone();
        wrong_targets.target_ids.push(Uuid::new_v4());
        assert_eq!(
            validate_pending_approval(&wrong_targets, &bound_call, &[target], true),
            Err(ApprovalInvalidationReason::TargetsChanged)
        );

        let mut wrong_call = call("service.logs", json!({"service": "nginx", "lines": 200}));
        wrong_call.tool_call_id = bound_call.tool_call_id;
        assert_eq!(
            validate_pending_approval(&approval, &wrong_call, &[target], true),
            Err(ApprovalInvalidationReason::ArgumentsChanged)
        );

        assert_eq!(
            validate_pending_approval(&approval, &bound_call, &[target], false),
            Err(ApprovalInvalidationReason::PolicyChanged)
        );
    }

    #[test]
    fn deciding_an_approval_stamps_the_decision_time() {
        let bound_call = call("system.disk_usage", json!({}));
        let mut approval = ApprovalRequest::bind(
            Uuid::new_v4(),
            &bound_call,
            Vec::new(),
            "R1".into(),
            1,
            "h".into(),
        );
        approval.decide(ApprovalRequestState::Rejected);
        assert_eq!(approval.state, ApprovalRequestState::Rejected);
        assert!(approval.decided_at_epoch_ms.is_some());
    }

    #[test]
    fn command_approval_binds_exact_command_and_target() {
        let run_id = Uuid::new_v4();
        let target = Uuid::new_v4();
        let approved = command("df -h");
        let session_id = Uuid::new_v4();
        let approval = ApprovalRequest::bind_command(
            run_id,
            &approved,
            vec![target],
            session_id,
            7,
            "policy-hash".into(),
        );
        assert!(validate_pending_command_approval(
            &approval,
            &approved,
            &[target],
            session_id,
            true
        )
        .is_ok());

        let mut changed = command("df -i");
        changed.command_id = approved.command_id;
        assert_eq!(
            validate_pending_command_approval(&approval, &changed, &[target], session_id, true),
            Err(ApprovalInvalidationReason::ArgumentsChanged)
        );
        assert_eq!(
            validate_pending_command_approval(
                &approval,
                &approved,
                &[Uuid::new_v4()],
                session_id,
                true
            ),
            Err(ApprovalInvalidationReason::TargetsChanged)
        );
        assert_eq!(
            validate_pending_command_approval(
                &approval,
                &approved,
                &[target],
                Uuid::new_v4(),
                true
            ),
            Err(ApprovalInvalidationReason::SessionChanged)
        );
    }
}
