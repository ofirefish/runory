use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::util::{
    dynamic_program, parse_input, require_caps, require_session, validate_git_ref,
    validate_remote_path,
};
use super::{
    risk_requires_approval, ActionContext, ActionPlan, ActionResult, PlannedChange, WorkflowAction,
};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::plugins::workflow::ast::PrivilegeMode;
use crate::tools::RiskLevel;

pub struct GitFetchAction;
pub struct GitCheckoutAction;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PathInput {
    #[serde(alias = "cwd")]
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckoutInput {
    #[serde(alias = "cwd")]
    path: String,
    #[serde(alias = "ref")]
    reference: String,
}

#[async_trait]
impl WorkflowAction for GitFetchAction {
    fn id(&self) -> &'static str {
        "git.fetch"
    }

    fn required_capabilities(
        &self,
        _input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        Ok(vec![PluginCapability::SshExec])
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        Ok(RiskLevel::R2)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed: PathInput = parse_input(input, self.id())?;
        validate_remote_path(&parsed.path)?;
        require_caps(ctx, &[PluginCapability::SshExec]).await?;
        let risk = RiskLevel::R2;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk,
            summary: format!("git fetch in {}", parsed.path),
            changes: vec![PlannedChange::RemoteCommand {
                executable: "git".into(),
                args: vec!["-C".into(), parsed.path, "fetch".into(), "--".into()],
                privilege: PrivilegeMode::None,
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
        let parsed: PathInput = parse_input(input, self.id())?;
        validate_remote_path(&parsed.path)?;
        require_caps(ctx, &[PluginCapability::SshExec]).await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;
        let command = dynamic_program(
            "git".into(),
            vec![
                "-C".into(),
                parsed.path.clone(),
                "fetch".into(),
                "--".into(),
            ],
        )?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        if result.exit_code != 0 {
            return Err(PluginError::RemoteExecution(format!(
                "git fetch failed with exit {}",
                result.exit_code
            )));
        }
        Ok(ActionResult {
            outputs: json!({ "path": parsed.path }),
            stdout: Some(result.stdout),
            stderr: Some(result.stderr),
            exit_code: Some(result.exit_code as i32),
        }
        .sanitize())
    }
}

#[async_trait]
impl WorkflowAction for GitCheckoutAction {
    fn id(&self) -> &'static str {
        "git.checkout"
    }

    fn required_capabilities(
        &self,
        _input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        Ok(vec![PluginCapability::SshExec])
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        Ok(RiskLevel::R2)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed: CheckoutInput = parse_input(input, self.id())?;
        validate_remote_path(&parsed.path)?;
        validate_git_ref(&parsed.reference)?;
        require_caps(ctx, &[PluginCapability::SshExec]).await?;
        let risk = RiskLevel::R2;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk,
            summary: format!("git checkout {} in {}", parsed.reference, parsed.path),
            changes: vec![PlannedChange::RemoteCommand {
                executable: "git".into(),
                args: vec![
                    "-C".into(),
                    parsed.path,
                    "checkout".into(),
                    "--".into(),
                    parsed.reference,
                ],
                privilege: PrivilegeMode::None,
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
        let parsed: CheckoutInput = parse_input(input, self.id())?;
        validate_remote_path(&parsed.path)?;
        validate_git_ref(&parsed.reference)?;
        require_caps(ctx, &[PluginCapability::SshExec]).await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;
        let command = dynamic_program(
            "git".into(),
            vec![
                "-C".into(),
                parsed.path.clone(),
                "checkout".into(),
                "--".into(),
                parsed.reference.clone(),
            ],
        )?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        if result.exit_code != 0 {
            return Err(PluginError::RemoteExecution(format!(
                "git checkout failed with exit {}",
                result.exit_code
            )));
        }
        Ok(ActionResult {
            outputs: json!({
                "path": parsed.path,
                "ref": parsed.reference,
            }),
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
    fn fetch_and_checkout_are_r2() {
        assert_eq!(
            GitFetchAction.risk(&json!({"path": "/opt/app"})).unwrap(),
            RiskLevel::R2
        );
        assert_eq!(
            GitCheckoutAction
                .risk(&json!({"path": "/opt/app", "ref": "main"}))
                .unwrap(),
            RiskLevel::R2
        );
    }
}
