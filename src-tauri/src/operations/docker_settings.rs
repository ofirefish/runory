use std::collections::BTreeSet;
use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::domain::{
    AppError, AppResult, DockerDaemonConfig, DockerEngineInfo, DockerEngineSettingsView,
    DockerLogOpts, OperationResult, SessionId,
};
use crate::ssh::{RemoteCommand, RemoteExecResult, ServerSessionManager};

const DAEMON_JSON_PATH: &str = "/etc/docker/daemon.json";
const MAX_MIRRORS: usize = 20;
const MAX_INSECURE: usize = 20;
const ALLOWED_LOG_DRIVERS: &[&str] = &["", "json-file", "local", "journald", "syslog", "none"];
const MANAGED_KEYS: &[&str] = &[
    "registry-mirrors",
    "insecure-registries",
    "log-driver",
    "log-opts",
    "live-restore",
];

const DOCKER_INFO: &str =
    "command -v docker >/dev/null 2>&1 || exit 90; docker info --format '{{json .}}'";

const READ_DAEMON_JSON: &str = r#"command -v docker >/dev/null 2>&1 || exit 90
path="/etc/docker/daemon.json"
if sudo -n test -f "$path" 2>/dev/null; then
  printf 'EXISTS\n'
  sudo -n cat "$path" || exit 1
elif test -f "$path"; then
  printf 'EXISTS\n'
  cat "$path" || exit 1
elif sudo -n test -e "$path" 2>/dev/null || test -e "$path"; then
  exit 1
else
  printf 'MISSING\n'
  printf '{}\n'
fi
"#;

const WRITE_AND_RESTART: &str = r#"command -v docker >/dev/null 2>&1 || exit 90
set -e
tmp=$(mktemp /tmp/runory-daemon.XXXXXX)
trap 'rm -f "$tmp"' EXIT
cat > "$tmp"
test -s "$tmp"
if command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
  sudo -n mkdir -p /etc/docker
  sudo -n cp "$tmp" /etc/docker/daemon.json
  sudo -n chmod 644 /etc/docker/daemon.json
  if command -v systemctl >/dev/null 2>&1; then
    sudo -n systemctl restart docker
  else
    sudo -n service docker restart
  fi
else
  mkdir -p /etc/docker
  cp "$tmp" /etc/docker/daemon.json
  chmod 644 /etc/docker/daemon.json
  if command -v systemctl >/dev/null 2>&1; then
    systemctl restart docker
  else
    service docker restart
  fi
fi
docker info >/dev/null
"#;

pub struct DockerSettingsService;

impl DockerSettingsService {
    pub async fn get(
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<DockerEngineSettingsView> {
        let info_result = sessions
            .exec(session_id, RemoteCommand::script(DOCKER_INFO, Vec::new()))
            .await?;
        ensure_supported(&info_result)?;
        let info = parse_docker_info(&info_result.stdout)?;

        let config_result = sessions
            .exec(
                session_id,
                RemoteCommand::script(READ_DAEMON_JSON, Vec::new()),
            )
            .await?;
        ensure_supported(&config_result)?;
        let (config_exists, raw) = parse_daemon_file_output(&config_result.stdout)?;
        let (config, config_raw_preserved) = extract_daemon_config(&raw)?;

        Ok(DockerEngineSettingsView {
            info,
            config,
            config_path: DAEMON_JSON_PATH.to_owned(),
            config_exists,
            config_raw_preserved,
        })
    }

    pub async fn apply(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        config: DockerDaemonConfig,
        restart: bool,
    ) -> AppResult<OperationResult> {
        if !restart {
            return Err(AppError::InvalidOperation);
        }
        validate_daemon_config(&config)?;

        let config_result = sessions
            .exec(
                session_id,
                RemoteCommand::script(READ_DAEMON_JSON, Vec::new()),
            )
            .await?;
        ensure_supported(&config_result)?;
        let (_exists, raw) = parse_daemon_file_output(&config_result.stdout)?;
        let merged = merge_daemon_json(&raw, &config)?;
        let content = serde_json::to_vec_pretty(&merged).map_err(|_| AppError::InvalidOperation)?;

        let command = RemoteCommand::script(WRITE_AND_RESTART, Vec::new())
            .with_stdin(content)?
            .with_timeout(Duration::from_secs(120));
        let result = sessions.exec(session_id, command).await?;
        if result.exit_code == 90 {
            return Err(AppError::UnsupportedRemote);
        }
        Ok(OperationResult {
            success: result.exit_code == 0,
            output: combined_output(result),
        })
    }
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

fn parse_daemon_file_output(stdout: &str) -> AppResult<(bool, Value)> {
    let trimmed = stdout.trim();
    let (flag, body) = if let Some(rest) = trimmed.strip_prefix("EXISTS\n") {
        (true, rest)
    } else if let Some(rest) = trimmed.strip_prefix("MISSING\n") {
        (false, rest)
    } else if trimmed.starts_with('{') {
        (true, trimmed)
    } else {
        return Err(AppError::ExecFailed);
    };
    let value = if body.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(body.trim()).map_err(|_| AppError::ExecFailed)?
    };
    if !value.is_object() {
        return Err(AppError::ExecFailed);
    }
    Ok((flag, value))
}

fn parse_docker_info(stdout: &str) -> AppResult<DockerEngineInfo> {
    let value: Value = serde_json::from_str(stdout.trim()).map_err(|_| AppError::ExecFailed)?;
    Ok(DockerEngineInfo {
        server_version: string_field(&value, &["ServerVersion"]),
        storage_driver: string_field(&value, &["Driver"]),
        logging_driver: string_field(&value, &["LoggingDriver"]),
        operating_system: string_field(&value, &["OperatingSystem"]),
        architecture: string_field(&value, &["Architecture"]),
        ncpu: number_field(&value, &["NCPU"]) as u32,
        mem_total_bytes: number_field(&value, &["MemTotal"]),
        docker_root_dir: string_field(&value, &["DockerRootDir"]),
        live_restore_enabled: bool_field(&value, &["LiveRestoreEnabled"]),
    })
}

fn extract_daemon_config(raw: &Value) -> AppResult<(DockerDaemonConfig, bool)> {
    let object = raw.as_object().ok_or(AppError::InvalidOperation)?;
    let preserved = object
        .keys()
        .any(|key| !MANAGED_KEYS.contains(&key.as_str()));
    let registry_mirrors = string_list_field(object.get("registry-mirrors"));
    let insecure_registries = string_list_field(object.get("insecure-registries"));
    let log_driver = object
        .get("log-driver")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let log_opts = object
        .get("log-opts")
        .and_then(Value::as_object)
        .map(|opts| DockerLogOpts {
            max_size: opts
                .get("max-size")
                .and_then(Value::as_str)
                .map(str::to_owned),
            max_file: opts
                .get("max-file")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
        .unwrap_or_default();
    let live_restore = object
        .get("live-restore")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok((
        DockerDaemonConfig {
            registry_mirrors,
            insecure_registries,
            log_driver,
            log_opts,
            live_restore,
        },
        preserved,
    ))
}

pub(crate) fn merge_daemon_json(raw: &Value, config: &DockerDaemonConfig) -> AppResult<Value> {
    let mut object = raw.as_object().cloned().unwrap_or_else(Map::new);

    set_or_remove_string_array(&mut object, "registry-mirrors", &config.registry_mirrors);
    set_or_remove_string_array(
        &mut object,
        "insecure-registries",
        &config.insecure_registries,
    );

    let driver = config.log_driver.trim();
    if driver.is_empty() {
        object.remove("log-driver");
        object.remove("log-opts");
    } else {
        object.insert("log-driver".into(), Value::String(driver.to_owned()));
        if matches!(driver, "json-file" | "local") {
            let mut opts = Map::new();
            if let Some(max_size) = config
                .log_opts
                .max_size
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                opts.insert("max-size".into(), Value::String(max_size.to_owned()));
            }
            if let Some(max_file) = config
                .log_opts
                .max_file
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                opts.insert("max-file".into(), Value::String(max_file.to_owned()));
            }
            if opts.is_empty() {
                object.remove("log-opts");
            } else {
                object.insert("log-opts".into(), Value::Object(opts));
            }
        } else {
            object.remove("log-opts");
        }
    }

    object.insert("live-restore".into(), Value::Bool(config.live_restore));

    Ok(Value::Object(object))
}

fn set_or_remove_string_array(object: &mut Map<String, Value>, key: &str, values: &[String]) {
    if values.is_empty() {
        object.remove(key);
    } else {
        object.insert(
            key.into(),
            Value::Array(values.iter().cloned().map(Value::String).collect()),
        );
    }
}

pub(crate) fn validate_daemon_config(config: &DockerDaemonConfig) -> AppResult<()> {
    if config.registry_mirrors.len() > MAX_MIRRORS
        || config.insecure_registries.len() > MAX_INSECURE
    {
        return Err(AppError::InvalidOperation);
    }
    for mirror in &config.registry_mirrors {
        validate_registry_mirror(mirror)?;
    }
    for registry in &config.insecure_registries {
        validate_insecure_registry(registry)?;
    }
    let driver = config.log_driver.trim();
    if !ALLOWED_LOG_DRIVERS.contains(&driver) {
        return Err(AppError::InvalidOperation);
    }
    if let Some(max_size) = config.log_opts.max_size.as_deref() {
        let max_size = max_size.trim();
        if !max_size.is_empty() && !is_valid_max_size(max_size) {
            return Err(AppError::InvalidOperation);
        }
    }
    if let Some(max_file) = config.log_opts.max_file.as_deref() {
        let max_file = max_file.trim();
        if !max_file.is_empty() && !is_valid_max_file(max_file) {
            return Err(AppError::InvalidOperation);
        }
    }
    if !matches!(driver, "json-file" | "local") {
        let has_opts = config
            .log_opts
            .max_size
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
            || config
                .log_opts
                .max_file
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some();
        if has_opts {
            return Err(AppError::InvalidOperation);
        }
    }
    Ok(())
}

fn validate_registry_mirror(value: &str) -> AppResult<()> {
    let value = value.trim();
    if value.is_empty() || value.len() > 512 {
        return Err(AppError::InvalidOperation);
    }
    if !(value.starts_with("http://") || value.starts_with("https://")) {
        return Err(AppError::InvalidOperation);
    }
    if value.bytes().any(|byte| {
        byte.is_ascii_control()
            || matches!(
                byte,
                b';' | b'`' | b'$' | b'|' | b'&' | b'<' | b'>' | b'\n' | b'\r' | b' '
            )
    }) {
        return Err(AppError::InvalidOperation);
    }
    Ok(())
}

fn validate_insecure_registry(value: &str) -> AppResult<()> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 {
        return Err(AppError::InvalidOperation);
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':' | b'[') || byte == b']'
    }) {
        return Err(AppError::InvalidOperation);
    }
    Ok(())
}

fn is_valid_max_size(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > 16 {
        return false;
    }
    let (digits, _unit) = match bytes.last().copied() {
        Some(b'k' | b'K' | b'm' | b'M' | b'g' | b'G') => (&bytes[..bytes.len() - 1], true),
        Some(b'0'..=b'9') => (bytes, false),
        _ => return false,
    };
    if digits.is_empty() {
        return false;
    }
    digits.iter().all(|byte| byte.is_ascii_digit()) && !digits.iter().all(|&byte| byte == b'0')
}

fn is_valid_max_file(value: &str) -> bool {
    if value.is_empty() || value.len() > 8 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    value.parse::<u32>().ok().is_some_and(|n| n > 0)
}

fn string_list_field(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            let mut seen = BTreeSet::new();
            items
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|item| {
                    let trimmed = item.trim();
                    if trimmed.is_empty() || !seen.insert(trimmed.to_owned()) {
                        None
                    } else {
                        Some(trimmed.to_owned())
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn string_field(value: &Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(text) = value.get(*key).and_then(Value::as_str) {
            return text.to_owned();
        }
    }
    String::new()
}

fn number_field(value: &Value, keys: &[&str]) -> u64 {
    for key in keys {
        if let Some(number) = value.get(*key).and_then(Value::as_u64) {
            return number;
        }
        if let Some(number) = value.get(*key).and_then(Value::as_f64) {
            if number.is_finite() && number >= 0.0 {
                return number as u64;
            }
        }
    }
    0
}

fn bool_field(value: &Value, keys: &[&str]) -> bool {
    for key in keys {
        if let Some(flag) = value.get(*key).and_then(Value::as_bool) {
            return flag;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_docker_info_fixture() {
        let raw = r#"{
          "ServerVersion":"24.0.7",
          "Driver":"overlay2",
          "LoggingDriver":"json-file",
          "OperatingSystem":"Ubuntu 22.04.3 LTS",
          "Architecture":"x86_64",
          "NCPU":4,
          "MemTotal":8312487936,
          "DockerRootDir":"/var/lib/docker",
          "LiveRestoreEnabled":true
        }"#;
        let info = parse_docker_info(raw).expect("info");
        assert_eq!(info.server_version, "24.0.7");
        assert_eq!(info.storage_driver, "overlay2");
        assert_eq!(info.ncpu, 4);
        assert!(info.live_restore_enabled);
    }

    #[test]
    fn extracts_and_preserves_unknown_keys() {
        let raw = json!({
          "registry-mirrors": ["https://mirror.example"],
          "insecure-registries": ["registry.local:5000"],
          "log-driver": "json-file",
          "log-opts": {"max-size":"10m","max-file":"3"},
          "live-restore": true,
          "features": {"buildkit": true}
        });
        let (config, preserved) = extract_daemon_config(&raw).expect("config");
        assert!(preserved);
        assert_eq!(
            config.registry_mirrors,
            vec!["https://mirror.example".to_owned()]
        );
        assert_eq!(config.log_opts.max_size.as_deref(), Some("10m"));
        assert!(config.live_restore);
    }

    #[test]
    fn merges_whitelist_without_dropping_unknown_keys() {
        let raw = json!({
          "features": {"buildkit": true},
          "registry-mirrors": ["https://old.example"]
        });
        let config = DockerDaemonConfig {
            registry_mirrors: vec!["https://new.example".into()],
            insecure_registries: vec![],
            log_driver: "local".into(),
            log_opts: DockerLogOpts {
                max_size: Some("20m".into()),
                max_file: Some("5".into()),
            },
            live_restore: false,
        };
        let merged = merge_daemon_json(&raw, &config).expect("merge");
        assert_eq!(merged["features"]["buildkit"], true);
        assert_eq!(merged["registry-mirrors"][0], "https://new.example");
        assert_eq!(merged["log-driver"], "local");
        assert_eq!(merged["log-opts"]["max-size"], "20m");
        assert_eq!(merged["live-restore"], false);
        assert!(merged.get("insecure-registries").is_none());
    }

    #[test]
    fn validate_rejects_bad_mirror_and_opts() {
        let mut config = DockerDaemonConfig {
            registry_mirrors: vec!["ftp://bad".into()],
            ..DockerDaemonConfig::default()
        };
        assert!(validate_daemon_config(&config).is_err());

        config = DockerDaemonConfig {
            log_driver: "journald".into(),
            log_opts: DockerLogOpts {
                max_size: Some("10m".into()),
                max_file: None,
            },
            ..DockerDaemonConfig::default()
        };
        assert!(validate_daemon_config(&config).is_err());

        config = DockerDaemonConfig {
            log_driver: "json-file".into(),
            log_opts: DockerLogOpts {
                max_size: Some("10m".into()),
                max_file: Some("3".into()),
            },
            registry_mirrors: vec!["https://mirror.example".into()],
            insecure_registries: vec!["registry.local:5000".into()],
            live_restore: true,
        };
        assert!(validate_daemon_config(&config).is_ok());
    }

    #[test]
    fn parse_daemon_file_marks_missing() {
        let (exists, value) = parse_daemon_file_output("MISSING\n{}\n").expect("parse");
        assert!(!exists);
        assert!(value.as_object().unwrap().is_empty());
    }
}
