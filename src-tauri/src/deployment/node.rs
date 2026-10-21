use std::time::Duration;

use crate::domain::{
    AppError, AppResult, NodeProjectConfig, NodeRuntime, NodeStartMode, OperationResult,
    PythonServiceAction, SessionId,
};
use crate::ssh::{RemoteCommand, ServerSessionManager};

use super::node_unit::{package_manager_label, render_node_unit};
use super::node_validation::{
    node_service_name, validate_node_bin, validate_node_config, validate_node_path,
};
use super::service::{ensure_operation, join_outputs, DeploymentService};
use super::systemd_install::INSTALL_UNIT_SCRIPT;
use super::validation::validate_app_name;

/// Exit 90 = missing Node/package manager/systemd, 91 = invalid account, path or mode.
/// Prints `NODE <path>` and, when a package manager is needed, `PM <path>`; a missing script or
/// entry file is reported on stderr. Dependencies install with the SSH user, not the run user.
const PREPARE_SCRIPT: &str = r#"set -e
if test "$2" = node; then node=$(command -v node 2>/dev/null) || exit 90; else node="$2"; fi
test -x "$node" || exit 90
node=$(readlink -f "$node" 2>/dev/null || printf '%s' "$node")
"$node" --version >/dev/null 2>&1 || exit 90
nodedir=$(dirname "$node")
command -v systemctl >/dev/null 2>&1 || exit 90
test -d /run/systemd/system || exit 90
id "$4" >/dev/null 2>&1 || exit 91
test -d "$1" || exit 91
need_pm=0
if test "$5" = 1 || test -n "$6" || test "$7" = script; then need_pm=1; fi
pm=""
if test "$need_pm" = 1; then
  test -f "$1/package.json" || exit 91
  if test -x "$nodedir/$3"; then pm="$nodedir/$3"; else pm=$(command -v "$3" 2>/dev/null) || exit 90; fi
fi
export PATH="$nodedir:$PATH"
cd "$1"
if test "$5" = 1; then
  case "$3" in
    npm) if test -f package-lock.json || test -f npm-shrinkwrap.json; then "$pm" ci; else "$pm" install; fi ;;
    yarn) "$pm" install ;;
    pnpm) if test -f pnpm-lock.yaml; then "$pm" install --frozen-lockfile; else "$pm" install; fi ;;
    *) exit 91 ;;
  esac
fi
if test -n "$6"; then "$pm" run "$6"; fi
case "$7" in
  script)
    "$node" -e 'const s=require(process.argv[1]).scripts||{};process.exit(Object.prototype.hasOwnProperty.call(s,process.argv[2])?0:1)' "$1/package.json" "$8" || { printf 'SCRIPT_MISSING %s\n' "$8" >&2; exit 1; } ;;
  entry)
    test -f "$1/$8" || { printf 'ENTRY_MISSING %s\n' "$8" >&2; exit 1; } ;;
  *) exit 91 ;;
esac
printf 'NODE %s\n' "$node"
if test -n "$pm"; then printf 'PM %s\n' "$pm"; fi
"#;

impl DeploymentService {
    pub async fn node_control(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        name: &str,
        action: PythonServiceAction,
    ) -> AppResult<OperationResult> {
        let service = node_service_name(name)?;
        let operation = match action {
            PythonServiceAction::Start => "node-start",
            PythonServiceAction::Stop => "node-stop",
            PythonServiceAction::Restart => "node-restart",
        };
        self.systemd_control(sessions, session_id, service, action, operation)
            .await
    }

    pub async fn node_provision(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        name: String,
        project_path: String,
        node: NodeProjectConfig,
    ) -> AppResult<OperationResult> {
        validate_app_name(&name)?;
        validate_node_path(&project_path)?;
        validate_node_config(&node)?;
        let service = node_service_name(&name)?;
        let profile_id = sessions.profile_id(session_id).await?;
        let (mode, target) = match node.start_mode {
            NodeStartMode::Script => ("script", node.script.clone()),
            NodeStartMode::Entry => ("entry", node.entry_file.clone()),
        };

        let result = async {
            let prepare = ensure_operation(
                sessions
                    .exec(
                        session_id,
                        RemoteCommand::script(
                            PREPARE_SCRIPT,
                            vec![
                                project_path.clone(),
                                node.node_bin.clone(),
                                package_manager_label(node.package_manager).into(),
                                node.run_user.clone(),
                                if node.install_deps { "1" } else { "0" }.into(),
                                node.build_script.clone(),
                                mode.into(),
                                target,
                            ],
                        )
                        .with_timeout(Duration::from_secs(1800)),
                    )
                    .await?,
            )?;
            if !prepare.success {
                return Ok(prepare);
            }
            let node_exec =
                parse_marker(&prepare.output, "NODE ").ok_or(AppError::UnsupportedRemote)?;
            let manager_exec = parse_marker(&prepare.output, "PM ");
            if node.start_mode == NodeStartMode::Script && manager_exec.is_none() {
                return Err(AppError::UnsupportedRemote);
            }
            let unit = render_node_unit(
                &name,
                &project_path,
                &node_exec,
                manager_exec.as_deref(),
                &node,
                &service,
            )?;

            let install = ensure_operation(
                sessions
                    .exec(
                        session_id,
                        RemoteCommand::script(
                            INSTALL_UNIT_SCRIPT,
                            vec![
                                project_path.clone(),
                                service.clone(),
                                if node.auto_start { "1" } else { "0" }.into(),
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

        self.finish(profile_id, "node-provision", project_path, result)
            .await
    }

    pub async fn list_node_runtimes(
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<Vec<NodeRuntime>> {
        let result = sessions
            .exec(
                session_id,
                RemoteCommand::script(
                    r#"found=0
seen=""
d=$(command -v node 2>/dev/null || true)
defreal=""
if test -n "$d"; then defreal=$(readlink -f "$d" 2>/dev/null || true); fi
for cand in "$d" /usr/local/bin/node /usr/bin/node "$HOME"/.nvm/versions/node/*/bin/node /usr/local/nvm/versions/node/*/bin/node /usr/local/n/versions/node/*/bin/node /opt/node*/bin/node /usr/local/node*/bin/node; do
  test -n "$cand" && test -x "$cand" || continue
  real=$(readlink -f "$cand" 2>/dev/null || printf '%s' "$cand")
  case "$seen" in *"|$real|"*) continue ;; esac
  seen="$seen|$real|"
  ver=$("$real" --version 2>/dev/null | sed -n '1s/^v//p')
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
        Ok(parse_node_runtimes(&result.stdout))
    }
}

/// Extracts `<marker><absolute path>` from script output. The path is spliced into a unit file,
/// so it must be strictly absolute and plain.
fn parse_marker(output: &str, marker: &str) -> Option<String> {
    let path = output
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix(marker))?
        .trim();
    validate_node_path(path).ok()?;
    Some(path.to_string())
}

fn parse_node_runtimes(output: &str) -> Vec<NodeRuntime> {
    let mut runtimes: Vec<NodeRuntime> = Vec::new();
    for line in output.lines() {
        let mut parts = line.split('\t');
        let binary = parts.next().unwrap_or("").trim();
        let version = parts.next().unwrap_or("").trim();
        let is_default = parts.next().is_some_and(|flag| flag.trim() == "1");
        if binary == "node"
            || validate_node_bin(binary).is_err()
            || runtimes.iter().any(|item| item.binary == binary)
        {
            continue;
        }
        runtimes.push(NodeRuntime {
            binary: binary.to_string(),
            version: version.chars().take(32).collect(),
            is_default,
        });
        if runtimes.len() >= 32 {
            break;
        }
    }
    runtimes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_node_runtime_lines_and_skips_unsafe_paths() {
        let runtimes = parse_node_runtimes(
            "/usr/bin/node\t20.11.0\t1\n/home/a/.nvm/versions/node/v18.19.0/bin/node\t18.19.0\t0\n/usr/bin/node\t20.11.0\t1\n/opt/my node/bin/node\t16\t0\n/usr/bin/python3\t3\t0\nnode\t20\t0\n",
        );
        assert_eq!(runtimes.len(), 2);
        assert!(runtimes[0].is_default);
        assert_eq!(runtimes[1].version, "18.19.0");
        assert!(!runtimes[1].is_default);
    }

    #[test]
    fn resolved_paths_must_be_plain_and_absolute() {
        let output = "npm output\nNODE /usr/bin/node\nPM /usr/bin/npm\n";
        assert_eq!(
            parse_marker(output, "NODE ").as_deref(),
            Some("/usr/bin/node")
        );
        assert_eq!(parse_marker(output, "PM ").as_deref(), Some("/usr/bin/npm"));
        assert!(parse_marker("NODE node", "NODE ").is_none());
        assert!(parse_marker("NODE /usr/bin/node;id", "NODE ").is_none());
        assert!(parse_marker("no marker", "NODE ").is_none());
        assert!(parse_marker("NODE /usr/bin/node\n", "PM ").is_none());
    }
}
