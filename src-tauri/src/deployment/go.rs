use std::time::Duration;

use crate::domain::{
    AppError, AppResult, GoProjectConfig, GoRuntime, OperationResult, PythonServiceAction,
    SessionId,
};
use crate::ssh::{RemoteCommand, ServerSessionManager};

use super::go_unit::{render_go_unit, resolve_executable_path};
use super::go_validation::{
    go_service_name, validate_go_bin, validate_go_config, validate_go_path,
};
use super::service::{ensure_operation, join_outputs, DeploymentService};
use super::systemd_install::INSTALL_UNIT_SCRIPT;
use super::validation::validate_app_name;

/// Exit 90 = missing Go toolchain/systemd, 91 = invalid account or path. The optional build runs
/// as the SSH user in the project directory; a missing or non-executable binary is reported on
/// stderr so a failed build never installs a service that cannot start.
const PREPARE_SCRIPT: &str = r#"set -e
command -v systemctl >/dev/null 2>&1 || exit 90
test -d /run/systemd/system || exit 90
id "$6" >/dev/null 2>&1 || exit 91
test -d "$1" || exit 91
if test "$3" = 1; then
  if test "$2" = go; then go=$(command -v go 2>/dev/null) || exit 90; else go="$2"; fi
  test -x "$go" || exit 90
  export PATH="$(dirname "$go"):$PATH"
  mkdir -p "$(dirname "$5")"
  cd "$1"
  "$go" build -o "$5" "$4"
fi
if ! test -f "$5"; then printf 'EXECUTABLE_MISSING %s\n' "$5" >&2; exit 1; fi
if ! test -x "$5"; then chmod u+x "$5" 2>/dev/null || true; fi
if ! test -x "$5"; then printf 'EXECUTABLE_NOT_EXECUTABLE %s\n' "$5" >&2; exit 1; fi
printf 'GO_READY %s\n' "$5"
"#;

impl DeploymentService {
    pub async fn go_control(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        name: &str,
        action: PythonServiceAction,
    ) -> AppResult<OperationResult> {
        let service = go_service_name(name)?;
        let operation = match action {
            PythonServiceAction::Start => "go-start",
            PythonServiceAction::Stop => "go-stop",
            PythonServiceAction::Restart => "go-restart",
        };
        self.systemd_control(sessions, session_id, service, action, operation)
            .await
    }

    pub async fn go_provision(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        name: String,
        project_path: String,
        go: GoProjectConfig,
    ) -> AppResult<OperationResult> {
        validate_app_name(&name)?;
        validate_go_path(&project_path)?;
        validate_go_config(&go)?;
        let service = go_service_name(&name)?;
        let executable = resolve_executable_path(&project_path, &go.executable);
        let unit = render_go_unit(&name, &project_path, &go, &service)?;
        let profile_id = sessions.profile_id(session_id).await?;

        let result = async {
            let prepare = ensure_operation(
                sessions
                    .exec(
                        session_id,
                        RemoteCommand::script(
                            PREPARE_SCRIPT,
                            vec![
                                project_path.clone(),
                                go.go_bin.clone(),
                                if go.build_enabled { "1" } else { "0" }.into(),
                                go.build_package.clone(),
                                executable,
                                go.run_user.clone(),
                            ],
                        )
                        .with_timeout(Duration::from_secs(1800)),
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
                            INSTALL_UNIT_SCRIPT,
                            vec![
                                project_path.clone(),
                                service.clone(),
                                if go.auto_start { "1" } else { "0" }.into(),
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

        self.finish(profile_id, "go-provision", project_path, result)
            .await
    }

    pub async fn list_go_runtimes(
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<Vec<GoRuntime>> {
        let result = sessions
            .exec(
                session_id,
                RemoteCommand::script(
                    r#"found=0
seen=""
d=$(command -v go 2>/dev/null || true)
defreal=""
if test -n "$d"; then defreal=$(readlink -f "$d" 2>/dev/null || true); fi
for cand in "$d" /usr/local/go/bin/go /usr/lib/go*/bin/go "$HOME"/sdk/go*/bin/go /opt/go*/bin/go /usr/local/go*/bin/go; do
  test -n "$cand" && test -x "$cand" || continue
  real=$(readlink -f "$cand" 2>/dev/null || printf '%s' "$cand")
  case "$seen" in *"|$real|"*) continue ;; esac
  seen="$seen|$real|"
  ver=$("$real" version 2>/dev/null | sed -n '1s/^go version go\([^ ]*\).*/\1/p')
  isdef=0
  if test "$real" = "$defreal"; then isdef=1; fi
  printf '%s\t%s\t%s\n' "$real" "$ver" "$isdef"
  found=1
done
if test "$found" = "0"; then exit 90; fi
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
        Ok(parse_go_runtimes(&result.stdout))
    }
}

fn parse_go_runtimes(output: &str) -> Vec<GoRuntime> {
    let mut runtimes: Vec<GoRuntime> = Vec::new();
    for line in output.lines() {
        let mut parts = line.split('\t');
        let binary = parts.next().unwrap_or("").trim();
        let version = parts.next().unwrap_or("").trim();
        let is_default = parts.next().is_some_and(|flag| flag.trim() == "1");
        if binary == "go"
            || validate_go_bin(binary).is_err()
            || runtimes.iter().any(|item| item.binary == binary)
        {
            continue;
        }
        runtimes.push(GoRuntime {
            binary: binary.to_string(),
            version: version.chars().take(32).collect(),
            is_default,
        });
        if runtimes.len() >= 16 {
            break;
        }
    }
    runtimes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_go_runtime_lines_and_skips_unsafe_paths() {
        let runtimes = parse_go_runtimes(
            "/usr/local/go/bin/go\t1.22.1\t1\n/home/a/sdk/go1.21.0/bin/go\t1.21.0\t0\n/usr/local/go/bin/go\t1.22.1\t1\n/opt/my go/bin/go\t1.20\t0\n/usr/bin/python3\t3\t0\ngo\t1.22\t0\n",
        );
        assert_eq!(runtimes.len(), 2);
        assert!(runtimes[0].is_default);
        assert_eq!(runtimes[1].version, "1.21.0");
        assert!(!runtimes[1].is_default);
    }
}
