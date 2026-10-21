use async_trait::async_trait;
use serde_json::json;

use super::util::{require_caps, require_session, sudo_program};
use super::{
    risk_requires_approval, ActionContext, ActionPlan, ActionResult, PlannedChange, WorkflowAction,
};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::plugins::workflow::ast::PrivilegeMode;
use crate::tools::RiskLevel;

pub struct NginxTestAction;
pub struct NginxReloadAction;

#[async_trait]
impl WorkflowAction for NginxTestAction {
    fn id(&self) -> &'static str {
        "nginx.test"
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
        // sudo raises floor to R2 for consistency with ssh.command classifier.
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
            summary: "nginx -t".into(),
            changes: vec![PlannedChange::RemoteCommand {
                executable: "nginx".into(),
                args: vec!["-t".into()],
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
        let (sessions, session_id) = require_session(ctx, self.id())?;
        let command = sudo_program("nginx", vec!["-t".into()])?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        Ok(ActionResult {
            outputs: json!({
                "ok": result.exit_code == 0,
            }),
            stdout: Some(result.stdout),
            stderr: Some(result.stderr),
            exit_code: Some(result.exit_code as i32),
        }
        .sanitize())
    }
}

#[async_trait]
impl WorkflowAction for NginxReloadAction {
    fn id(&self) -> &'static str {
        "nginx.reload"
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
        Ok(RiskLevel::R3)
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
        let risk = RiskLevel::R3;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk,
            summary: "systemctl reload nginx".into(),
            changes: vec![PlannedChange::RemoteCommand {
                executable: "systemctl".into(),
                args: vec!["reload".into(), "nginx".into()],
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
        let (sessions, session_id) = require_session(ctx, self.id())?;
        let command = sudo_program("systemctl", vec!["reload".into(), "nginx".into()])?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        if result.exit_code != 0 {
            return Err(PluginError::RemoteExecution(format!(
                "nginx reload failed with exit {}",
                result.exit_code
            )));
        }
        Ok(ActionResult {
            outputs: json!({ "reloaded": true }),
            stdout: Some(result.stdout),
            stderr: Some(result.stderr),
            exit_code: Some(result.exit_code as i32),
        }
        .sanitize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nginx_risks() {
        assert_eq!(NginxTestAction.risk(&json!({})).unwrap(), RiskLevel::R2);
        assert_eq!(NginxReloadAction.risk(&json!({})).unwrap(), RiskLevel::R3);
    }
}
