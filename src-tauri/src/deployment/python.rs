use super::python_unit::render_python_unit;
use std::time::Duration;

use crate::deployment::validation::{
    python_service_name, validate_app_name, validate_python_config, validate_remote_path,
};
use crate::domain::{
    AppError, AppResult, OperationResult, PythonProjectConfig, PythonProtocol, PythonRuntime,
    PythonServiceAction, PythonStartupMethod, SessionId,
};
use crate::ssh::{RemoteCommand, ServerSessionManager};

use super::service::{ensure_operation, join_outputs, DeploymentService};

impl DeploymentService {
    pub async fn python_control(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        name: &str,
        action: PythonServiceAction,
    ) -> AppResult<OperationResult> {
        let service = python_service_name(name)?;
        let operation = match action {
            PythonServiceAction::Start => "python-start",
            PythonServiceAction::Stop => "python-stop",
            PythonServiceAction::Restart => "python-restart",
        };
        self.systemd_control(sessions, session_id, service, action, operation)
            .await
    }

    /// Start/stop/restart one Runory-generated unit and verify the resulting systemd state.
    pub(super) async fn systemd_control(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        service: String,
        action: PythonServiceAction,
        operation: &'static str,
    ) -> AppResult<OperationResult> {
        let profile_id = sessions.profile_id(session_id).await?;
        let action = match action {
            PythonServiceAction::Start => "start",
            PythonServiceAction::Stop => "stop",
            PythonServiceAction::Restart => "restart",
        };
        let result = async {
            ensure_operation(
                sessions
                    .exec(
                        session_id,
                        RemoteCommand::script(
                            r#"set -e
command -v systemctl >/dev/null 2>&1 || exit 90
if test "$(id -u)" = 0; then
  systemctl "$2" "$1"
else
  sudo -n systemctl "$2" "$1"
fi
state=$(systemctl show --property=ActiveState --value "$1")
if test "$2" = stop; then
  test "$state" = inactive || exit 1
else
  test "$state" = active || exit 1
fi
printf '%s\n' "$state"
"#,
                            vec![service.clone(), action.into()],
                        )
                        .with_timeout(Duration::from_secs(120)),
                    )
                    .await?,
            )
        }
        .await;
        self.finish(profile_id, operation, service, result).await
    }

    pub async fn python_provision(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        name: String,
        project_path: String,
        python: PythonProjectConfig,
        venv_path: String,
    ) -> AppResult<OperationResult> {
        validate_app_name(&name)?;
        validate_remote_path(&project_path)?;
        validate_python_config(&python)?;
        let venv = resolve_venv_path(&project_path, &venv_path)?;
        let service = python_service_name(&name)?;
        let profile_id = sessions.profile_id(session_id).await?;
        let requirements = resolve_requirements_path(&project_path, &python.requirements_path);
        let unit = render_python_unit(&name, &project_path, &venv, &python, &service)?;
        let needs_uvicorn = matches!(
            (python.startup, python.protocol),
            (PythonStartupMethod::Gunicorn, PythonProtocol::Asgi)
        );

        let result = async {
            let prepare = ensure_operation(
                sessions
                    .exec(
                        session_id,
                        RemoteCommand::script(
                            r#"set -e
command -v "$2" >/dev/null 2>&1 || exit 90
command -v systemctl >/dev/null 2>&1 || exit 90
test -d /run/systemd/system || exit 90
id "$7" >/dev/null 2>&1 || exit 91
test -d "$1" || exit 91
if ! test -d "$6"; then "$2" -m venv "$6"; fi
"$6/bin/pip" install --upgrade pip
req="$3"
if test -n "$req"; then
  case "$req" in
    /*) ;;
    *) req="$1/$req" ;;
  esac
  test -f "$req" || exit 91
  "$6/bin/pip" install -r "$req"
fi
case "$4" in
  gunicorn) "$6/bin/pip" install 'gunicorn>=21' ;;
  uwsgi) "$6/bin/pip" install 'uwsgi>=2.0' ;;
  command) ;;
  *) exit 91 ;;
esac
if test "$5" = "1"; then "$6/bin/pip" install 'uvicorn>=0.27'; fi
"#,
                            vec![
                                project_path.clone(),
                                python.python_bin.clone(),
                                if python.requirements_path.is_empty() {
                                    String::new()
                                } else {
                                    requirements
                                },
                                startup_label(python.startup).into(),
                                if needs_uvicorn { "1" } else { "0" }.into(),
                                venv.clone(),
                                python.run_user.clone(),
                            ],
                        )
                        .with_timeout(Duration::from_secs(900)),
                    )
                    .await?,
            )?;
            if !prepare.success {
                return Ok(prepare);
            }

            let install = ensure_operation(
                sessions
                    .exec(
                        session_id,
                        RemoteCommand::script(
                            r#"set -e
mkdir -p "$1/.runory"
unit_file="$1/.runory/$2"
cat > "$unit_file"
target="/etc/systemd/system/$2"
if test -w /etc/systemd/system 2>/dev/null; then
  cp "$unit_file" "$target"
elif command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
  sudo -n cp "$unit_file" "$target"
else
  printf 'UNIT_PENDING %s\n' "$unit_file"
  exit 1
fi
if command -v systemctl >/dev/null 2>&1; then
  if test -w /etc/systemd/system 2>/dev/null; then
    systemctl daemon-reload
    if test "$3" = 1; then systemctl enable "$2"; else systemctl disable "$2"; fi
    systemctl restart "$2"
    systemctl is-active --quiet "$2"
  elif command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
    sudo -n systemctl daemon-reload
    if test "$3" = 1; then sudo -n systemctl enable "$2"; else sudo -n systemctl disable "$2"; fi
    sudo -n systemctl restart "$2"
    systemctl is-active --quiet "$2"
  fi
fi
printf 'SERVICE %s\n' "$2"
"#,
                            vec![
                                project_path.clone(),
                                service.clone(),
                                if python.auto_start { "1" } else { "0" }.into(),
                            ],
                        )
                        .with_stdin(unit.into_bytes())?
                        .with_timeout(Duration::from_secs(120)),
                    )
                    .await?,
            )?;
            Ok(join_outputs(vec![prepare, install]))
        }
        .await;

        self.finish(profile_id, "python-provision", project_path, result)
            .await
    }

    pub async fn python_status(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        service: String,
    ) -> AppResult<OperationResult> {
        crate::deployment::validation::validate_identifier(&service)?;
        let result = sessions
            .exec(
                session_id,
                RemoteCommand::script(
                    r#"command -v systemctl >/dev/null 2>&1 || exit 90
load=$(systemctl show --property=LoadState --value "$1") || exit 1
if test "$load" != loaded; then printf 'unknown\n'; exit 0; fi
state=$(systemctl show --property=ActiveState --value "$1") || exit 1
case "$state" in
  active|inactive|failed|activating|deactivating) printf '%s\n' "$state" ;;
  *) printf 'unknown\n' ;;
esac
"#,
                    vec![service],
                ),
            )
            .await?;
        ensure_operation(result)
    }

    pub async fn list_system_users(
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<Vec<String>> {
        let result = sessions
            .exec(
                session_id,
                RemoteCommand::script(
                    r#"set -e
if command -v getent >/dev/null 2>&1; then
  getent passwd
elif test -r /etc/passwd; then
  cat /etc/passwd
else
  exit 90
fi
"#,
                    Vec::new(),
                ),
            )
            .await?;
        if result.exit_code == 90 {
            return Err(AppError::UnsupportedRemote);
        }
        if result.exit_code != 0 {
            return Err(AppError::ExecFailed);
        }
        Ok(parse_system_usernames(&result.stdout))
    }

    pub async fn list_python_runtimes(
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<Vec<PythonRuntime>> {
        let result = sessions
            .exec(
                session_id,
                RemoteCommand::script(
                    r#"set -e
found=0
for bin in python3 python3.13 python3.12 python3.11 python3.10 python3.9 python python3.8; do
  if command -v "$bin" >/dev/null 2>&1; then
    ver=$("$bin" -c 'import sys; print("%d.%d.%d" % sys.version_info[:3])' 2>/dev/null || true)
    printf '%s\t%s\n' "$bin" "$ver"
    found=1
  fi
done
if test "$found" = "0"; then
  exit 90
fi
"#,
                    Vec::new(),
                )
                .with_timeout(Duration::from_secs(30)),
            )
            .await?;
        if result.exit_code == 90 {
            return Ok(Vec::new());
        }
        if result.exit_code != 0 {
            return Err(AppError::ExecFailed);
        }
        Ok(parse_python_runtimes(&result.stdout))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn python_prepare_env(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        project_path: String,
        python_bin: String,
        venv_path: String,
        requirements_path: String,
        install_packages: bool,
        startup: Option<PythonStartupMethod>,
        protocol: Option<PythonProtocol>,
    ) -> AppResult<OperationResult> {
        validate_remote_path(&project_path)?;
        crate::deployment::validation::validate_python_bin(&python_bin)?;
        let venv = if venv_path.trim().is_empty() {
            format!("{}/.venv", project_path.trim_end_matches('/'))
        } else {
            validate_remote_path(&venv_path)?;
            venv_path
        };
        if !requirements_path.is_empty() {
            if requirements_path.starts_with('/') {
                validate_remote_path(&requirements_path)?;
            } else {
                crate::deployment::validation::validate_relative_path(&requirements_path)?;
            }
        }
        let profile_id = sessions.profile_id(session_id).await?;
        let needs_uvicorn = matches!(
            (startup, protocol),
            (
                Some(PythonStartupMethod::Gunicorn),
                Some(PythonProtocol::Asgi)
            )
        );
        let startup_token = startup.map(startup_label).unwrap_or("");
        let install = if install_packages { "1" } else { "0" };
        let result = ensure_operation(
            sessions
                .exec(
                    session_id,
                    RemoteCommand::script(
                        r#"set -e
command -v "$2" >/dev/null 2>&1 || exit 90
test -d "$1" || exit 91
if ! test -d "$3"; then "$2" -m venv "$3"; fi
if test "$6" = "1"; then
  "$3/bin/pip" install --upgrade pip
  req="$4"
  if test -n "$req"; then
    case "$req" in
      /*) ;;
      *) req="$1/$req" ;;
    esac
    test -f "$req" || exit 91
    "$3/bin/pip" install -r "$req"
  fi
  case "$5" in
    gunicorn) "$3/bin/pip" install 'gunicorn>=21' ;;
    uwsgi) "$3/bin/pip" install 'uwsgi>=2.0' ;;
    command|"") ;;
    *) exit 91 ;;
  esac
  if test "$7" = "1"; then "$3/bin/pip" install 'uvicorn>=0.27'; fi
fi
printf 'VENV %s\n' "$3"
"#,
                        vec![
                            project_path.clone(),
                            python_bin,
                            venv,
                            requirements_path,
                            startup_token.into(),
                            install.into(),
                            if needs_uvicorn { "1" } else { "0" }.into(),
                        ],
                    )
                    .with_timeout(Duration::from_secs(900)),
                )
                .await?,
        )?;
        self.finish(profile_id, "python-prepare-env", project_path, Ok(result))
            .await
    }
}

fn parse_system_usernames(passwd: &str) -> Vec<String> {
    let mut users = passwd
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let name = line.split(':').next()?.trim();
            if name.is_empty()
                || name.len() > 32
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            {
                return None;
            }
            // Skip common non-login placeholders that clutter service pickers.
            if matches!(
                name,
                "sync" | "halt" | "shutdown" | "nobody" | "nfsnobody" | "nogroup"
            ) {
                return None;
            }
            Some(name.to_string())
        })
        .collect::<Vec<_>>();
    users.sort();
    users.dedup();
    users.truncate(500);
    users
}

fn parse_python_runtimes(output: &str) -> Vec<PythonRuntime> {
    let mut runtimes = Vec::new();
    for line in output.lines() {
        let mut parts = line.split('\t');
        let binary = parts.next().unwrap_or("").trim();
        let version = parts.next().unwrap_or("").trim();
        if binary.is_empty() || binary.len() > 32 {
            continue;
        }
        if crate::deployment::validation::validate_python_bin(binary).is_err() {
            continue;
        }
        if runtimes
            .iter()
            .any(|runtime: &PythonRuntime| runtime.binary == binary)
        {
            continue;
        }
        runtimes.push(PythonRuntime {
            binary: binary.to_string(),
            version: version.chars().take(32).collect(),
        });
        if runtimes.len() >= 32 {
            break;
        }
    }
    runtimes
}

fn startup_label(method: PythonStartupMethod) -> &'static str {
    match method {
        PythonStartupMethod::Command => "command",
        PythonStartupMethod::Uwsgi => "uwsgi",
        PythonStartupMethod::Gunicorn => "gunicorn",
    }
}

fn resolve_requirements_path(project_path: &str, requirements: &str) -> String {
    if requirements.starts_with('/') {
        requirements.to_string()
    } else {
        format!(
            "{}/{}",
            project_path.trim_end_matches('/'),
            requirements.trim_start_matches('/')
        )
    }
}

fn resolve_venv_path(project_path: &str, venv_path: &str) -> AppResult<String> {
    let trimmed = venv_path.trim();
    if trimmed.is_empty() {
        return Ok(format!("{}/.venv", project_path.trim_end_matches('/')));
    }
    validate_remote_path(trimmed)?;
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires docker compose OpenSSH fixture (without Python/systemd)"]
    async fn python_deployment_fails_closed_on_unsupported_openssh_host() {
        use crate::domain::{ConnectRequest, SshAuthentication, SshConnectionRequest};
        use crate::ssh::SshService;
        use crate::storage::JsonRepository;
        let fingerprint = SshService::scan_host_key("127.0.0.1", 2222)
            .await
            .unwrap()
            .fingerprint;
        let sessions = ServerSessionManager::default();
        let profile_id = uuid::Uuid::new_v4();
        let session_id = sessions
            .connect(
                profile_id,
                ConnectRequest {
                    connection: SshConnectionRequest {
                        host: "127.0.0.1".into(),
                        port: 2222,
                        username: "runory".into(),
                        authentication: SshAuthentication::Password {
                            password: zeroize::Zeroizing::new("runory-spike".into()),
                        },
                    },
                    cols: 80,
                    rows: 24,
                },
                fingerprint,
                tauri::ipc::Channel::new(|_| Ok(())),
            )
            .await
            .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let service = DeploymentService::new(crate::deployment::DeploymentHistoryRepository::new(
            JsonRepository::new(temp.path().join("history.json")),
        ));
        let config: PythonProjectConfig = serde_json::from_str(r#"{"port":8000,"pythonBin":"python3","startup":"gunicorn","entryFile":"app.py","protocol":"wsgi","application":"app:app","runUser":"runory","requirementsPath":""}"#).unwrap();
        let result = service
            .python_provision(
                &sessions,
                session_id,
                "test-unsupported".into(),
                "/home/runory".into(),
                config,
                String::new(),
            )
            .await;
        assert!(matches!(result, Err(AppError::UnsupportedRemote)));
        let history = service.history(Some(profile_id)).await.unwrap();
        assert_eq!(history.len(), 1);
        assert!(!history[0].success);
        assert_eq!(history[0].error_code.as_deref(), Some("UNSUPPORTED_REMOTE"));
        sessions.disconnect(session_id).await.unwrap();
    }

    #[test]
    fn parses_python_runtime_lines() {
        let runtimes =
            parse_python_runtimes("python3\t3.12.3\npython3.12\t3.12.3\nnot-python\t1\n");
        assert_eq!(runtimes.len(), 2);
        assert_eq!(runtimes[0].binary, "python3");
        assert_eq!(runtimes[0].version, "3.12.3");
    }

    #[test]
    fn parses_passwd_usernames() {
        let users = parse_system_usernames(
            "root:x:0:0:root:/root:/bin/bash\nwww-data:x:33:33:www-data:/var/www:/usr/sbin/nologin\nsync:x:4:65534:sync:/bin:/bin/sync\n",
        );
        assert_eq!(users, vec!["root".to_string(), "www-data".to_string()]);
    }

    #[test]
    fn renders_gunicorn_asgi_unit() {
        let python = PythonProjectConfig {
            port: 8000,
            python_bin: "python3".into(),
            startup: PythonStartupMethod::Gunicorn,
            entry_file: "main.py".into(),
            protocol: PythonProtocol::Asgi,
            application: "main:app".into(),
            run_user: "www".into(),
            requirements_path: "requirements.txt".into(),
            startup_command: String::new(),
            processes: 1,
            threads: 1,
            auto_start: true,
        };
        let unit = render_python_unit(
            "demo",
            "/srv/demo",
            "/srv/demo/.venv",
            &python,
            "runory-py-demo.service",
        )
        .unwrap();
        assert!(unit.contains("gunicorn\" -k uvicorn.workers.UvicornWorker"));
        assert!(unit.contains("--workers 1 --threads 1"));
        assert!(unit.contains("127.0.0.1:8000"));
        assert!(unit.contains("main:app"));
        assert!(unit.contains("User=www"));
    }

    #[test]
    fn renders_command_startup_unit() {
        let python = PythonProjectConfig {
            port: 0,
            python_bin: "python3".into(),
            startup: PythonStartupMethod::Command,
            entry_file: String::new(),
            protocol: PythonProtocol::Wsgi,
            application: String::new(),
            run_user: "www".into(),
            requirements_path: String::new(),
            startup_command: "app.py --bind 127.0.0.1:9000".into(),
            processes: 1,
            threads: 1,
            auto_start: true,
        };
        let unit = render_python_unit(
            "cli",
            "/srv/cli",
            "/srv/cli/.venv",
            &python,
            "runory-py-cli.service",
        )
        .unwrap();
        assert!(unit.contains("WorkingDirectory=\"/srv/cli\""));
        assert!(unit.contains(
            "ExecStart=\"/srv/cli/.venv/bin/python\" \"app.py\" \"--bind\" \"127.0.0.1:9000\""
        ));
    }

    #[test]
    fn renders_unit_with_custom_venv_path() {
        let python = PythonProjectConfig {
            port: 8000,
            python_bin: "python3".into(),
            startup: PythonStartupMethod::Gunicorn,
            entry_file: "main.py".into(),
            protocol: PythonProtocol::Wsgi,
            application: "app:app".into(),
            run_user: "www".into(),
            requirements_path: "requirements.txt".into(),
            startup_command: String::new(),
            processes: 1,
            threads: 1,
            auto_start: true,
        };
        let unit = render_python_unit(
            "demo",
            "/srv/demo",
            "/opt/venvs/demo",
            &python,
            "runory-py-demo.service",
        )
        .unwrap();
        assert!(unit.contains("ExecStart=\"/opt/venvs/demo/bin/gunicorn\""));
        assert!(unit.contains("Environment=\"PATH=/opt/venvs/demo/bin:/usr/bin:/bin\""));
        assert!(!unit.contains("/srv/demo/.venv"));
    }

    #[test]
    fn resolve_venv_path_defaults_to_project_dot_venv() {
        assert_eq!(
            resolve_venv_path("/srv/demo", "").unwrap(),
            "/srv/demo/.venv"
        );
        assert_eq!(
            resolve_venv_path("/srv/demo/", "  ").unwrap(),
            "/srv/demo/.venv"
        );
        assert_eq!(
            resolve_venv_path("/srv/demo", "/opt/venvs/demo").unwrap(),
            "/opt/venvs/demo"
        );
    }
}
