//! Thin wrapper around the Teleport `tsh` CLI (structured outputs only).
//!
//! Never shell-concatenate user input; every argument is a separate argv element.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::connection::bastion::asset::{AssetProtocol, BastionAsset, BastionProtocol};
use crate::connection::bastion::session::BastionEndpoint;
use crate::helper::HelperError;

/// Non-secret Teleport profile knobs resolved from endpoint / provider_config.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TeleportConnectParams {
    pub proxy_addr: String,
    pub teleport_user: Option<String>,
    pub cluster_name: Option<String>,
    pub insecure: bool,
}

impl TeleportConnectParams {
    pub fn from_endpoint(endpoint: &BastionEndpoint, username_hint: Option<&str>) -> Self {
        let proxy_addr = proxy_addr_from_endpoint(endpoint);
        let cluster_name = string_config(&endpoint.provider_config, &["clusterName", "cluster_name"]);
        let insecure = bool_config(&endpoint.provider_config, &["insecure", "insecureSkipVerify"])
            || endpoint
                .tls
                .as_ref()
                .map(|tls| tls.insecure_skip_verify)
                .unwrap_or(false);
        let teleport_user = string_config(&endpoint.provider_config, &["teleportUser", "teleport_user"])
            .or_else(|| username_hint.map(str::to_string));
        Self {
            proxy_addr,
            teleport_user,
            cluster_name,
            insecure,
        }
    }
}

/// Parsed `tsh status --format=json` (schema-tolerant).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TshStatus {
    pub logged_in: bool,
    pub user: Option<String>,
    pub cluster: Option<String>,
    pub proxy: Option<String>,
    /// Unix millis when the short-lived cert expires.
    pub valid_until_ms: Option<i64>,
    pub os_logins: Vec<String>,
}

impl TshStatus {
    pub fn is_expired(&self) -> bool {
        match self.valid_until_ms {
            Some(until) if until > 0 => until <= now_millis(),
            _ => !self.logged_in,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.logged_in && !self.is_expired()
    }
}

/// Paths to Teleport-issued identity material under `~/.tsh` (never copied into Profile JSON).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TshIdentityFiles {
    pub identity_file: PathBuf,
    pub certificate_file: PathBuf,
}

pub struct TshClient {
    binary: PathBuf,
}

impl TshClient {
    pub fn new(binary: PathBuf) -> Self {
        Self { binary }
    }

    pub fn binary(&self) -> &PathBuf {
        &self.binary
    }

    pub fn version_string(&self) -> Result<String, HelperError> {
        let output = Command::new(&self.binary)
            .arg("version")
            .output()
            .map_err(|_| HelperError::Missing)?;
        let text = String::from_utf8_lossy(&output.stdout);
        let line = text.lines().next().unwrap_or("teleport").trim().to_string();
        if line.is_empty() {
            Err(HelperError::VersionMismatch)
        } else {
            Ok(line)
        }
    }

    pub fn status(&self, params: &TeleportConnectParams) -> Result<TshStatus, HelperError> {
        // `tsh status` supports --proxy / --insecure but NOT --cluster (Teleport 18).
        let mut cmd = Command::new(&self.binary);
        cmd.arg("status").arg("--format=json");
        append_proxy_flags(&mut cmd, params);
        let output = cmd.output().map_err(|_| HelperError::Missing)?;
        if !output.status.success() {
            // Still try to parse JSON if present (some builds exit non-zero with partial status).
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                let parsed = parse_tsh_status(&value);
                if parsed.logged_in {
                    return Ok(parsed);
                }
            }
            return Ok(TshStatus {
                logged_in: false,
                ..TshStatus::default()
            });
        }
        let value: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|_| HelperError::SpawnFailed)?;
        Ok(parse_tsh_status(&value))
    }

    pub fn status_ok(&self) -> Result<bool, HelperError> {
        let status = self.status(&TeleportConnectParams::default())?;
        Ok(status.is_valid())
    }

    /// Build argv for `tsh login` (no shell). Caller owns spawn / PTY.
    ///
    /// Cluster is a positional arg (`tsh login [<cluster>]`), not `--cluster`
    /// (Teleport 18 rejects `--cluster` on `login` / `status`).
    pub fn login_args(params: &TeleportConnectParams) -> Result<Vec<String>, HelperError> {
        if params.proxy_addr.trim().is_empty() {
            return Err(HelperError::SpawnFailed);
        }
        let mut args = vec![
            "login".into(),
            format!("--proxy={}", params.proxy_addr.trim()),
        ];
        if let Some(user) = params.teleport_user.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            args.push(format!("--user={user}"));
        }
        if params.insecure {
            args.push("--insecure".into());
        }
        if let Some(cluster) = params
            .cluster_name
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            args.push(cluster.to_string());
        }
        Ok(args)
    }

    pub fn logout_args(params: &TeleportConnectParams) -> Vec<String> {
        let mut args = vec!["logout".into()];
        if !params.proxy_addr.trim().is_empty() {
            args.push(format!("--proxy={}", params.proxy_addr.trim()));
        }
        args
    }

    /// `tsh proxy ssh [--proxy] [--cluster] [--insecure] os_login@node`
    pub fn proxy_ssh_args(
        params: &TeleportConnectParams,
        os_login: &str,
        node: &str,
    ) -> Result<Vec<String>, HelperError> {
        let login = os_login.trim();
        let node = node.trim();
        if login.is_empty() || node.is_empty() {
            return Err(HelperError::SpawnFailed);
        }
        // Defense in depth: reject shell metacharacters even though we use argv.
        if login.contains([' ', '\t', '\n', ';', '|', '&', '$', '`', '"', '\''])
            || node.contains([' ', '\t', '\n', ';', '|', '&', '$', '`', '"', '\''])
        {
            return Err(HelperError::SpawnFailed);
        }
        let mut args = vec!["proxy".into(), "ssh".into()];
        if !params.proxy_addr.trim().is_empty() {
            args.push(format!("--proxy={}", params.proxy_addr.trim()));
        }
        if let Some(cluster) = params
            .cluster_name
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            args.push(format!("--cluster={cluster}"));
        }
        if params.insecure {
            args.push("--insecure".into());
        }
        args.push(format!("{login}@{node}"));
        Ok(args)
    }

    /// `tsh ls --format=json` -> BastionAsset list. Never parse table output.
    pub fn list_nodes_as_assets(
        &self,
        params: &TeleportConnectParams,
    ) -> Result<Vec<BastionAsset>, HelperError> {
        // `tsh ls` supports -c/--cluster (unlike status/login).
        let mut cmd = Command::new(&self.binary);
        cmd.arg("ls").arg("--format=json");
        append_proxy_flags(&mut cmd, params);
        append_cluster_flag(&mut cmd, params);
        let output = cmd.output().map_err(|_| HelperError::Missing)?;
        if !output.status.success() {
            return Err(HelperError::SpawnFailed);
        }
        let value: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|_| HelperError::SpawnFailed)?;
        Ok(parse_tsh_ls(&value))
    }

    /// Resolve IdentityFile + CertificateFile via `tsh config` (preferred) or `~/.tsh` layout.
    pub fn resolve_identity_files(
        &self,
        params: &TeleportConnectParams,
        teleport_user: &str,
    ) -> Result<TshIdentityFiles, HelperError> {
        if let Some(files) = self.identity_from_tsh_config(params)? {
            return Ok(files);
        }
        identity_from_tsh_home(params, teleport_user).ok_or(HelperError::SpawnFailed)
    }

    fn identity_from_tsh_config(
        &self,
        params: &TeleportConnectParams,
    ) -> Result<Option<TshIdentityFiles>, HelperError> {
        if params.proxy_addr.trim().is_empty() {
            return Ok(None);
        }
        let mut cmd = Command::new(&self.binary);
        cmd.arg("config")
            .arg(format!("--proxy={}", params.proxy_addr.trim()));
        if params.insecure {
            cmd.arg("--insecure");
        }
        let output = cmd.output().map_err(|_| HelperError::Missing)?;
        if !output.status.success() {
            return Ok(None);
        }
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(parse_ssh_config_identity(&text))
    }
}

/// Flags shared by most tsh subcommands: `--proxy` and `--insecure`.
/// Do NOT add `--cluster` here — `status` / `login` / `config` reject it on Teleport 18.
fn append_proxy_flags(cmd: &mut Command, params: &TeleportConnectParams) {
    if !params.proxy_addr.trim().is_empty() {
        cmd.arg(format!("--proxy={}", params.proxy_addr.trim()));
    }
    if params.insecure {
        cmd.arg("--insecure");
    }
}

/// `-c/--cluster` only for subcommands that document it (`ls`, `proxy ssh`, …).
fn append_cluster_flag(cmd: &mut Command, params: &TeleportConnectParams) {
    if let Some(cluster) = params
        .cluster_name
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        cmd.arg(format!("--cluster={cluster}"));
    }
}

pub fn proxy_addr_from_endpoint(endpoint: &BastionEndpoint) -> String {
    let host = endpoint.host.trim();
    if host.is_empty() {
        return String::new();
    }
    if host.contains(':') {
        return host.to_string();
    }
    let port = endpoint
        .ports
        .web
        .or(endpoint.ports.api)
        .or(endpoint.ports.ssh)
        .unwrap_or(443);
    format!("{host}:{port}")
}

fn string_config(config: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = config
            .get(*key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            return Some(value.to_string());
        }
    }
    None
}

fn bool_config(config: &serde_json::Value, keys: &[&str]) -> bool {
    keys.iter().any(|key| {
        config
            .get(*key)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    })
}

pub fn parse_tsh_status(value: &serde_json::Value) -> TshStatus {
    // Teleport 18 `tsh status --format=json` nests fields under `active`.
    let root = value
        .get("active")
        .cloned()
        .unwrap_or_else(|| {
            if value.get("cluster").is_some() || value.get("username").is_some() {
                value.clone()
            } else if let Some(obj) = value.as_object() {
                obj.values().next().cloned().unwrap_or_else(|| value.clone())
            } else {
                value.clone()
            }
        });

    let user = root
        .get("username")
        .or_else(|| root.get("user"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let cluster = root
        .get("cluster")
        .or_else(|| root.get("cluster_name"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let proxy = root
        .get("proxy")
        .or_else(|| root.get("proxy_host"))
        .or_else(|| root.get("profile_url"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim_start_matches("https://").trim_start_matches("http://").to_string());

    let valid_until_ms = root
        .get("valid_until")
        .or_else(|| root.get("validUntil"))
        .and_then(parse_time_to_millis);

    let mut os_logins = Vec::new();
    collect_logins(root.get("logins").or_else(|| root.get("login")), &mut os_logins);
    // Traits may also list logins (Teleport 18).
    collect_logins(root.pointer("/traits/logins"), &mut os_logins);
    if let Some(roles) = root.get("roles").and_then(|v| v.as_array()) {
        for role in roles {
            collect_logins(role.get("logins"), &mut os_logins);
        }
    }

    os_logins.sort();
    os_logins.dedup();

    let logged_in = user.is_some() || cluster.is_some() || !os_logins.is_empty();
    TshStatus {
        logged_in,
        user,
        cluster,
        proxy,
        valid_until_ms,
        os_logins,
    }
}

fn collect_logins(value: Option<&serde_json::Value>, out: &mut Vec<String>) {
    let Some(value) = value else {
        return;
    };
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                if let Some(s) = item.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                    out.push(s.to_string());
                }
            }
        }
        serde_json::Value::String(s) => {
            for part in s.split([',', ' ']) {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    out.push(trimmed.to_string());
                }
            }
        }
        _ => {}
    }
}

fn parse_time_to_millis(value: &serde_json::Value) -> Option<i64> {
    if let Some(n) = value.as_i64() {
        // seconds vs millis heuristic
        return Some(if n > 1_000_000_000_000 { n } else { n * 1000 });
    }
    if let Some(n) = value.as_u64() {
        let n = n as i64;
        return Some(if n > 1_000_000_000_000 { n } else { n * 1000 });
    }
    let s = value.as_str()?;
    // RFC3339
    if let Ok(dt) = chrono_parse_rfc3339(s) {
        return Some(dt);
    }
    None
}

fn chrono_parse_rfc3339(s: &str) -> Result<i64, ()> {
    // Avoid adding chrono dependency: use a minimal parser via `httpdate`-less approach.
    // Prefer `time` crate if present; otherwise accept Unix-like numeric strings only.
    // Teleport typically emits RFC3339; parse without adding a chrono dependency.
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err(());
    }
    // Try `humantime`/`chrono` alternatives: parse via `SystemTime` compatible crate.
    // russh stack often has no chrono; use `time` if available through dependencies.
    parse_rfc3339_millis(trimmed).ok_or(())
}

fn parse_rfc3339_millis(s: &str) -> Option<i64> {
    // Format: 2026-09-12T12:00:00Z or with fractional / offset (+08:00 / -05:00)
    let (date, rest) = s.split_once('T')?;
    let mut date_parts = date.split('-');
    let year: i32 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;

    let (time, offset_secs) = if let Some(idx) = rest.rfind(['+', '-']) {
        // Avoid treating the date separator; offset always appears after HH:MM:SS.
        if idx > 0 && rest.as_bytes().get(idx - 1).is_some_and(|b| b.is_ascii_digit()) {
            let (time, off) = rest.split_at(idx);
            (time, parse_offset_secs(off)?)
        } else if rest.ends_with('Z') || rest.ends_with('z') {
            (rest.trim_end_matches(['Z', 'z']), 0)
        } else {
            (rest, 0)
        }
    } else {
        (rest.trim_end_matches(['Z', 'z']), 0)
    };

    let mut time_parts = time.split(':');
    let hour: u32 = time_parts.next()?.parse().ok()?;
    let minute: u32 = time_parts.next()?.parse().ok()?;
    let second_raw = time_parts.next().unwrap_or("0");
    let second: u32 = second_raw.split('.').next()?.parse().ok()?;
    let days = days_from_civil(year, month, day)?;
    let secs = i64::from(days) * 86400
        + i64::from(hour) * 3600
        + i64::from(minute) * 60
        + i64::from(second)
        - offset_secs;
    Some(secs * 1000)
}

fn parse_offset_secs(offset: &str) -> Option<i64> {
    if offset == "Z" || offset == "z" {
        return Some(0);
    }
    let sign = match offset.chars().next()? {
        '+' => 1i64,
        '-' => -1i64,
        _ => return None,
    };
    let body = &offset[1..];
    let (h, m) = if let Some((h, m)) = body.split_once(':') {
        (h.parse::<i64>().ok()?, m.parse::<i64>().ok()?)
    } else if body.len() == 4 {
        (body[..2].parse().ok()?, body[2..].parse().ok()?)
    } else {
        return None;
    };
    Some(sign * (h * 3600 + m * 60))
}

/// Civil date ->?days since Unix epoch (Howard Hinnant algorithm).
fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i32> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let m = month as i32;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy as u32;
    Some(era * 146097 + doe as i32 - 719468)
}

pub fn parse_tsh_ls(value: &serde_json::Value) -> Vec<BastionAsset> {
    let nodes = match value {
        serde_json::Value::Array(items) => items.clone(),
        serde_json::Value::Object(map) => map
            .get("nodes")
            .or_else(|| map.get("items"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    nodes.into_iter().filter_map(node_to_asset).collect()
}

fn node_to_asset(node: serde_json::Value) -> Option<BastionAsset> {
    let labels = extract_labels(&node);
    let hostname = node
        .pointer("/spec/hostname")
        .or_else(|| node.get("hostname"))
        .or_else(|| node.get("host"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| labels.get("hostname").cloned())
        .or_else(|| {
            node.get("name")
                .or_else(|| node.pointer("/metadata/name"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })?;
    let remote_id = node
        .pointer("/spec/hostname")
        .or_else(|| node.get("hostname"))
        .or_else(|| node.get("host"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| hostname.clone());
    let address = node
        .pointer("/spec/addr")
        .or_else(|| node.get("addr"))
        .or_else(|| node.get("address"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Some(BastionAsset {
        provider: "teleport".into(),
        remote_id,
        name: hostname,
        address,
        platform: Some("linux".into()),
        protocols: vec![AssetProtocol {
            protocol: BastionProtocol::Ssh,
            port: Some(22),
            enabled: true,
        }],
        node_path: None,
        labels,
        metadata: node,
    })
}

fn extract_labels(node: &serde_json::Value) -> HashMap<String, String> {
    let mut labels = HashMap::new();
    let sources = [
        node.get("labels"),
        node.pointer("/metadata/labels"),
        node.get("cmd_labels"),
        node.pointer("/spec/cmd_labels"),
    ];
    for source in sources.into_iter().flatten() {
        match source {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    let value = match v {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Object(inner) => inner
                            .get("result")
                            .or_else(|| inner.get("value"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        other => other.to_string(),
                    };
                    if !k.is_empty() && !value.is_empty() {
                        labels.insert(k.clone(), value);
                    }
                }
            }
            _ => {}
        }
    }
    labels
}

pub fn parse_ssh_config_identity(text: &str) -> Option<TshIdentityFiles> {
    let mut identity: Option<PathBuf> = None;
    let mut certificate: Option<PathBuf> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("identityfile") {
            if let Some(path) = config_value(trimmed) {
                identity = Some(PathBuf::from(unquote(&path)));
            }
        } else if lower.starts_with("certificatefile") {
            if let Some(path) = config_value(trimmed) {
                certificate = Some(PathBuf::from(unquote(&path)));
            }
        }
    }
    match (identity, certificate) {
        (Some(identity_file), Some(certificate_file))
            if identity_file.is_file() && certificate_file.is_file() =>
        {
            Some(TshIdentityFiles {
                identity_file,
                certificate_file,
            })
        }
        _ => None,
    }
}

fn config_value(line: &str) -> Option<String> {
    let mut parts = line.split_whitespace();
    let _key = parts.next()?;
    let value = parts.collect::<Vec<_>>().join(" ");
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn unquote(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn identity_from_tsh_home(
    params: &TeleportConnectParams,
    teleport_user: &str,
) -> Option<TshIdentityFiles> {
    let home = dirs_home()?;
    let proxy_host = params
        .proxy_addr
        .split(':')
        .next()
        .unwrap_or(params.proxy_addr.as_str())
        .trim();
    if proxy_host.is_empty() || teleport_user.trim().is_empty() {
        return None;
    }
    let base = home.join(".tsh").join("keys").join(proxy_host);
    let identity_file = base.join(teleport_user.trim());
    if !identity_file.is_file() {
        return None;
    }
    // Teleport 15+: <user>-ssh/<cluster>-cert.pub
    let ssh_dir = base.join(format!("{}-ssh", teleport_user.trim()));
    let certificate_file = find_cert_in_dir(&ssh_dir)
        .or_else(|| {
            let legacy = base.join(format!("{}-cert.pub", teleport_user.trim()));
            legacy.is_file().then_some(legacy)
        })
        .or_else(|| {
            params.cluster_name.as_ref().and_then(|cluster| {
                let path = ssh_dir.join(format!("{cluster}-cert.pub"));
                path.is_file().then_some(path)
            })
        })?;
    Some(TshIdentityFiles {
        identity_file,
        certificate_file,
    })
}

fn find_cert_in_dir(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("-cert.pub") || n.ends_with(".cert.pub"))
            && path.is_file()
        {
            return Some(path);
        }
    }
    None
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_args_use_separate_argv_and_reject_empty_proxy() {
        let err = TshClient::login_args(&TeleportConnectParams::default()).unwrap_err();
        assert!(matches!(err, HelperError::SpawnFailed));

        let args = TshClient::login_args(&TeleportConnectParams {
            proxy_addr: "teleport.local:3080".into(),
            teleport_user: Some("runory-test".into()),
            cluster_name: Some("teleport.local".into()),
            insecure: true,
        })
        .unwrap();
        assert_eq!(
            args,
            vec![
                "login".to_string(),
                "--proxy=teleport.local:3080".to_string(),
                "--user=runory-test".to_string(),
                "--insecure".to_string(),
                "teleport.local".to_string(),
            ]
        );
    }

    #[test]
    fn proxy_ssh_args_include_cluster_flag() {
        let params = TeleportConnectParams {
            proxy_addr: "teleport.local:3080".into(),
            cluster_name: Some("leaf.example".into()),
            insecure: true,
            ..Default::default()
        };
        let args = TshClient::proxy_ssh_args(&params, "root", "teleport-node01").unwrap();
        assert!(args.iter().any(|a| a == "--cluster=leaf.example"));
        assert!(args.ends_with(&["root@teleport-node01".to_string()]));
    }

    #[test]
    fn proxy_ssh_args_reject_shell_metacharacters() {
        let params = TeleportConnectParams {
            proxy_addr: "teleport.local:3080".into(),
            insecure: true,
            ..Default::default()
        };
        let err = TshClient::proxy_ssh_args(&params, "root", "server; rm -rf /").unwrap_err();
        assert!(matches!(err, HelperError::SpawnFailed));

        let args = TshClient::proxy_ssh_args(&params, "root", "teleport-node01").unwrap();
        assert_eq!(
            args,
            vec![
                "proxy".to_string(),
                "ssh".to_string(),
                "--proxy=teleport.local:3080".to_string(),
                "--insecure".to_string(),
                "root@teleport-node01".to_string(),
            ]
        );
    }

    #[test]
    fn parse_tsh_ls_v18_fixture() {
        let raw = include_str!("../../../../../tests/fixtures/teleport/tsh-ls-v18.json");
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        let assets = parse_tsh_ls(&value);
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].remote_id, "teleport-node01");
        assert_eq!(assets[0].name, "teleport-node01");
        assert_eq!(
            assets[0].labels.get("env").map(String::as_str),
            Some("development")
        );
        assert_eq!(
            assets[0].labels.get("hostname").map(String::as_str),
            Some("teleport-node01")
        );
    }

    #[test]
    fn parse_tsh_status_extracts_logins_and_expiry() {
        let raw = include_str!("../../../../../tests/fixtures/teleport/tsh-status-v18.json");
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        let status = parse_tsh_status(&value);
        assert!(status.logged_in);
        assert_eq!(status.user.as_deref(), Some("runory-test"));
        assert_eq!(status.cluster.as_deref(), Some("teleport.local"));
        assert!(status.os_logins.contains(&"root".to_string()));
        assert!(status.os_logins.contains(&"ubuntu".to_string()));
        assert!(status.valid_until_ms.is_some());
    }

    #[test]
    fn parse_tsh_status_live_v18_active_nesting_and_offset() {
        let raw = include_str!("../../../../../tests/fixtures/teleport/tsh-status-live-v18.json");
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        let status = parse_tsh_status(&value);
        assert!(status.is_valid());
        assert_eq!(status.user.as_deref(), Some("runory-test"));
        assert_eq!(status.cluster.as_deref(), Some("teleport.local"));
        assert_eq!(status.os_logins, vec!["root".to_string()]);
        assert!(status.valid_until_ms.is_some());
    }

    #[test]
    fn parse_tsh_ls_live_uses_hostname_not_uuid() {
        let raw = include_str!("../../../../../tests/fixtures/teleport/tsh-ls-live-v18.json");
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        let assets = parse_tsh_ls(&value);
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].remote_id, "teleport-node01");
        assert_eq!(assets[0].name, "teleport-node01");
        assert_ne!(assets[0].remote_id, "933afa7c-f803-4c42-a540-d4cf3660d5c8");
    }

    #[test]
    fn parse_ssh_config_identity_paths() {
        let text = r#"
Host *.teleport.local teleport.local
    IdentityFile "C:\Users\me\.tsh\keys\teleport.local\runory-test"
    CertificateFile "C:\Users\me\.tsh\keys\teleport.local\runory-test-ssh\teleport.local-cert.pub"
"#;
        // Files do not exist on disk -> None (path presence required).
        assert!(parse_ssh_config_identity(text).is_none());
    }

    #[test]
    fn load_live_teleport_identity_material() {
        let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap();
        let key_path = std::path::PathBuf::from(&home).join(".tsh/keys/teleport.local/runory-test");
        let cert_path = std::path::PathBuf::from(&home)
            .join(".tsh/keys/teleport.local/runory-test-ssh/teleport.local-cert.pub");
        if !key_path.is_file() || !cert_path.is_file() {
            eprintln!("skip: no live teleport identity");
            return;
        }
        let key_bytes = std::fs::read(&key_path).unwrap();
        let cert_bytes = std::fs::read(&cert_path).unwrap();
        let key = russh::keys::decode_secret_key(std::str::from_utf8(&key_bytes).unwrap(), None)
            .expect("decode key");
        let cert = russh::keys::Certificate::from_openssh(std::str::from_utf8(&cert_bytes).unwrap())
            .expect("decode cert");
        eprintln!(
            "key ok alg={:?} cert principals={:?}",
            key.algorithm(),
            cert.valid_principals()
        );
        assert!(!cert.valid_principals().is_empty());
    }

    #[tokio::test]
    #[ignore = "requires live Teleport cluster + tsh login"]
    async fn live_stdio_proxy_cert_auth_smoke() {
        let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap();
        let key_path = std::path::PathBuf::from(&home).join(".tsh/keys/teleport.local/runory-test");
        let cert_path = std::path::PathBuf::from(&home)
            .join(".tsh/keys/teleport.local/runory-test-ssh/teleport.local-cert.pub");
        let tsh = which_tsh();
        if tsh.is_none() || !key_path.is_file() || !cert_path.is_file() {
            eprintln!("skip: live teleport unavailable");
            return;
        }
        let tsh = tsh.unwrap();
        let key = russh::keys::load_secret_key(&key_path, None).expect("key");
        let cert = russh::keys::load_openssh_certificate(&cert_path).expect("cert");

        let mut child = tokio::process::Command::new(&tsh)
            .args([
                "proxy",
                "ssh",
                "--proxy=teleport.local:3080",
                "--cluster=teleport.local",
                "--insecure",
                "root@teleport-node01",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn tsh proxy");
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        // drain stderr
        if let Some(mut err) = child.stderr.take() {
            tokio::spawn(async move {
                use tokio::io::AsyncReadExt;
                let mut buf = [0u8; 512];
                while let Ok(n) = err.read(&mut buf).await {
                    if n == 0 { break; }
                    eprintln!("tsh-proxy-stderr: {}", String::from_utf8_lossy(&buf[..n]));
                }
            });
        }

        struct Stream {
            r: tokio::process::ChildStdout,
            w: tokio::process::ChildStdin,
        }
        impl tokio::io::AsyncRead for Stream {
            fn poll_read(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                std::pin::Pin::new(&mut self.r).poll_read(cx, buf)
            }
        }
        impl tokio::io::AsyncWrite for Stream {
            fn poll_write(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &[u8],
            ) -> std::task::Poll<Result<usize, std::io::Error>> {
                std::pin::Pin::new(&mut self.w).poll_write(cx, buf)
            }
            fn poll_flush(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Result<(), std::io::Error>> {
                std::pin::Pin::new(&mut self.w).poll_flush(cx)
            }
            fn poll_shutdown(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Result<(), std::io::Error>> {
                std::pin::Pin::new(&mut self.w).poll_shutdown(cx)
            }
        }

        struct H;
        impl russh::client::Handler for H {
            type Error = russh::Error;
            async fn check_server_key(
                &mut self,
                _key: &russh::keys::PublicKeyOrCertificate,
            ) -> Result<bool, Self::Error> {
                Ok(true)
            }
        }

        let stream = Stream { r: stdout, w: stdin };
        let config = std::sync::Arc::new(russh::client::Config {
            preferred: russh::Preferred {
                host_key_certificates: std::borrow::Cow::Borrowed(&[
                    russh::keys::Algorithm::Ecdsa {
                        curve: russh::keys::EcdsaCurve::NistP256,
                    },
                    russh::keys::Algorithm::Ecdsa {
                        curve: russh::keys::EcdsaCurve::NistP384,
                    },
                    russh::keys::Algorithm::Ed25519,
                ]),
                ..Default::default()
            },
            ..Default::default()
        });
        let mut handle = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            russh::client::connect_stream(config, stream, H),
        )
        .await
        .expect("handshake timeout")
        .expect("connect_stream");

        let auth = handle
            .authenticate_openssh_cert("root", std::sync::Arc::new(key), cert)
            .await
            .expect("auth call");
        eprintln!("auth success={}", auth.success());
        assert!(auth.success(), "teleport cert auth must succeed");
        let _ = child.kill().await;
    }

    fn which_tsh() -> Option<std::path::PathBuf> {
        let name = if cfg!(windows) { "tsh.exe" } else { "tsh" };
        std::env::var_os("PATH").and_then(|paths| {
            for dir in std::env::split_paths(&paths) {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
            None
        })
    }
}
