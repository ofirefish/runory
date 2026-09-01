use serde_json::Value;

use crate::domain::{
    AppError, AppResult, DockerContainer, LogSource, NginxAction, OperationResult, Pm2Process,
    ResourceAction, SessionId,
};
use crate::ssh::{RemoteCommand, RemoteExecResult, ServerSessionManager};

const DOCKER_LIST: &str =
    "command -v docker >/dev/null 2>&1 || exit 90; docker ps -a --no-trunc --format '{{json .}}'";
const PM2_LIST: &str = "command -v pm2 >/dev/null 2>&1 || exit 90; pm2 jlist";
const NGINX_TEST: &str = "command -v nginx >/dev/null 2>&1 || exit 90; nginx -t";
const NGINX_RELOAD: &str = "command -v nginx >/dev/null 2>&1 || exit 90; nginx -s reload";

pub struct OperationsService;

impl OperationsService {
    pub async fn docker_list(
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<Vec<DockerContainer>> {
        let result = sessions
            .exec(session_id, RemoteCommand::script(DOCKER_LIST, Vec::new()))
            .await?;
        ensure_supported(&result)?;
        result
            .stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(2_000)
            .map(parse_docker_container)
            .collect()
    }

    pub async fn docker_action(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        container: String,
        action: ResourceAction,
    ) -> AppResult<OperationResult> {
        validate_target(&container)?;
        let verb = action_verb(action);
        let script = "command -v docker >/dev/null 2>&1 || exit 90; docker \"$1\" \"$2\"";
        run_operation(
            sessions,
            session_id,
            RemoteCommand::script(script, vec![verb.into(), container]),
        )
        .await
    }

    pub async fn pm2_list(
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<Vec<Pm2Process>> {
        let result = sessions
            .exec(session_id, RemoteCommand::script(PM2_LIST, Vec::new()))
            .await?;
        ensure_supported(&result)?;
        parse_pm2(&result.stdout)
    }

    pub async fn pm2_action(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        process: String,
        action: ResourceAction,
    ) -> AppResult<OperationResult> {
        validate_target(&process)?;
        let script = "command -v pm2 >/dev/null 2>&1 || exit 90; pm2 \"$1\" \"$2\"";
        run_operation(
            sessions,
            session_id,
            RemoteCommand::script(script, vec![action_verb(action).into(), process]),
        )
        .await
    }

    pub async fn nginx_action(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        action: NginxAction,
    ) -> AppResult<OperationResult> {
        let script = match action {
            NginxAction::Test => NGINX_TEST,
            NginxAction::Reload => NGINX_RELOAD,
        };
        run_operation(
            sessions,
            session_id,
            RemoteCommand::script(script, Vec::new()),
        )
        .await
    }

    pub async fn logs(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        source: LogSource,
        target: Option<String>,
        lines: u32,
    ) -> AppResult<OperationResult> {
        if !(20..=5_000).contains(&lines) {
            return Err(AppError::InvalidOperation);
        }
        let lines = lines.to_string();
        let (script, args) = match source {
            LogSource::System => (
                "if command -v journalctl >/dev/null 2>&1; then journalctl -n \"$1\" --no-pager; elif test -r /var/log/syslog; then tail -n \"$1\" /var/log/syslog; else exit 90; fi",
                vec![lines],
            ),
            LogSource::Auth => (
                "if test -r /var/log/auth.log; then tail -n \"$1\" /var/log/auth.log; elif test -r /var/log/secure; then tail -n \"$1\" /var/log/secure; else exit 90; fi",
                vec![lines],
            ),
            LogSource::NginxAccess => (
                "test -r /var/log/nginx/access.log || exit 90; tail -n \"$1\" /var/log/nginx/access.log",
                vec![lines],
            ),
            LogSource::NginxError => (
                "test -r /var/log/nginx/error.log || exit 90; tail -n \"$1\" /var/log/nginx/error.log",
                vec![lines],
            ),
            LogSource::Docker => {
                let target = required_target(target)?;
                ("command -v docker >/dev/null 2>&1 || exit 90; docker logs --tail \"$1\" \"$2\"", vec![lines, target])
            }
            LogSource::Pm2 => {
                let target = required_target(target)?;
                ("command -v pm2 >/dev/null 2>&1 || exit 90; pm2 logs \"$2\" --lines \"$1\" --nostream", vec![lines, target])
            }
            LogSource::Service => {
                let target = required_target(target)?;
                ("command -v journalctl >/dev/null 2>&1 || exit 90; journalctl -u \"$2\" -n \"$1\" --no-pager", vec![lines, target])
            }
        };
        run_operation(
            sessions,
            session_id,
            RemoteCommand::script(script, args).with_output_limit(1024 * 1024),
        )
        .await
    }
}

async fn run_operation(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    command: RemoteCommand,
) -> AppResult<OperationResult> {
    let result = sessions.exec(session_id, command).await?;
    if result.exit_code == 90 {
        return Err(AppError::UnsupportedRemote);
    }
    Ok(OperationResult {
        success: result.exit_code == 0,
        output: combined_output(result),
    })
}

fn ensure_supported(result: &RemoteExecResult) -> AppResult<()> {
    match result.exit_code {
        0 => Ok(()),
        90 => Err(AppError::UnsupportedRemote),
        _ => Err(AppError::ExecFailed),
    }
}

fn combined_output(result: RemoteExecResult) -> String {
    let stdout = result.stdout.trim();
    let stderr = result.stderr.trim();
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("{stdout}\n{stderr}"),
        (false, true) => stdout.to_owned(),
        (true, false) => stderr.to_owned(),
        (true, true) => String::new(),
    }
}

fn action_verb(action: ResourceAction) -> &'static str {
    match action {
        ResourceAction::Start => "start",
        ResourceAction::Stop => "stop",
        ResourceAction::Restart => "restart",
    }
}

fn required_target(target: Option<String>) -> AppResult<String> {
    let target = target.ok_or(AppError::InvalidOperation)?;
    validate_target(&target)?;
    Ok(target)
}

fn validate_target(target: &str) -> AppResult<()> {
    if target.is_empty()
        || target.len() > 256
        || !target.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'@' | b':' | b'/')
        })
    {
        Err(AppError::InvalidOperation)
    } else {
        Ok(())
    }
}

fn parse_docker_container(line: &str) -> AppResult<DockerContainer> {
    let value: Value = serde_json::from_str(line).map_err(|_| AppError::ExecFailed)?;
    Ok(DockerContainer {
        id: json_string(&value, "ID")?,
        name: json_string(&value, "Names")?,
        image: json_string(&value, "Image")?,
        state: json_string(&value, "State")?,
        status: json_string(&value, "Status")?,
    })
}

fn parse_pm2(output: &str) -> AppResult<Vec<Pm2Process>> {
    let values: Vec<Value> = serde_json::from_str(output).map_err(|_| AppError::ExecFailed)?;
    values
        .into_iter()
        .take(2_000)
        .map(|value| {
            let environment = value.get("pm2_env").ok_or(AppError::ExecFailed)?;
            let monitoring = value.get("monit").ok_or(AppError::ExecFailed)?;
            Ok(Pm2Process {
                id: value
                    .get("pm_id")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or(AppError::ExecFailed)?,
                name: json_string(&value, "name")?,
                status: json_string(environment, "status")?,
                cpu_percent: monitoring.get("cpu").and_then(Value::as_f64).unwrap_or(0.0),
                memory_bytes: monitoring
                    .get("memory")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            })
        })
        .collect()
}

fn json_string(value: &Value, key: &str) -> AppResult<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| value.len() <= 4096)
        .map(ToOwned::to_owned)
        .ok_or(AppError::ExecFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_docker_json_without_shell_columns() {
        let item = parse_docker_container(
            r#"{"ID":"abc","Names":"web","Image":"nginx:latest","State":"running","Status":"Up"}"#,
        )
        .expect("container");
        assert_eq!(item.name, "web");
    }

    #[test]
    fn operation_targets_reject_metacharacters() {
        assert!(validate_target("web-1").is_ok());
        assert!(matches!(
            validate_target("web; reboot"),
            Err(AppError::InvalidOperation)
        ));
    }
}
