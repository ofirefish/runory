use std::time::Duration;

use crate::domain::{
    AppError, AppResult, JavaBuildTool, JavaProjectConfig, JavaRuntime, JdkVersion,
    OperationResult, PythonServiceAction, SessionId,
};
use crate::ssh::{RemoteCommand, ServerSessionManager};

use super::java_unit::{render_java_unit, resolve_jar_path};
use super::java_validation::{
    java_service_name, validate_java_bin, validate_java_config, validate_java_path,
};
use super::service::{ensure_operation, join_outputs, DeploymentService};
use super::systemd_install::INSTALL_UNIT_SCRIPT;
use super::validation::validate_app_name;

/// Exit 90 = missing JDK/systemd/build tool, 91 = invalid account or path.
/// Prints `JAVA <path>` for the resolved interpreter; a missing jar is reported on stderr.
const PREPARE_SCRIPT: &str = r#"set -e
if test "$2" = java; then java=$(command -v java 2>/dev/null) || exit 90; else java="$2"; fi
test -x "$java" || exit 90
java=$(readlink -f "$java" 2>/dev/null || printf '%s' "$java")
"$java" -version >/dev/null 2>&1 || exit 90
command -v systemctl >/dev/null 2>&1 || exit 90
test -d /run/systemd/system || exit 90
id "$4" >/dev/null 2>&1 || exit 91
test -d "$1" || exit 91
case "$5" in
  maven)
    (cd "$1"
     if test -x ./mvnw; then ./mvnw -B -q -DskipTests package
     else command -v mvn >/dev/null 2>&1 || exit 90; mvn -B -q -DskipTests package; fi)
    ;;
  gradle)
    (cd "$1"
     if test -x ./gradlew; then ./gradlew --no-daemon -q build -x test
     else command -v gradle >/dev/null 2>&1 || exit 90; gradle --no-daemon -q build -x test; fi)
    ;;
  none) ;;
  *) exit 91 ;;
esac
if ! test -f "$3"; then printf 'JAR_MISSING %s\n' "$3" >&2; exit 1; fi
printf 'JAVA %s\n' "$java"
"#;

/// Installs one OpenJDK LTS through the distribution package manager. Exit 90 = no supported
/// package manager, 91 = unsupported version; without root or passwordless sudo nothing is run.
const INSTALL_JDK_SCRIPT: &str = r#"set -e
case "$1" in 8|11|17|21|25) ;; *) exit 91 ;; esac
if test "$(id -u)" = 0; then
  SUDO=""
elif command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
  SUDO="sudo -n"
else
  printf 'PRIVILEGE_REQUIRED\n' >&2
  exit 1
fi
if command -v apt-get >/dev/null 2>&1; then
  $SUDO env DEBIAN_FRONTEND=noninteractive apt-get update -qq || true
  $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y "openjdk-$1-jdk-headless"
elif command -v dnf >/dev/null 2>&1 || command -v yum >/dev/null 2>&1; then
  pm=dnf; command -v dnf >/dev/null 2>&1 || pm=yum
  pkg="java-$1-openjdk-devel"
  if test "$1" = 8; then pkg="java-1.8.0-openjdk-devel"; fi
  $SUDO "$pm" install -y "$pkg"
elif command -v zypper >/dev/null 2>&1; then
  pkg="java-$1-openjdk-devel"
  if test "$1" = 8; then pkg="java-1_8_0-openjdk-devel"; fi
  $SUDO zypper --non-interactive install "$pkg"
else
  exit 90
fi
printf 'JDK_INSTALLED %s\n' "$1"
"#;

/// Switches the system-wide `java` (and `javac` when registered) alternative. Only a path that the
/// alternatives system already knows can be selected, so the caller cannot point it at arbitrary files.
const SET_DEFAULT_JDK_SCRIPT: &str = r#"set -e
if command -v update-alternatives >/dev/null 2>&1; then tool=update-alternatives
elif command -v alternatives >/dev/null 2>&1; then tool=alternatives
else exit 90; fi
registered() { "$tool" --display "$1" 2>/dev/null | awk -v p="$2" '$1==p{f=1} END{exit !f}'; }
registered java "$1" || exit 91
if test "$(id -u)" = 0; then
  SUDO=""
elif command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
  SUDO="sudo -n"
else
  printf 'PRIVILEGE_REQUIRED\n' >&2
  exit 1
fi
$SUDO "$tool" --set java "$1"
javac="${1%/java}/javac"
if registered javac "$javac"; then $SUDO "$tool" --set javac "$javac"; fi
now=$(readlink -f "$(command -v java)" 2>/dev/null || true)
test "$now" = "$1" || exit 1
printf 'DEFAULT_JDK %s\n' "$1"
"#;

impl DeploymentService {
    pub async fn java_install_jdk(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        version: JdkVersion,
    ) -> AppResult<OperationResult> {
        let profile_id = sessions.profile_id(session_id).await?;
        let label = jdk_version_label(version);
        let result = async {
            ensure_operation(
                sessions
                    .exec(
                        session_id,
                        RemoteCommand::script(INSTALL_JDK_SCRIPT, vec![label.into()])
                            .with_timeout(Duration::from_secs(1200)),
                    )
                    .await?,
            )
        }
        .await;
        self.finish(
            profile_id,
            "java-install-jdk",
            format!("jdk-{label}"),
            result,
        )
        .await
    }

    pub async fn java_set_default_jdk(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        binary: String,
    ) -> AppResult<OperationResult> {
        if binary == "java" {
            return Err(AppError::InvalidOperation);
        }
        validate_java_bin(&binary)?;
        let profile_id = sessions.profile_id(session_id).await?;
        let result = async {
            ensure_operation(
                sessions
                    .exec(
                        session_id,
                        RemoteCommand::script(SET_DEFAULT_JDK_SCRIPT, vec![binary.clone()])
                            .with_timeout(Duration::from_secs(60)),
                    )
                    .await?,
            )
        }
        .await;
        self.finish(profile_id, "java-set-default-jdk", binary, result)
            .await
    }

    pub async fn java_control(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        name: &str,
        action: PythonServiceAction,
    ) -> AppResult<OperationResult> {
        let service = java_service_name(name)?;
        let operation = match action {
            PythonServiceAction::Start => "java-start",
            PythonServiceAction::Stop => "java-stop",
            PythonServiceAction::Restart => "java-restart",
        };
        self.systemd_control(sessions, session_id, service, action, operation)
            .await
    }

    pub async fn java_provision(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        name: String,
        project_path: String,
        java: JavaProjectConfig,
    ) -> AppResult<OperationResult> {
        validate_app_name(&name)?;
        validate_java_path(&project_path)?;
        validate_java_config(&java)?;
        let service = java_service_name(&name)?;
        let jar = resolve_jar_path(&project_path, &java.jar_path);
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
                                java.java_bin.clone(),
                                jar,
                                java.run_user.clone(),
                                build_tool_label(java.build_tool).into(),
                            ],
                        )
                        .with_timeout(Duration::from_secs(1800)),
                    )
                    .await?,
            )?;
            if !prepare.success {
                return Ok(prepare);
            }
            let java_exec =
                parse_java_executable(&prepare.output).ok_or(AppError::UnsupportedRemote)?;
            let unit = render_java_unit(&name, &project_path, &java_exec, &java, &service)?;

            let install = ensure_operation(
                sessions
                    .exec(
                        session_id,
                        RemoteCommand::script(
                            INSTALL_UNIT_SCRIPT,
                            vec![
                                project_path.clone(),
                                service.clone(),
                                if java.auto_start { "1" } else { "0" }.into(),
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

        self.finish(profile_id, "java-provision", project_path, result)
            .await
    }

    pub async fn list_java_runtimes(
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<Vec<JavaRuntime>> {
        let result = sessions
            .exec(
                session_id,
                RemoteCommand::script(
                    r#"found=0
seen=""
d=$(command -v java 2>/dev/null || true)
defreal=""
if test -n "$d"; then defreal=$(readlink -f "$d" 2>/dev/null || true); fi
for cand in "$(command -v java 2>/dev/null)" /usr/lib/jvm/*/bin/java /usr/java/*/bin/java /opt/*/bin/java /usr/local/*/bin/java; do
  test -n "$cand" && test -x "$cand" || continue
  real=$(readlink -f "$cand" 2>/dev/null || printf '%s' "$cand")
  case "$seen" in *"|$real|"*) continue ;; esac
  seen="$seen|$real|"
  ver=$("$real" -version 2>&1 | sed -n '1s/.*"\(.*\)".*/\1/p')
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
        Ok(parse_java_runtimes(&result.stdout))
    }
}

fn jdk_version_label(version: JdkVersion) -> &'static str {
    match version {
        JdkVersion::V8 => "8",
        JdkVersion::V11 => "11",
        JdkVersion::V17 => "17",
        JdkVersion::V21 => "21",
        JdkVersion::V25 => "25",
    }
}

fn build_tool_label(tool: JavaBuildTool) -> &'static str {
    match tool {
        JavaBuildTool::None => "none",
        JavaBuildTool::Maven => "maven",
        JavaBuildTool::Gradle => "gradle",
    }
}

fn parse_java_executable(output: &str) -> Option<String> {
    let path = output
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix("JAVA "))?
        .trim();
    // The path is spliced into a unit file; keep it strictly absolute and plain.
    validate_java_path(path).ok()?;
    Some(path.to_string())
}

fn parse_java_runtimes(output: &str) -> Vec<JavaRuntime> {
    let mut runtimes: Vec<JavaRuntime> = Vec::new();
    for line in output.lines() {
        let mut parts = line.split('\t');
        let binary = parts.next().unwrap_or("").trim();
        let version = parts.next().unwrap_or("").trim();
        let is_default = parts.next().is_some_and(|flag| flag.trim() == "1");
        if binary == "java"
            || validate_java_bin(binary).is_err()
            || runtimes.iter().any(|item| item.binary == binary)
        {
            continue;
        }
        runtimes.push(JavaRuntime {
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
    fn parses_java_runtime_lines_and_skips_unsafe_paths() {
        let runtimes = parse_java_runtimes(
            "/usr/lib/jvm/java-17-openjdk-amd64/bin/java\t17.0.9\t1\n/usr/lib/jvm/java-8/bin/java\t1.8.0_392\t0\n/usr/lib/jvm/java-17-openjdk-amd64/bin/java\t17.0.9\t1\n/opt/my jdk/bin/java\t21\n/usr/bin/python3\t3\njava\t17\n",
        );
        assert_eq!(runtimes.len(), 2);
        assert_eq!(runtimes[0].version, "17.0.9");
        assert!(runtimes[0].is_default);
        assert!(!runtimes[1].is_default);
        assert_eq!(runtimes[1].binary, "/usr/lib/jvm/java-8/bin/java");
    }

    #[test]
    fn jdk_versions_are_a_closed_set() {
        for (raw, label) in [("\"8\"", "8"), ("\"17\"", "17"), ("\"25\"", "25")] {
            let version: JdkVersion = serde_json::from_str(raw).unwrap();
            assert_eq!(jdk_version_label(version), label);
        }
        for bad in ["\"7\"", "\"17;id\"", "17"] {
            assert!(serde_json::from_str::<JdkVersion>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn resolved_interpreter_must_be_a_plain_absolute_path() {
        assert_eq!(
            parse_java_executable("build log\nJAVA /usr/lib/jvm/jdk-21/bin/java\n").as_deref(),
            Some("/usr/lib/jvm/jdk-21/bin/java")
        );
        assert!(parse_java_executable("JAVA java").is_none());
        assert!(parse_java_executable("JAVA /usr/bin/java;id").is_none());
        assert!(parse_java_executable("no marker").is_none());
    }
}
