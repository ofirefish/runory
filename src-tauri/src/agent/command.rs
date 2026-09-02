//! Runtime V2 approved-command execution boundary.
//!
//! The model never calls this service. `AgentController` first persists an
//! exact command proposal and approval binding; only the granted path reaches
//! this service. Execution writes the approved command plus Enter into the
//! already-visible interactive PTY. Rust captures a bounded copy of the same
//! output as untrusted evidence; React never executes the command.

use std::time::Instant;

use tokio::sync::watch;
use uuid::Uuid;

use super::decision::PreparedCommandProposal;
use crate::agentic::context::redact_secrets;
use crate::domain::SessionId;
use crate::ssh::ServerSessionManager;

pub const COMMAND_EXIT_NON_ZERO: &str = "COMMAND_EXIT_NON_ZERO";
pub const COMMAND_CANCELLED: &str = "COMMAND_CANCELLED";
pub const COMMAND_TARGET_MISMATCH: &str = "COMMAND_TARGET_MISMATCH";
const OBSERVATION_TEXT_LIMIT: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub struct CommandOutcome {
    pub command_id: Uuid,
    pub success: bool,
    pub exit_code: Option<u32>,
    pub stdout: String,
    pub stderr: String,
    pub error_code: Option<String>,
    pub duration_ms: u64,
    pub cancelled: bool,
}

impl CommandOutcome {
    pub fn failure(command_id: Uuid, error_code: impl Into<String>) -> Self {
        Self {
            command_id,
            success: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            error_code: Some(error_code.into()),
            duration_ms: 0,
            cancelled: false,
        }
    }
}

pub struct AgentCommandExecutionService<'a> {
    sessions: &'a ServerSessionManager,
}

impl<'a> AgentCommandExecutionService<'a> {
    pub fn new(sessions: &'a ServerSessionManager) -> Self {
        Self { sessions }
    }

    pub async fn execute(
        &self,
        session_id: SessionId,
        target_id: Uuid,
        command: &PreparedCommandProposal,
        mut cancellation: watch::Receiver<bool>,
    ) -> CommandOutcome {
        if *cancellation.borrow() {
            let mut outcome = CommandOutcome::failure(command.command_id, COMMAND_CANCELLED);
            outcome.cancelled = true;
            return outcome;
        }
        match self.sessions.profile_id(session_id).await {
            Ok(bound_target) if bound_target == target_id => {}
            _ => return CommandOutcome::failure(command.command_id, COMMAND_TARGET_MISMATCH),
        }

        let started = Instant::now();
        let result = tokio::select! {
            result = self.sessions.execute_terminal_command(session_id, &command.command) => Some(result),
            changed = cancellation.changed() => {
                let _ = changed;
                None
            }
        };
        let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        let Some(result) = result else {
            let mut outcome = CommandOutcome::failure(command.command_id, COMMAND_CANCELLED);
            outcome.duration_ms = duration_ms;
            outcome.cancelled = true;
            return outcome;
        };
        match result {
            Ok(result) => {
                let (output, _) = redact_secrets(&result.output);
                let timed_out = result.timed_out;
                let success = !timed_out && !looks_like_shell_failure(&output);
                CommandOutcome {
                    command_id: command.command_id,
                    success,
                    // An interactive shell channel does not expose an SSH
                    // exit-status message for each entered line.
                    exit_code: None,
                    stdout: bounded(&output),
                    stderr: String::new(),
                    error_code: if timed_out {
                        Some("COMMAND_TERMINAL_TIMEOUT".into())
                    } else {
                        (!success).then(|| COMMAND_EXIT_NON_ZERO.into())
                    },
                    duration_ms,
                    cancelled: false,
                }
            }
            Err(error) => {
                let mut outcome = CommandOutcome::failure(command.command_id, error.code());
                outcome.duration_ms = duration_ms;
                outcome
            }
        }
    }
}

fn bounded(value: &str) -> String {
    value.chars().take(OBSERVATION_TEXT_LIMIT).collect()
}

fn looks_like_shell_failure(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "no such file or directory",
        "command not found",
        "permission denied",
        "operation not permitted",
        "syntax error",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_failures_are_content_free() {
        let outcome = CommandOutcome::failure(Uuid::new_v4(), COMMAND_TARGET_MISMATCH);
        assert!(outcome.stdout.is_empty());
        assert!(outcome.stderr.is_empty());
        assert_eq!(outcome.error_code.as_deref(), Some(COMMAND_TARGET_MISMATCH));
    }

    #[test]
    fn common_shell_errors_are_reported_as_failed_execution() {
        assert!(looks_like_shell_failure(
            "find: '/var/log/nginx': No such file or directory"
        ));
        assert!(!looks_like_shell_failure("/dev/sda2 100G 94G 6G 94% /"));
    }
}
