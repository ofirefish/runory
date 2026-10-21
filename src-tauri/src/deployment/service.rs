use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::Mutex;
use uuid::Uuid;

use crate::deployment::format::{cron_kind, parse_cron, render_cron, serialize_environment};
use crate::deployment::validation::{
    validate_branch, validate_cron_task, validate_domain, validate_email,
    validate_environment_path, validate_remote_path, validate_remote_url, validate_restart,
};
use crate::deployment::DeploymentHistoryRepository;
use crate::domain::{
    AppError, AppResult, BackupRequest, BuildPreset, CronEntry, CronSchedule, CronTask,
    DeploymentRecord, EnvironmentEntry, OperationResult, RestartTarget, SessionId,
};
use crate::ssh::{RemoteCommand, RemoteExecResult, ServerSessionManager};

pub struct DeploymentService {
    history: DeploymentHistoryRepository,
    write_lock: Arc<Mutex<()>>,
}

impl DeploymentService {
    pub fn new(history: DeploymentHistoryRepository) -> Self {
        Self {
            history,
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    pub async fn git_setup(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        repository_path: String,
        remote_url: String,
        branch: String,
    ) -> AppResult<OperationResult> {
        validate_remote_path(&repository_path)?;
        validate_remote_url(&remote_url)?;
        validate_branch(&branch)?;
        let profile_id = sessions.profile_id(session_id).await?;
        let script = r#"set -e
command -v git >/dev/null 2>&1 || exit 90
parent=$(dirname -- "$1")
mkdir -p -- "$parent" || exit 93
if test -d "$1"; then test -r "$1" && test -x "$1" || exit 93; fi
if test -d "$1/.git"; then
  git -C "$1" remote set-url origin "$2"
  git -C "$1" fetch --prune origin "$3"
  git -C "$1" checkout -B "$3" "origin/$3" 2>/dev/null || git -C "$1" checkout "$3"
elif test ! -e "$1" || { test -d "$1" && test -z "$(ls -A "$1" 2>/dev/null || true)"; }; then
  git clone --branch "$3" --single-branch -- "$2" "$1"
else
  exit 92
fi
printf 'GIT_OK %s %s\n' "$1" "$3"
"#;
        let result = sessions
            .exec(
                session_id,
                RemoteCommand::script(script, vec![repository_path.clone(), remote_url, branch])
                    .with_timeout(Duration::from_secs(180)),
            )
            .await
            .map(git_setup_result)
            .and_then(|value| value);
        self.finish(profile_id, "git-setup", repository_path, result)
            .await
    }

    pub async fn deploy(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        repository_path: String,
        branch: String,
        build: BuildPreset,
        restart: RestartTarget,
        requirements_path: Option<String>,
    ) -> AppResult<OperationResult> {
        validate_remote_path(&repository_path)?;
        validate_branch(&branch)?;
        validate_restart(&restart)?;
        if let Some(path) = requirements_path.as_deref() {
            if path.starts_with('/') {
                validate_remote_path(path)?;
            } else if !path.is_empty() {
                crate::deployment::validation::validate_relative_path(path)?;
            }
        }
        let profile_id = sessions.profile_id(session_id).await?;
        let pull = RemoteCommand::script("command -v git >/dev/null 2>&1 || exit 90; test -d \"$1/.git\" || exit 91; git -C \"$1\" fetch --prune origin \"$2\" && git -C \"$1\" checkout \"$2\" && git -C \"$1\" pull --ff-only origin \"$2\"", vec![repository_path.clone(), branch]).with_timeout(Duration::from_secs(180));
        let result = async {
            let first = ensure_operation(sessions.exec(session_id, pull).await?)?;
            if !first.success {
                return Ok(first);
            }
            let mut outputs = vec![first];
            if let Some(command) =
                build_command(build, &repository_path, requirements_path.as_deref())
            {
                let value = ensure_operation(sessions.exec(session_id, command).await?)?;
                if !value.success {
                    return Ok(value);
                }
                outputs.push(value);
            }
            if let Some(command) = restart_command(&restart, &repository_path) {
                let value = ensure_operation(sessions.exec(session_id, command).await?)?;
                if !value.success {
                    return Ok(value);
                }
                outputs.push(value);
            }
            Ok(join_outputs(outputs))
        }
        .await;
        self.finish(profile_id, "deploy", repository_path, result)
            .await
    }

    pub async fn write_environment(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        path: String,
        entries: Vec<EnvironmentEntry>,
    ) -> AppResult<OperationResult> {
        validate_environment_path(&path)?;
        let content = serialize_environment(entries)?;
        let profile_id = sessions.profile_id(session_id).await?;
        let command = RemoteCommand::script("set -e; umask 077; tmp=\"$1.runory.tmp\"; cat > \"$tmp\"; chmod 600 \"$tmp\"; mv -f \"$tmp\" \"$1\"", vec![path.clone()]).with_stdin(content)?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map(ensure_operation)
            .and_then(|value| value);
        self.finish(profile_id, "environment-update", path, result)
            .await
    }

    pub async fn ssl_inspect(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        domain: String,
    ) -> AppResult<OperationResult> {
        validate_domain(&domain)?;
        let script = "command -v openssl >/dev/null 2>&1 || exit 90; printf '' | openssl s_client -servername \"$1\" -connect \"$1:443\" 2>/dev/null | openssl x509 -noout -subject -issuer -dates";
        let result = sessions
            .exec(session_id, RemoteCommand::script(script, vec![domain]))
            .await?;
        ensure_operation(result)
    }

    pub async fn ssl_issue(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        domain: String,
        email: String,
        webroot: String,
    ) -> AppResult<OperationResult> {
        validate_domain(&domain)?;
        validate_email(&email)?;
        validate_remote_path(&webroot)?;
        let profile_id = sessions.profile_id(session_id).await?;
        let script = "command -v certbot >/dev/null 2>&1 || exit 90; certbot certonly --non-interactive --agree-tos --webroot -w \"$3\" -d \"$1\" --email \"$2\"";
        let result = sessions
            .exec(
                session_id,
                RemoteCommand::script(script, vec![domain.clone(), email, webroot])
                    .with_timeout(Duration::from_secs(300)),
            )
            .await
            .map(ensure_operation)
            .and_then(|value| value);
        self.finish(profile_id, "ssl-issue", domain, result).await
    }

    pub async fn backup(
        &self,
        sessions: &ServerSessionManager,
        request: BackupRequest,
    ) -> AppResult<OperationResult> {
        validate_remote_path(&request.source_path)?;
        validate_remote_path(&request.destination_directory)?;
        let profile_id = sessions.profile_id(request.session_id).await?;
        let target = request.source_path.clone();
        let script = r#"set -e; test -e "$1" || exit 91; mkdir -p "$2"; stamp=$(date -u +%Y%m%dT%H%M%SZ); file="$2/runory-backup-$stamp.tar.gz"; tar -czf "$file" -- "$1"; printf '%s\n' "$file""#;
        let result = sessions
            .exec(
                request.session_id,
                RemoteCommand::script(
                    script,
                    vec![request.source_path, request.destination_directory],
                )
                .with_timeout(Duration::from_secs(300)),
            )
            .await
            .map(ensure_operation)
            .and_then(|value| value);
        self.finish(profile_id, "backup", target, result).await
    }

    pub async fn cron_list(
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<Vec<CronEntry>> {
        let result = sessions
            .exec(
                session_id,
                RemoteCommand::script(
                    "command -v crontab >/dev/null 2>&1 || exit 90; crontab -l 2>/dev/null | grep '# runory:' || true",
                    Vec::new(),
                ),
            )
            .await?;
        parse_cron(&result.stdout)
    }

    pub async fn cron_add(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        schedule: CronSchedule,
        task: CronTask,
    ) -> AppResult<CronEntry> {
        validate_cron_task(&task)?;
        let entry = CronEntry {
            id: Uuid::new_v4(),
            schedule,
            task_kind: cron_kind(&task).into(),
        };
        let line = render_cron(&entry, &task)?;
        let command = RemoteCommand::script(
            "command -v crontab >/dev/null 2>&1 || exit 90; set -e; (crontab -l 2>/dev/null || true; cat) | crontab -",
            Vec::new(),
        )
        .with_stdin(line.into_bytes())?;
        let result = ensure_operation(sessions.exec(session_id, command).await?)?;
        if !result.success {
            return Err(AppError::ExecFailed);
        }
        Ok(entry)
    }

    pub async fn cron_remove(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        cron_id: Uuid,
    ) -> AppResult<OperationResult> {
        let script = "command -v crontab >/dev/null 2>&1 || exit 90; set -e; tmp=$(mktemp); crontab -l 2>/dev/null | grep -v \"# runory:$1:\" > \"$tmp\" || true; crontab \"$tmp\"; rm -f \"$tmp\"";
        ensure_operation(
            sessions
                .exec(
                    session_id,
                    RemoteCommand::script(script, vec![cron_id.to_string()]),
                )
                .await?,
        )
    }

    pub async fn history(&self, profile_id: Option<Uuid>) -> AppResult<Vec<DeploymentRecord>> {
        let mut records = self.history.list().await?;
        if let Some(profile_id) = profile_id {
            records.retain(|record| record.profile_id == profile_id);
        }
        records.sort_by_key(|record| std::cmp::Reverse(record.started_at_epoch_seconds));
        records.truncate(200);
        Ok(records)
    }

    pub(crate) async fn finish(
        &self,
        profile_id: Uuid,
        operation: &str,
        target: String,
        result: AppResult<OperationResult>,
    ) -> AppResult<OperationResult> {
        let record = DeploymentRecord {
            id: Uuid::new_v4(),
            profile_id,
            operation: operation.into(),
            target,
            started_at_epoch_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            success: result.as_ref().is_ok_and(|value| value.success),
            error_code: match &result {
                Err(error) => Some(error.code().to_owned()),
                Ok(value) if !value.success => Some(AppError::ExecFailed.code().to_owned()),
                Ok(_) => None,
            },
        };
        if let Err(error) = self.append_history(record).await {
            tracing::warn!(
                error_code = error.code(),
                "could not persist deployment history"
            );
        }
        result
    }

    async fn append_history(&self, record: DeploymentRecord) -> AppResult<()> {
        let _guard = self.write_lock.lock().await;
        let mut records = self.history.list().await?;
        records.push(record);
        if records.len() > 1_000 {
            records.drain(..records.len() - 1_000);
        }
        self.history.save(&records).await
    }
}

pub(crate) fn build_command(
    preset: BuildPreset,
    path: &str,
    requirements_path: Option<&str>,
) -> Option<RemoteCommand> {
    let script = match preset {
        BuildPreset::None => return None,
        BuildPreset::Npm => {
            "command -v npm >/dev/null 2>&1 || exit 90; cd \"$1\" && npm ci && npm run build"
        }
        BuildPreset::Pnpm => {
            "command -v pnpm >/dev/null 2>&1 || exit 90; cd \"$1\" && pnpm install --frozen-lockfile && pnpm build"
        }
        BuildPreset::Cargo => {
            "command -v cargo >/dev/null 2>&1 || exit 90; cd \"$1\" && cargo build --release --locked"
        }
        BuildPreset::Pip => {
            r#"set -e
command -v python3 >/dev/null 2>&1 || exit 90
cd "$1"
req="${2:-requirements.txt}"
case "$req" in
  /*) ;;
  *) req="$1/$req" ;;
esac
test -f "$req" || exit 91
if ! test -d .venv; then python3 -m venv .venv; fi
.venv/bin/pip install --upgrade pip
.venv/bin/pip install -r "$req"
"#
        }
    };
    let args = match preset {
        BuildPreset::Pip => vec![
            path.into(),
            requirements_path.unwrap_or("requirements.txt").into(),
        ],
        _ => vec![path.into()],
    };
    Some(RemoteCommand::script(script, args).with_timeout(Duration::from_secs(600)))
}

fn restart_command(target: &RestartTarget, path: &str) -> Option<RemoteCommand> {
    match target {
        RestartTarget::None => None,
        RestartTarget::Systemd { service } => Some(RemoteCommand::script(
            "command -v systemctl >/dev/null 2>&1 || exit 90; systemctl restart \"$1\"",
            vec![service.clone()],
        )),
        RestartTarget::Pm2 { process } => Some(RemoteCommand::script(
            "command -v pm2 >/dev/null 2>&1 || exit 90; pm2 restart \"$1\"",
            vec![process.clone()],
        )),
        RestartTarget::DockerCompose { service } => Some(RemoteCommand::script(
            "command -v docker >/dev/null 2>&1 || exit 90; cd \"$1\" && docker compose restart \"$2\"",
            vec![path.into(), service.clone()],
        )),
    }
}

pub(crate) fn ensure_operation(result: RemoteExecResult) -> AppResult<OperationResult> {
    if result.exit_code == 90 {
        return Err(AppError::UnsupportedRemote);
    }
    if result.exit_code == 91 {
        return Err(AppError::InvalidOperation);
    }
    let output = [result.stdout.trim(), result.stderr.trim()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(OperationResult {
        success: result.exit_code == 0,
        output,
    })
}

fn git_setup_result(result: RemoteExecResult) -> AppResult<OperationResult> {
    match result.exit_code {
        92 => Err(AppError::DeploymentGitTargetNotEmpty),
        93 => Err(AppError::DeploymentGitPathDenied),
        _ => ensure_operation(result),
    }
}

pub(crate) fn join_outputs(values: Vec<OperationResult>) -> OperationResult {
    OperationResult {
        success: values.iter().all(|value| value.success),
        output: values
            .into_iter()
            .map(|value| value.output)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

#[cfg(test)]
mod result_tests {
    use super::*;

    #[test]
    fn git_setup_distinguishes_directory_errors_from_invalid_configuration() {
        for (exit_code, code) in [
            (92, "DEPLOYMENT_GIT_TARGET_NOT_EMPTY"),
            (93, "DEPLOYMENT_GIT_PATH_DENIED"),
            (90, "UNSUPPORTED_REMOTE"),
        ] {
            let result = git_setup_result(RemoteExecResult {
                stdout: String::new(),
                stderr: String::new(),
                exit_code,
            });
            assert_eq!(result.unwrap_err().code(), code);
        }
        assert!(
            !git_setup_result(RemoteExecResult {
                stdout: String::new(),
                stderr: "checkout failed".into(),
                exit_code: 1
            })
            .unwrap()
            .success
        );
    }

    #[test]
    fn failed_install_is_not_reported_as_successful_deployment() {
        let result = join_outputs(vec![
            OperationResult {
                success: true,
                output: "prepared".into(),
            },
            OperationResult {
                success: false,
                output: "UNIT_PENDING".into(),
            },
        ]);
        assert!(!result.success);
        assert_eq!(result.output, "prepared\nUNIT_PENDING");
    }
}
