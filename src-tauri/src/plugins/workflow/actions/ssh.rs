use std::collections::HashMap;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::{
    risk_requires_approval, ActionContext, ActionPlan, ActionResult, PlannedChange, WorkflowAction,
};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::plugins::workflow::ast::{EnvValue, PrivilegeMode};
use crate::ssh::RemoteCommand;
use crate::tools::RiskLevel;

pub struct SshCommandAction;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshCommandInput {
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, EnvValue>,
    #[serde(default)]
    pub privilege: PrivilegeMode,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[async_trait]
impl WorkflowAction for SshCommandAction {
    fn id(&self) -> &'static str {
        "ssh.command"
    }

    fn required_capabilities(
        &self,
        input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        let parsed: SshCommandInput = parse_input(input)?;
        let mut caps = vec![PluginCapability::SshExec];
        if matches!(parsed.privilege, PrivilegeMode::Sudo) {
            caps.push(PluginCapability::SshExecSudo);
        }
        Ok(caps)
    }

    fn risk(&self, input: &serde_json::Value) -> PluginResult<RiskLevel> {
        let parsed: SshCommandInput = parse_input(input)?;
        Ok(classify_ssh_command_risk(
            &parsed.executable,
            &parsed.args,
            parsed.privilege,
        ))
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed = parse_input(input)?;
        for cap in self.required_capabilities(input)? {
            ctx.permission_service.require(&ctx.plugin_id, cap).await?;
        }
        // Validate executable early so plan fails closed.
        let _ = build_remote_command(
            &parsed.executable,
            &parsed.args,
            parsed.privilege,
            parsed.cwd.as_deref(),
            &parsed.env,
        )?;
        let risk = classify_ssh_command_risk(&parsed.executable, &parsed.args, parsed.privilege);
        let summary = format!("Run {} {}", parsed.executable, parsed.args.join(" "));
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk,
            summary: summary.trim().to_owned(),
            changes: vec![PlannedChange::RemoteCommand {
                executable: parsed.executable,
                args: parsed.args,
                privilege: parsed.privilege,
            }],
            requires_approval: risk_requires_approval(risk),
        })
    }

    async fn execute(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionResult> {
        ctx.check_cancelled()?;
        let parsed = parse_input(input)?;
        for cap in self.required_capabilities(input)? {
            ctx.permission_service.require(&ctx.plugin_id, cap).await?;
        }

        let (sessions, session_id) = match (ctx.sessions.as_ref(), ctx.session_id) {
            (Some(s), Some(id)) => (s, id),
            _ => {
                return Err(PluginError::RemoteExecution(
                    "ssh.command requires an active server session".into(),
                ));
            }
        };

        let command = build_remote_command(
            &parsed.executable,
            &parsed.args,
            parsed.privilege,
            parsed.cwd.as_deref(),
            &parsed.env,
        )?;

        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;

        Ok(ActionResult {
            outputs: json!({
                "exitCode": result.exit_code,
            }),
            stdout: Some(result.stdout),
            stderr: Some(result.stderr),
            exit_code: Some(result.exit_code as i32),
        }
        .sanitize())
    }
}

fn parse_input(input: &serde_json::Value) -> PluginResult<SshCommandInput> {
    serde_json::from_value(input.clone())
        .map_err(|err| PluginError::WorkflowValidation(format!("invalid ssh.command input: {err}")))
}

/// Deterministic risk classifier for `ssh.command` (Core-owned).
pub fn classify_ssh_command_risk(
    executable: &str,
    args: &[String],
    privilege: PrivilegeMode,
) -> RiskLevel {
    let exe = executable.to_ascii_lowercase();
    let joined = args
        .iter()
        .map(|a| a.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ");

    let destructive = ["rm", "mkfs", "dd", "userdel", "passwd", "wipefs", "shred"];
    let network_fw = ["iptables", "nft", "nftables", "ufw"];
    let package_mgr = ["apt", "apt-get", "dnf", "yum", "pacman", "zypper", "apk"];
    let readonly = [
        "uname",
        "whoami",
        "pwd",
        "id",
        "hostname",
        "date",
        "uptime",
        "cat",
        "ls",
        "echo",
        "printf",
        "true",
        "false",
        "test",
        "which",
        "command",
        "python3",
        "python",
        "git",
        "pip",
        "pip3",
        "journalctl",
        "curl",
        "nc",
    ];

    let version_probe = args
        .iter()
        .any(|a| matches!(a.as_str(), "--version" | "-V" | "-v" | "--help" | "-h"));

    let mut risk = if destructive.iter().any(|d| exe == *d)
        || joined.contains("mkfs")
        || (exe == "rm" || joined.split_whitespace().any(|t| t == "rm"))
    {
        RiskLevel::R4
    } else if network_fw.iter().any(|d| exe == *d) {
        RiskLevel::R4
    } else if package_mgr.iter().any(|d| exe == *d)
        && args.iter().any(|a| {
            matches!(
                a.as_str(),
                "install" | "remove" | "purge" | "upgrade" | "dist-upgrade"
            )
        })
    {
        RiskLevel::R3
    } else if exe == "systemctl" {
        if args
            .iter()
            .any(|a| a == "restart" || a == "stop" || a == "disable")
        {
            RiskLevel::R3
        } else if args
            .iter()
            .any(|a| a == "reload" || a == "start" || a == "enable")
        {
            RiskLevel::R2
        } else {
            // status / show / is-active / cat / --version
            RiskLevel::R0
        }
    } else if exe == "mkdir" || exe == "touch" || exe == "cp" || exe == "mv" || exe == "chmod" {
        RiskLevel::R1
    } else if exe == "nginx" {
        if args
            .iter()
            .any(|a| a == "-t" || a == "-T" || a == "-v" || a == "-V")
        {
            RiskLevel::R0
        } else if args.iter().any(|a| a == "-s") {
            RiskLevel::R2
        } else {
            RiskLevel::R3
        }
    } else if exe == "docker" {
        classify_docker_risk(args, version_probe)
    } else if version_probe
        && (readonly.iter().any(|d| exe == *d)
            || exe.starts_with("python")
            || exe == "git"
            || exe == "nginx")
    {
        RiskLevel::R0
    } else if readonly.iter().any(|d| exe == *d) || exe.starts_with("python") {
        RiskLevel::R0
    } else {
        // Unknown command: conservative.
        RiskLevel::R3
    };

    if matches!(privilege, PrivilegeMode::Sudo) {
        risk = risk.max(RiskLevel::R2);
        if matches!(risk, RiskLevel::R0 | RiskLevel::R1) {
            risk = RiskLevel::R2;
        }
        if !readonly.iter().any(|d| exe == *d) && !matches!(risk, RiskLevel::R4) {
            risk = risk.max(RiskLevel::R3);
        }
    }

    risk
}

/// Core-owned Docker CLI risk mapping for Plugin API 1 / PP2 validation plugin.
fn classify_docker_risk(args: &[String], version_probe: bool) -> RiskLevel {
    if version_probe && args.iter().all(|a| a.starts_with('-')) {
        return RiskLevel::R0;
    }
    let sub = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map(|s| s.as_str())
        .unwrap_or("");
    match sub {
        "version" | "ps" | "inspect" | "logs" | "images" | "info" => RiskLevel::R0,
        "start" => RiskLevel::R2,
        "stop" | "restart" | "pull" | "kill" => RiskLevel::R3,
        "rm" | "rmi" => RiskLevel::R4,
        // Unknown docker subcommand: conservative write risk.
        _ => RiskLevel::R3,
    }
}

/// Build a `RemoteCommand` for a validated executable.
///
/// Prefers `RemoteCommand::dynamic_program` (owned program name + argv). When
/// `cwd` or `env` are set, falls back to Core-owned POSIX-quoted `freeform`
/// because those require a shell wrapper.
pub fn build_remote_command(
    executable: &str,
    args: &[String],
    privilege: PrivilegeMode,
    cwd: Option<&str>,
    env: &HashMap<String, EnvValue>,
) -> PluginResult<RemoteCommand> {
    if !is_safe_program(executable) {
        return Err(PluginError::WorkflowValidation(format!(
            "unsafe executable name: {executable}"
        )));
    }
    if let Some(dir) = cwd {
        if dir.contains('\0') || dir.len() > 4096 {
            return Err(PluginError::WorkflowValidation("invalid cwd".into()));
        }
    }

    let needs_shell = cwd.is_some() || !env.is_empty();
    if !needs_shell {
        let command = match privilege {
            PrivilegeMode::None => {
                RemoteCommand::dynamic_program(executable.to_owned(), args.to_vec())
                    .map_err(|_| PluginError::RemoteExecution("invalid executable".into()))?
            }
            PrivilegeMode::Sudo => {
                let mut sudo_args = vec!["--".into(), executable.to_owned()];
                sudo_args.extend(args.iter().cloned());
                RemoteCommand::dynamic_program("sudo".into(), sudo_args)
                    .map_err(|_| PluginError::RemoteExecution("invalid sudo command".into()))?
            }
        };
        return Ok(command);
    }

    let mut script = String::new();
    for (key, value) in env {
        if !is_safe_env_key(key) {
            return Err(PluginError::WorkflowValidation(format!(
                "unsafe env key: {key}"
            )));
        }
        match value {
            EnvValue::Literal(v) | EnvValue::Value { value: v } => {
                script.push_str(&format!("{}={} ", key, shell_quote(v)?));
            }
            EnvValue::SecretRef { .. } => {
                return Err(PluginError::WorkflowValidation(
                    "secretRef env values are not resolved in V1 ssh.command".into(),
                ));
            }
        }
    }

    if let Some(dir) = cwd {
        script.push_str(&format!("cd {} && ", shell_quote(dir)?));
    }

    if matches!(privilege, PrivilegeMode::Sudo) {
        script.push_str("sudo -- ");
    }

    script.push_str(&shell_quote(executable)?);
    for arg in args {
        script.push(' ');
        script.push_str(&shell_quote(arg)?);
    }

    RemoteCommand::freeform(script)
        .map_err(|_| PluginError::RemoteExecution("failed to build remote command".into()))
}

fn is_safe_program(program: &str) -> bool {
    if program.is_empty() || program.contains('\0') || program.len() > 4096 {
        return false;
    }
    if program.contains("..") {
        return false;
    }
    // Bare command name or absolute unix path (no shell metacharacters).
    if program.starts_with('/') {
        return program
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/'));
    }
    !program.contains('/')
        && !program.contains('\\')
        && program
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn is_safe_env_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

fn shell_quote(value: &str) -> PluginResult<String> {
    if value.contains('\0') || value.len() > 16 * 1024 {
        return Err(PluginError::WorkflowValidation(
            "argument too large or contains NUL".into(),
        ));
    }
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uname_is_r0() {
        assert_eq!(
            classify_ssh_command_risk("uname", &["-a".into()], PrivilegeMode::None),
            RiskLevel::R0
        );
    }

    #[test]
    fn rm_is_r4() {
        assert_eq!(
            classify_ssh_command_risk("rm", &["-rf".into(), "/tmp/x".into()], PrivilegeMode::None),
            RiskLevel::R4
        );
    }

    #[test]
    fn rejects_unsafe_program() {
        assert!(
            build_remote_command("sh;id", &[], PrivilegeMode::None, None, &HashMap::new()).is_err()
        );
    }

    #[test]
    fn builds_quoted_freeform() {
        let cmd = build_remote_command(
            "uname",
            &["-a".into()],
            PrivilegeMode::None,
            None,
            &HashMap::new(),
        )
        .expect("build");
        // Smoke: construction succeeds; render happens inside ExecChannel.
        let _ = cmd;
    }

    #[test]
    fn nginx_probe_keeps_conservative_sudo_risk() {
        assert_eq!(
            classify_ssh_command_risk("nginx", &["-t".into()], PrivilegeMode::None),
            RiskLevel::R0
        );
        assert_eq!(
            classify_ssh_command_risk("nginx", &["-t".into()], PrivilegeMode::Sudo),
            RiskLevel::R3
        );
    }

    #[test]
    fn docker_subcommand_risks_match_api_1() {
        assert_eq!(
            classify_ssh_command_risk("docker", &["version".into()], PrivilegeMode::None),
            RiskLevel::R0
        );
        assert_eq!(
            classify_ssh_command_risk("docker", &["ps".into(), "-a".into()], PrivilegeMode::None),
            RiskLevel::R0
        );
        assert_eq!(
            classify_ssh_command_risk(
                "docker",
                &["inspect".into(), "abc".into()],
                PrivilegeMode::None
            ),
            RiskLevel::R0
        );
        assert_eq!(
            classify_ssh_command_risk(
                "docker",
                &["logs".into(), "--tail".into(), "100".into(), "abc".into()],
                PrivilegeMode::None
            ),
            RiskLevel::R0
        );
        assert_eq!(
            classify_ssh_command_risk(
                "docker",
                &["start".into(), "abc".into()],
                PrivilegeMode::None
            ),
            RiskLevel::R2
        );
        assert_eq!(
            classify_ssh_command_risk(
                "docker",
                &["stop".into(), "abc".into()],
                PrivilegeMode::None
            ),
            RiskLevel::R3
        );
        assert_eq!(
            classify_ssh_command_risk(
                "docker",
                &["restart".into(), "abc".into()],
                PrivilegeMode::None
            ),
            RiskLevel::R3
        );
        assert_eq!(
            classify_ssh_command_risk(
                "docker",
                &["pull".into(), "nginx:alpine".into()],
                PrivilegeMode::None
            ),
            RiskLevel::R3
        );
        assert_eq!(
            classify_ssh_command_risk("docker", &["rm".into(), "abc".into()], PrivilegeMode::None),
            RiskLevel::R4
        );
    }

    #[test]
    fn systemctl_reload_nginx_is_r2_or_higher_with_sudo() {
        assert_eq!(
            classify_ssh_command_risk(
                "systemctl",
                &["reload".into(), "nginx".into()],
                PrivilegeMode::None
            ),
            RiskLevel::R2
        );
        assert!(
            classify_ssh_command_risk(
                "systemctl",
                &["reload".into(), "nginx".into()],
                PrivilegeMode::Sudo
            ) >= RiskLevel::R2
        );
    }

    #[test]
    fn sudo_requires_sudo_capability_in_plan_capabilities() {
        let action = SshCommandAction;
        let caps = action
            .required_capabilities(&serde_json::json!({
                "executable": "nginx",
                "args": ["-t"],
                "privilege": "sudo"
            }))
            .expect("caps");
        assert!(caps.contains(&PluginCapability::SshExec));
        assert!(caps.contains(&PluginCapability::SshExecSudo));
    }

    #[test]
    fn secret_ref_env_rejected() {
        let mut env = HashMap::new();
        env.insert(
            "TOKEN".into(),
            EnvValue::SecretRef {
                secret_ref: "vault://x".into(),
            },
        );
        assert!(build_remote_command("uname", &[], PrivilegeMode::None, None, &env).is_err());
    }
}
