use crate::domain::{AppError, AppResult, LogSource, ServiceHealth, ServiceStatus, SessionId};
use crate::operations::OperationsService;
use crate::ssh::{RemoteCommand, ServerSessionManager};

use super::{ServiceLogsData, ServiceStatusData};

const MAX_LOG_LINES: u32 = 500;
const MAX_LOG_ENTRY_BYTES: usize = 8 * 1024;
const MAX_LOG_RESULT_BYTES: usize = 256 * 1024;
const SERVICE_STATUS_SCRIPT: &str = "command -v systemctl >/dev/null 2>&1 || exit 90; status=$(systemctl is-active \"$1\" 2>/dev/null || true); printf 'SERVICE_STATUS\\t%s\\t%s\\n' \"$1\" \"$status\"";

pub(super) async fn status(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    service: String,
) -> AppResult<ServiceStatusData> {
    validate_service(&service)?;
    let result = sessions
        .exec(
            session_id,
            RemoteCommand::script(SERVICE_STATUS_SCRIPT, vec![service.clone()])
                .with_output_limit(8 * 1024),
        )
        .await?;
    if result.exit_code == 90 {
        return Err(AppError::UnsupportedRemote);
    }
    if result.exit_code != 0 {
        return Err(AppError::ExecFailed);
    }
    Ok(ServiceStatusData {
        service: parse_service_status(&result.stdout, service)?,
    })
}

pub(super) async fn logs(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    service: String,
    lines: u32,
) -> AppResult<(ServiceLogsData, bool)> {
    if !(20..=MAX_LOG_LINES).contains(&lines) {
        return Err(AppError::InvalidOperation);
    }
    validate_service(&service)?;
    let result = OperationsService::logs(
        sessions,
        session_id,
        LogSource::Service,
        Some(service.clone()),
        lines,
    )
    .await?;
    if !result.success {
        return Err(AppError::ExecFailed);
    }
    let (entries, truncated) = bounded_log_entries(&result.output, lines as usize);
    Ok((ServiceLogsData { service, entries }, truncated))
}

fn validate_service(service: &str) -> AppResult<()> {
    if service.is_empty()
        || service.len() > 128
        || !service.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'@' | b':' | b'-')
        })
    {
        return Err(AppError::InvalidOperation);
    }
    Ok(())
}

fn parse_service_status(output: &str, service: String) -> AppResult<ServiceHealth> {
    let fields = output.trim_end().split('\t').collect::<Vec<_>>();
    if fields.len() != 3 || fields[0] != "SERVICE_STATUS" || fields[1] != service {
        return Err(AppError::ExecFailed);
    }
    let status = match fields[2] {
        "active" => ServiceStatus::Active,
        "inactive" => ServiceStatus::Inactive,
        "failed" => ServiceStatus::Failed,
        _ => ServiceStatus::Unknown,
    };
    Ok(ServiceHealth {
        name: service,
        status,
    })
}

fn bounded_log_entries(output: &str, requested_lines: usize) -> (Vec<String>, bool) {
    let mut entries = Vec::new();
    let mut bytes = 0usize;
    let mut truncated = false;
    for line in output.lines() {
        if entries.len() >= requested_lines || bytes >= MAX_LOG_RESULT_BYTES {
            truncated = true;
            break;
        }
        let (line, line_truncated) = truncate_utf8(line, MAX_LOG_ENTRY_BYTES);
        let remaining = MAX_LOG_RESULT_BYTES.saturating_sub(bytes);
        let (line, result_truncated) = truncate_utf8(line, remaining);
        bytes = bytes.saturating_add(line.len());
        entries.push(line.to_owned());
        truncated |= line_truncated || result_truncated;
    }
    (entries, truncated)
}

fn truncate_utf8(value: &str, max_bytes: usize) -> (&str, bool) {
    if value.len() <= max_bytes {
        return (value, false);
    }
    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    (&value[..end], true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_entries_are_line_and_byte_bounded() {
        let oversized = "x".repeat(MAX_LOG_ENTRY_BYTES + 20);
        let input = format!("first\n{oversized}\nthird\n");
        let (entries, truncated) = bounded_log_entries(&input, 2);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].len(), MAX_LOG_ENTRY_BYTES);
        assert!(truncated);
    }

    #[test]
    fn utf8_truncation_preserves_character_boundaries() {
        let (value, truncated) = truncate_utf8("服务日志", 5);
        assert_eq!(value, "服");
        assert!(truncated);
    }

    #[test]
    fn validates_and_parses_service_status_without_shell_interpolation() {
        let parsed = parse_service_status(
            "SERVICE_STATUS\tsshd.service\tactive\n",
            "sshd.service".into(),
        )
        .expect("parse status");
        assert!(matches!(parsed.status, ServiceStatus::Active));
        assert!(validate_service("sshd.service").is_ok());
        assert!(validate_service("sshd; reboot").is_err());
    }
}
