use serde_json::{json, Value};

use crate::agentic::context::redact_secrets;
use crate::dashboard::DashboardService;
use crate::domain::{AppError, AppResult, LogSource, SessionId};
use crate::operations::OperationsService;
use crate::ssh::{RemoteCommand, ServerSessionManager};

use super::DiagnosticData;

const DNS_SCRIPT: &str = "command -v getent >/dev/null 2>&1 || exit 90; getent ahosts \"$1\" | awk '{print $1}' | sort -u | head -n 32";
const TLS_SCRIPT: &str = "command -v openssl >/dev/null 2>&1 || exit 90; cert=$(openssl s_client -connect \"$1:$2\" -servername \"$1\" -verify_return_error </dev/null 2>/dev/null | openssl x509 -noout -subject -issuer -dates -fingerprint -sha256 2>/dev/null) || exit 1; test -n \"$cert\" || exit 1; printf '%s\\n' \"$cert\"";
const LISTENERS_SCRIPT: &str =
    "command -v ss >/dev/null 2>&1 || exit 90; ss -lntpH 2>/dev/null | head -n 200";
const FILE_SCRIPT: &str = "test -f \"$1\" || exit 91; test -r \"$1\" || exit 92; stat -Lc 'META\\t%s\\t%a\\t%U\\t%G' \"$1\" || exit 1; printf 'CONTENT\\n'; head -c 262144 \"$1\"";
const DIRECTORY_SCRIPT: &str =
    "test -d \"$1\" || exit 91; du -x -B1 -d1 \"$1\" 2>/dev/null | sort -nr | head -n 100";
const LARGE_FILES_SCRIPT: &str = "test -d \"$1\" || exit 91; find \"$1\" -xdev -type f -size +\"$2\"c -printf '%s\\t%p\\n' 2>/dev/null | sort -nr | head -n 100";
const DOCKER_INSPECT_SCRIPT: &str =
    "command -v docker >/dev/null 2>&1 || exit 90; docker inspect \"$1\"";

pub(super) async fn dns_resolve(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    host: String,
) -> AppResult<DiagnosticData> {
    validate_host(&host)?;
    let output = exec(
        sessions,
        session_id,
        DNS_SCRIPT,
        vec![host.clone()],
        32 * 1024,
    )
    .await?;
    let addresses = output
        .lines()
        .filter(|line| !line.is_empty() && line.len() <= 64)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    Ok(data(
        "dns",
        json!({"host":host,"addresses":addresses,"resolved":!addresses.is_empty()}),
    ))
}

pub(super) async fn tls_inspect(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    host: String,
    port: u16,
) -> AppResult<DiagnosticData> {
    validate_host(&host)?;
    let output = exec(
        sessions,
        session_id,
        TLS_SCRIPT,
        vec![host.clone(), port.to_string()],
        32 * 1024,
    )
    .await?;
    let field = |prefix: &str| {
        output
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .map(str::trim)
            .filter(|value| value.len() <= 4096)
            .map(str::to_owned)
    };
    Ok(data(
        "tls",
        json!({"host":host,"port":port,"subject":field("subject="),"issuer":field("issuer="),"notBefore":field("notBefore="),"notAfter":field("notAfter="),"sha256Fingerprint":field("sha256 Fingerprint=").or_else(|| field("SHA256 Fingerprint=")),"verified":true}),
    ))
}

pub(super) async fn network_listeners(
    sessions: &ServerSessionManager,
    session_id: SessionId,
) -> AppResult<DiagnosticData> {
    let output = exec(
        sessions,
        session_id,
        LISTENERS_SCRIPT,
        Vec::new(),
        256 * 1024,
    )
    .await?;
    Ok(data(
        "network-listeners",
        json!({"listeners": bounded_lines(&output, 200)}),
    ))
}

pub(super) async fn process_list(
    sessions: &ServerSessionManager,
    session_id: SessionId,
) -> AppResult<DiagnosticData> {
    Ok(data(
        "process-list",
        json!({"processes": DashboardService::processes(sessions, session_id).await?}),
    ))
}

pub(super) async fn file_inspect(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    path: String,
) -> AppResult<DiagnosticData> {
    validate_path(&path)?;
    let output = exec(
        sessions,
        session_id,
        FILE_SCRIPT,
        vec![path.clone()],
        320 * 1024,
    )
    .await?;
    let (metadata, content) = output
        .split_once("\nCONTENT\n")
        .ok_or(AppError::ExecFailed)?;
    let fields = metadata.split('\t').collect::<Vec<_>>();
    if fields.len() != 5 || fields[0] != "META" {
        return Err(AppError::ExecFailed);
    }
    let (preview, redacted) = redact_secrets(content);
    Ok(data(
        "file",
        json!({"path":path,"sizeBytes":parse_u64(fields[1])?,"mode":fields[2],"owner":fields[3],"group":fields[4],"contentPreview":preview,"redacted":redacted,"truncated":content.len() >= 262144}),
    ))
}

pub(super) async fn directory_usage(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    path: String,
) -> AppResult<DiagnosticData> {
    validate_path(&path)?;
    let output = exec(
        sessions,
        session_id,
        DIRECTORY_SCRIPT,
        vec![path.clone()],
        128 * 1024,
    )
    .await?;
    Ok(data(
        "directory-usage",
        json!({"path":path,"entries":parse_size_paths(&output)}),
    ))
}

pub(super) async fn large_files(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    path: String,
    minimum_bytes: u64,
) -> AppResult<DiagnosticData> {
    validate_path(&path)?;
    if !(1_048_576..=1_099_511_627_776).contains(&minimum_bytes) {
        return Err(AppError::InvalidOperation);
    }
    let output = exec(
        sessions,
        session_id,
        LARGE_FILES_SCRIPT,
        vec![path.clone(), minimum_bytes.to_string()],
        128 * 1024,
    )
    .await?;
    Ok(data(
        "large-files",
        json!({"path":path,"minimumBytes":minimum_bytes,"files":parse_size_paths(&output)}),
    ))
}

pub(super) async fn docker_list(
    sessions: &ServerSessionManager,
    session_id: SessionId,
) -> AppResult<DiagnosticData> {
    Ok(data(
        "docker-list",
        json!({"containers": OperationsService::docker_list(sessions, session_id).await?}),
    ))
}

pub(super) async fn docker_inspect(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    container: String,
) -> AppResult<DiagnosticData> {
    validate_identifier(&container)?;
    let output = exec(
        sessions,
        session_id,
        DOCKER_INSPECT_SCRIPT,
        vec![container.clone()],
        512 * 1024,
    )
    .await?;
    let values: Value = serde_json::from_str(&output).map_err(|_| AppError::ExecFailed)?;
    let item = values
        .as_array()
        .and_then(|items| items.first())
        .ok_or(AppError::ExecFailed)?;
    Ok(data(
        "docker-inspect",
        json!({
            "container":container,
            "state":item.pointer("/State/Status"), "running":item.pointer("/State/Running"),
            "exitCode":item.pointer("/State/ExitCode"), "error":item.pointer("/State/Error"),
            "restartCount":item.get("RestartCount"), "health":item.pointer("/State/Health/Status"),
            "ports":item.pointer("/NetworkSettings/Ports"), "mounts":item.get("Mounts")
        }),
    ))
}

pub(super) async fn docker_logs(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    container: String,
    lines: u32,
) -> AppResult<DiagnosticData> {
    validate_identifier(&container)?;
    if !(20..=500).contains(&lines) {
        return Err(AppError::InvalidOperation);
    }
    let result = OperationsService::logs(
        sessions,
        session_id,
        LogSource::Docker,
        Some(container.clone()),
        lines,
    )
    .await?;
    if !result.success {
        return Err(AppError::ExecFailed);
    }
    let (logs, redacted) = redact_secrets(&result.output);
    Ok(data(
        "docker-logs",
        json!({"container":container,"entries":bounded_lines(&logs, lines as usize),"redacted":redacted}),
    ))
}

async fn exec(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    script: &'static str,
    args: Vec<String>,
    limit: usize,
) -> AppResult<String> {
    let result = sessions
        .exec(
            session_id,
            RemoteCommand::script(script, args).with_output_limit(limit),
        )
        .await?;
    match result.exit_code {
        0 => Ok(result.stdout),
        90 => Err(AppError::UnsupportedRemote),
        91 | 92 => Err(AppError::SftpNotFound),
        _ => Err(AppError::ExecFailed),
    }
}

fn data(category: &'static str, fields: Value) -> DiagnosticData {
    DiagnosticData { category, fields }
}
fn validate_host(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 255
        || value.starts_with('-')
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        Err(AppError::InvalidOperation)
    } else {
        Ok(())
    }
}
fn validate_identifier(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 256
        || !value.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'@' | b':' | b'/')
        })
    {
        Err(AppError::InvalidOperation)
    } else {
        Ok(())
    }
}
fn validate_path(value: &str) -> AppResult<()> {
    if !value.starts_with('/') || value.len() > 4096 || value.chars().any(char::is_control) {
        Err(AppError::InvalidOperation)
    } else {
        Ok(())
    }
}
fn parse_u64(value: &str) -> AppResult<u64> {
    value.parse().map_err(|_| AppError::ExecFailed)
}
fn bounded_lines(value: &str, limit: usize) -> Vec<String> {
    value
        .lines()
        .take(limit)
        .filter(|line| line.len() <= 8192)
        .map(str::to_owned)
        .collect()
}
fn parse_size_paths(value: &str) -> Vec<Value> {
    value
        .lines()
        .take(100)
        .filter_map(|line| {
            let (size, path) = line.split_once('\t')?;
            Some(json!({"sizeBytes":size.parse::<u64>().ok()?,"path":path}))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inputs_are_closed_to_shell_metacharacters() {
        assert!(validate_host("example.com").is_ok());
        assert!(validate_host("host name").is_err());
        assert!(validate_identifier("web-1").is_ok());
        assert!(validate_identifier("web;id").is_err());
        assert!(validate_path("/var/log").is_ok());
        assert!(validate_path("relative").is_err());
    }
    #[test]
    fn parses_bounded_size_rows() {
        let rows = parse_size_paths("42\t/var/log/a\n");
        assert_eq!(rows[0]["sizeBytes"], 42);
    }
}
