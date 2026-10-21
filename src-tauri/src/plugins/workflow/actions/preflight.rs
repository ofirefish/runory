//! Structured host readiness report for deploy preflight (R0, no mutation).

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::util::{
    deserialize_port, parse_input, require_caps, require_session, validate_hostname, validate_port,
};
use super::{risk_requires_approval, ActionContext, ActionPlan, ActionResult, WorkflowAction};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::ssh::RemoteCommand;
use crate::tools::RiskLevel;

pub struct SystemPreflightReportAction;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreflightInput {
    #[serde(default = "default_host")]
    host: String,
    #[serde(default = "default_port", deserialize_with = "deserialize_port")]
    port: u16,
    #[serde(default = "default_min_python")]
    min_python: String,
}

fn default_host() -> String {
    "127.0.0.1".into()
}
fn default_port() -> u16 {
    8000
}
fn default_min_python() -> String {
    "3.10".into()
}

#[async_trait]
impl WorkflowAction for SystemPreflightReportAction {
    fn id(&self) -> &'static str {
        "system.preflight_report"
    }

    fn required_capabilities(
        &self,
        _input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        Ok(vec![PluginCapability::HostRead, PluginCapability::SshExec])
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        Ok(RiskLevel::R0)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed: PreflightInput = parse_input(input, self.id())?;
        validate_hostname(&parsed.host)?;
        validate_port(parsed.port)?;
        require_caps(
            ctx,
            &[PluginCapability::HostRead, PluginCapability::SshExec],
        )
        .await?;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk: RiskLevel::R0,
            summary: format!(
                "Preflight readiness (port {}, python>={})",
                parsed.port, parsed.min_python
            ),
            changes: Vec::new(),
            requires_approval: risk_requires_approval(RiskLevel::R0),
        })
    }

    async fn execute(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionResult> {
        ctx.check_cancelled()?;
        let parsed: PreflightInput = parse_input(input, self.id())?;
        validate_hostname(&parsed.host)?;
        validate_port(parsed.port)?;
        require_caps(
            ctx,
            &[PluginCapability::HostRead, PluginCapability::SshExec],
        )
        .await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;

        // Host/port already validated; embed as literals (no metacharacters).
        let script = format!(
            "set +e; \
OS=$(. /etc/os-release 2>/dev/null; echo \"$ID\"); \
OSVER=$(. /etc/os-release 2>/dev/null; echo \"$VERSION_ID\"); \
ARCH=$(uname -m); \
PY=$(python3 --version 2>&1 | awk '{{print $2}}'); \
GIT=$(git --version 2>/dev/null | awk '{{print $3}}'); \
NGX=$(nginx -v 2>&1 | sed -n 's/.*nginx\\/\\([0-9.]*\\).*/\\1/p'); \
command -v systemctl >/dev/null 2>&1; SYS=$?; \
sudo -n true >/dev/null 2>&1; SUDO=$?; \
DISK=$(df -B1 / 2>/dev/null | awk 'NR==2 {{print $4}}'); \
MEM=$(awk '/MemTotal/ {{print $2*1024}}' /proc/meminfo 2>/dev/null); \
nc -z -w 3 {host} {port} >/dev/null 2>&1; PORT=$?; \
printf '%s\\n' \"$OS\" \"$OSVER\" \"$ARCH\" \"$PY\" \"$GIT\" \"$NGX\" \"$SYS\" \"$SUDO\" \"$DISK\" \"$MEM\" \"$PORT\"",
            host = parsed.host,
            port = parsed.port,
        );

        let command = RemoteCommand::freeform(script)
            .map_err(|_| PluginError::RemoteExecution("preflight script".into()))?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;

        let lines: Vec<&str> = result.stdout.lines().collect();
        let get = |i: usize| lines.get(i).copied().unwrap_or("").trim();
        let os = get(0);
        let os_version = get(1);
        let arch = get(2);
        let python = get(3);
        let git = get(4);
        let nginx = get(5);
        let systemd = get(6) == "0";
        let sudo = get(7) == "0";
        let disk_free: u64 = get(8).parse().unwrap_or(0);
        let memory_total: u64 = get(9).parse().unwrap_or(0);
        let port_available = get(10) != "0";
        let python_ok = version_gte(python, &parsed.min_python);
        let ready = !os.is_empty()
            && python_ok
            && !git.is_empty()
            && systemd
            && disk_free > 512 * 1024 * 1024
            && memory_total > 256 * 1024 * 1024
            && port_available;

        Ok(ActionResult::outputs_only(json!({
            "os": os,
            "osVersion": os_version,
            "arch": arch,
            "python": python,
            "git": git,
            "nginx": nginx,
            "systemd": systemd,
            "sudo": sudo,
            "diskFreeBytes": disk_free,
            "memoryTotalBytes": memory_total,
            "port": parsed.port,
            "portAvailable": port_available,
            "ready": ready,
        }))
        .sanitize())
    }
}

fn version_gte(have: &str, need: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.split('.')
            .filter_map(|p| {
                p.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .ok()
            })
            .collect()
    };
    let a = parse(have);
    let b = parse(need);
    if a.is_empty() {
        return false;
    }
    for i in 0..b.len().max(a.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(version_gte("3.12.3", "3.10"));
        assert!(!version_gte("3.9.1", "3.10"));
        assert!(version_gte("3.10.0", "3.10"));
    }

    #[test]
    fn preflight_is_r0() {
        assert_eq!(
            SystemPreflightReportAction
                .risk(&json!({"port": 8000}))
                .unwrap(),
            RiskLevel::R0
        );
    }
}
