use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::util::{parse_input, require_caps, require_session, sudo_program, validate_remote_path};
use super::{
    risk_requires_approval, ActionContext, ActionPlan, ActionResult, PlannedChange, WorkflowAction,
};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::plugins::workflow::ast::PrivilegeMode;
use crate::tools::RiskLevel;

const MAX_UNIT_BYTES: usize = 256 * 1024;

pub struct SystemdInstallAction;
pub struct SystemdDaemonReloadAction;
pub struct SystemdStatusAction;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallInput {
    path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServiceInput {
    pub service: String,
}

#[async_trait]
impl WorkflowAction for SystemdInstallAction {
    fn id(&self) -> &'static str {
        "systemd.install"
    }

    fn required_capabilities(
        &self,
        _input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        Ok(vec![
            PluginCapability::RemoteFsWrite,
            PluginCapability::SshExecSudo,
        ])
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        Ok(RiskLevel::R3)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed: InstallInput = parse_input(input, self.id())?;
        validate_unit(&parsed)?;
        require_caps(
            ctx,
            &[
                PluginCapability::RemoteFsWrite,
                PluginCapability::SshExecSudo,
            ],
        )
        .await?;
        let risk = RiskLevel::R3;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk,
            summary: format!("Install systemd unit {}", parsed.path),
            changes: vec![PlannedChange::RemoteFileWrite { path: parsed.path }],
            requires_approval: risk_requires_approval(risk),
        })
    }

    async fn execute(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionResult> {
        ctx.check_cancelled()?;
        let parsed: InstallInput = parse_input(input, self.id())?;
        validate_unit(&parsed)?;
        require_caps(
            ctx,
            &[
                PluginCapability::RemoteFsWrite,
                PluginCapability::SshExecSudo,
            ],
        )
        .await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;
        let sftp = sessions
            .sftp_channel(session_id)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        let (path, size) = sftp
            .write_text(&parsed.path, &parsed.content)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        Ok(ActionResult::outputs_only(json!({
            "path": path,
            "size": size,
        })))
    }
}

#[async_trait]
impl WorkflowAction for SystemdDaemonReloadAction {
    fn id(&self) -> &'static str {
        "systemd.daemon_reload"
    }

    fn required_capabilities(
        &self,
        _input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        Ok(vec![
            PluginCapability::SshExec,
            PluginCapability::SshExecSudo,
        ])
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        Ok(RiskLevel::R2)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        _input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        require_caps(
            ctx,
            &[PluginCapability::SshExec, PluginCapability::SshExecSudo],
        )
        .await?;
        let risk = RiskLevel::R2;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk,
            summary: "systemctl daemon-reload".into(),
            changes: vec![PlannedChange::RemoteCommand {
                executable: "systemctl".into(),
                args: vec!["daemon-reload".into()],
                privilege: PrivilegeMode::Sudo,
            }],
            requires_approval: risk_requires_approval(risk),
        })
    }

    async fn execute(
        &self,
        ctx: &ActionContext,
        _input: &serde_json::Value,
    ) -> PluginResult<ActionResult> {
        ctx.check_cancelled()?;
        require_caps(
            ctx,
            &[PluginCapability::SshExec, PluginCapability::SshExecSudo],
        )
        .await?;
        exec_systemctl(ctx, self.id(), vec!["daemon-reload".into()]).await
    }
}

#[async_trait]
impl WorkflowAction for SystemdStatusAction {
    fn id(&self) -> &'static str {
        "systemd.status"
    }

    fn required_capabilities(
        &self,
        _input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        Ok(vec![PluginCapability::SshExec])
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        Ok(RiskLevel::R0)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed: ServiceInput = parse_input(input, self.id())?;
        super::util::validate_service_name(&parsed.service)?;
        require_caps(ctx, &[PluginCapability::SshExec]).await?;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk: RiskLevel::R0,
            summary: format!("systemctl status {}", parsed.service),
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
        let parsed: ServiceInput = parse_input(input, self.id())?;
        super::util::validate_service_name(&parsed.service)?;
        require_caps(ctx, &[PluginCapability::SshExec]).await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;
        let command = super::util::dynamic_program(
            "systemctl".into(),
            vec!["status".into(), "--".into(), parsed.service.clone()],
        )?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        Ok(ActionResult {
            outputs: json!({
                "service": parsed.service,
                "active": result.exit_code == 0,
            }),
            stdout: Some(result.stdout),
            stderr: Some(result.stderr),
            exit_code: Some(result.exit_code as i32),
        }
        .sanitize())
    }
}

fn validate_unit(parsed: &InstallInput) -> PluginResult<()> {
    validate_remote_path(&parsed.path)?;
    if !parsed.path.ends_with(".service")
        && !parsed.path.ends_with(".socket")
        && !parsed.path.ends_with(".timer")
    {
        return Err(PluginError::WorkflowValidation(
            "systemd unit path must end with .service, .socket, or .timer".into(),
        ));
    }
    if parsed.content.len() > MAX_UNIT_BYTES || parsed.content.contains('\0') {
        return Err(PluginError::WorkflowValidation(
            "unit content invalid or too large".into(),
        ));
    }
    Ok(())
}

pub(crate) async fn exec_systemctl(
    ctx: &ActionContext,
    action: &str,
    args: Vec<String>,
) -> PluginResult<ActionResult> {
    let (sessions, session_id) = require_session(ctx, action)?;
    let command = sudo_program("systemctl", args.clone())?;
    let result = sessions
        .exec(session_id, command)
        .await
        .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
    if result.exit_code != 0 && action != "systemd.status" {
        return Err(PluginError::RemoteExecution(format!(
            "systemctl {:?} failed with exit {}",
            args, result.exit_code
        )));
    }
    Ok(ActionResult {
        outputs: json!({ "args": args }),
        stdout: Some(result.stdout),
        stderr: Some(result.stderr),
        exit_code: Some(result.exit_code as i32),
    }
    .sanitize())
}

pub(crate) fn planned_remote(verb: &str, service: &str) -> Vec<PlannedChange> {
    if verb == "restart" {
        return vec![PlannedChange::ServiceRestart {
            service: service.into(),
        }];
    }
    vec![PlannedChange::RemoteCommand {
        executable: "systemctl".into(),
        args: vec![verb.into(), service.into()],
        privilege: PrivilegeMode::Sudo,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_reload_r2_status_r0() {
        assert_eq!(
            SystemdDaemonReloadAction.risk(&json!({})).unwrap(),
            RiskLevel::R2
        );
        assert_eq!(
            SystemdStatusAction
                .risk(&json!({"service": "app.service"}))
                .unwrap(),
            RiskLevel::R0
        );
    }
}
