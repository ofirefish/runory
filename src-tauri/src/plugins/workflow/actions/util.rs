//! Shared validation and session helpers for Core workflow actions.

use std::sync::Arc;

use serde::Deserialize;

use super::ActionContext;
use crate::domain::SessionId;
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::ssh::{RemoteCommand, ServerSessionManager};

pub fn require_session(
    ctx: &ActionContext,
    action: &str,
) -> PluginResult<(Arc<ServerSessionManager>, SessionId)> {
    match (ctx.sessions.as_ref(), ctx.session_id) {
        (Some(s), Some(id)) => Ok((Arc::clone(s), id)),
        _ => Err(PluginError::RemoteExecution(format!(
            "{action} requires an active server session"
        ))),
    }
}

pub async fn require_caps(ctx: &ActionContext, caps: &[PluginCapability]) -> PluginResult<()> {
    for cap in caps {
        ctx.permission_service.require(&ctx.plugin_id, *cap).await?;
    }
    Ok(())
}

pub fn parse_input<T: for<'de> Deserialize<'de>>(
    input: &serde_json::Value,
    action: &str,
) -> PluginResult<T> {
    serde_json::from_value(input.clone())
        .map_err(|err| PluginError::WorkflowValidation(format!("invalid {action} input: {err}")))
}

pub fn validate_remote_path(path: &str) -> PluginResult<()> {
    if path.trim().is_empty() || path.contains('\0') || path.len() > 4096 {
        return Err(PluginError::WorkflowValidation(
            "invalid remote path".into(),
        ));
    }
    if path.contains("..") {
        return Err(PluginError::WorkflowValidation(
            "remote path must not contain ..".into(),
        ));
    }
    Ok(())
}

/// Safe bare program / command name (no path separators).
pub fn is_safe_command_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

/// Hostname or IP-like host: alphanumeric, dots, dashes, colons (IPv6).
pub fn validate_hostname(host: &str) -> PluginResult<()> {
    if host.is_empty() || host.len() > 253 {
        return Err(PluginError::WorkflowValidation("invalid host".into()));
    }
    if !host
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':'))
    {
        return Err(PluginError::WorkflowValidation(
            "host contains unsafe characters".into(),
        ));
    }
    Ok(())
}

pub fn validate_port(port: u16) -> PluginResult<()> {
    if port == 0 {
        return Err(PluginError::WorkflowValidation(
            "port must be 1..=65535".into(),
        ));
    }
    Ok(())
}

/// Accept JSON number or numeric string (expressions always resolve to strings).
pub fn deserialize_port<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    struct PortVisitor;
    impl<'de> Visitor<'de> for PortVisitor {
        type Value = u16;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a port number or numeric string")
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<u16, E> {
            u16::try_from(v).map_err(E::custom)
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<u16, E> {
            u16::try_from(v).map_err(E::custom)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<u16, E> {
            v.trim().parse().map_err(E::custom)
        }
    }
    deserializer.deserialize_any(PortVisitor)
}

/// Allow http/https URLs without shell metacharacters.
pub fn validate_http_url(url: &str) -> PluginResult<()> {
    if url.is_empty() || url.len() > 2048 || url.contains('\0') {
        return Err(PluginError::WorkflowValidation("invalid url".into()));
    }
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err(PluginError::WorkflowValidation(
            "url must be http or https".into(),
        ));
    }
    if url.bytes().any(|b| {
        matches!(
            b,
            b';' | b'"' | b'`' | b'$' | b'|' | b'&' | b'(' | b')' | b'<' | b'>' | b'\n' | b'\r'
        )
    }) {
        return Err(PluginError::WorkflowValidation(
            "url contains unsafe characters".into(),
        ));
    }
    Ok(())
}

pub fn validate_git_ref(value: &str) -> PluginResult<()> {
    if value.is_empty() || value.len() > 256 || value.contains('\0') {
        return Err(PluginError::WorkflowValidation("invalid git ref".into()));
    }
    if value.starts_with('-') {
        return Err(PluginError::WorkflowValidation(
            "git ref must not start with -".into(),
        ));
    }
    if value.contains("..") || value.contains('\\') {
        return Err(PluginError::WorkflowValidation(
            "git ref contains unsafe characters".into(),
        ));
    }
    Ok(())
}

pub fn validate_git_repository(repo: &str) -> PluginResult<()> {
    if repo.is_empty() || repo.len() > 2048 || repo.contains('\0') {
        return Err(PluginError::WorkflowValidation("invalid repository".into()));
    }
    if repo.bytes().any(|b| {
        matches!(
            b,
            b';' | b'"' | b'`' | b'$' | b'|' | b'&' | b'(' | b')' | b'<' | b'>' | b'\n' | b'\r'
        )
    }) {
        return Err(PluginError::WorkflowValidation(
            "repository contains unsafe characters".into(),
        ));
    }
    Ok(())
}

/// Unit / service name for systemctl and systemd unit basenames.
pub fn validate_service_name(name: &str) -> PluginResult<()> {
    if name.is_empty() || name.len() > 256 {
        return Err(PluginError::WorkflowValidation(
            "invalid service name".into(),
        ));
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'@' | b':'))
    {
        return Err(PluginError::WorkflowValidation(
            "service name contains unsafe characters".into(),
        ));
    }
    Ok(())
}

pub fn shell_quote(value: &str) -> PluginResult<String> {
    if value.contains('\0') || value.len() > 16 * 1024 {
        return Err(PluginError::WorkflowValidation(
            "argument too large or contains NUL".into(),
        ));
    }
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}

pub fn dynamic_program(program: String, args: Vec<String>) -> PluginResult<RemoteCommand> {
    RemoteCommand::dynamic_program(program, args)
        .map_err(|_| PluginError::RemoteExecution("invalid remote program".into()))
}

pub fn freeform_quoted(program: &str, args: &[&str]) -> PluginResult<RemoteCommand> {
    let mut script = shell_quote(program)?;
    for arg in args {
        script.push(' ');
        script.push_str(&shell_quote(arg)?);
    }
    RemoteCommand::freeform(script)
        .map_err(|_| PluginError::RemoteExecution("failed to build remote command".into()))
}

pub fn sudo_program(program: &str, args: Vec<String>) -> PluginResult<RemoteCommand> {
    let mut sudo_args = vec!["--".into(), program.to_owned()];
    sudo_args.extend(args);
    dynamic_program("sudo".into(), sudo_args)
}

/// Ensure `child` is the same as `base` or a path under `base`.
pub fn path_under_base(base: &str, child: &str) -> PluginResult<()> {
    validate_remote_path(base)?;
    validate_remote_path(child)?;
    let base_norm = base.trim_end_matches('/');
    if child == base_norm || child.starts_with(&format!("{base_norm}/")) {
        Ok(())
    } else {
        Err(PluginError::WorkflowValidation(
            "path must be under the declared base directory".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_escape() {
        assert!(validate_remote_path("/opt/../etc/passwd").is_err());
    }

    #[test]
    fn accepts_safe_command_name() {
        assert!(is_safe_command_name("python3"));
        assert!(!is_safe_command_name("/usr/bin/python3"));
        assert!(!is_safe_command_name("a;b"));
    }

    #[test]
    fn validates_http_url() {
        assert!(validate_http_url("https://example.com/health").is_ok());
        assert!(validate_http_url("ftp://x").is_err());
        assert!(validate_http_url("https://x$(id)").is_err());
    }

    #[test]
    fn path_under_base_checks_prefix() {
        assert!(path_under_base("/opt/app/.venv", "/opt/app/.venv/bin/python").is_ok());
        assert!(path_under_base("/opt/app/.venv", "/opt/other/bin/python").is_err());
    }
}
