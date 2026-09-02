use crate::agentic::context::redact_secrets;
use crate::domain::{AppError, AppResult, SessionId};
use crate::ssh::{RemoteCommand, ServerSessionManager};

use super::DiagnosticData;

const OUTPUT_LIMIT: usize = 64 * 1024;

const ALLOWED_READONLY_COMMANDS: &[&str] = &[
    "uptime",
    "whoami",
    "id",
    "uname -a",
    "uname -r",
    "uname -m",
    "free -h",
    "free -b",
    "df -h",
    "df -i",
    "lsblk",
    "lsblk -b",
    "cat /proc/loadavg",
    "cat /proc/meminfo",
];

pub(super) fn validate_readonly_command(command: &str) -> AppResult<()> {
    let trimmed = command.trim();
    if trimmed.is_empty() || trimmed.len() > 256 {
        return Err(AppError::InvalidOperation);
    }
    if trimmed.lines().count() != 1
        || trimmed.chars().any(|character| {
            matches!(
                character,
                ';' | '|' | '&' | '$' | '`' | '<' | '>' | '\n' | '\r'
            )
        })
    {
        return Err(AppError::InvalidOperation);
    }
    if ALLOWED_READONLY_COMMANDS
        .iter()
        .any(|allowed| trimmed.eq_ignore_ascii_case(allowed))
    {
        Ok(())
    } else {
        Err(AppError::InvalidOperation)
    }
}

pub(super) async fn exec_readonly(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    command: String,
) -> AppResult<DiagnosticData> {
    validate_readonly_command(&command)?;
    let result = sessions
        .exec(
            session_id,
            RemoteCommand::freeform(command.clone())?.with_output_limit(OUTPUT_LIMIT),
        )
        .await?;
    if result.exit_code != 0 {
        return Err(AppError::ExecFailed);
    }
    let (output, redacted) = redact_secrets(&result.stdout);
    Ok(DiagnosticData {
        category: "terminal-readonly",
        fields: serde_json::json!({
            "command": command,
            "exitCode": result.exit_code,
            "output": output.chars().take(16_384).collect::<String>(),
            "redacted": redacted,
            "truncated": output.len() >= OUTPUT_LIMIT,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_accepts_bounded_readonly_commands() {
        for command in ALLOWED_READONLY_COMMANDS {
            validate_readonly_command(command).expect("allowed command");
        }
    }

    #[test]
    fn allowlist_rejects_shell_metacharacters_and_chaining() {
        for command in ["uptime; rm -rf /", "df -h | tee /tmp/x", "whoami && id"] {
            assert!(validate_readonly_command(command).is_err(), "{command}");
        }
    }

    #[test]
    fn allowlist_rejects_unknown_commands() {
        assert!(validate_readonly_command("curl example.com").is_err());
    }
}
