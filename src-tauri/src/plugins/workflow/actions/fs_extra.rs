use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::util::{
    dynamic_program, parse_input, require_caps, require_session, validate_remote_path,
};
use super::{
    risk_requires_approval, ActionContext, ActionPlan, ActionResult, PlannedChange, WorkflowAction,
};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::tools::RiskLevel;

pub struct RemoteFsMkdirAction;
pub struct RemoteFsSymlinkAction;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MkdirInput {
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SymlinkInput {
    target: String,
    #[serde(alias = "link")]
    link_path: String,
}

#[async_trait]
impl WorkflowAction for RemoteFsMkdirAction {
    fn id(&self) -> &'static str {
        "remote.fs.mkdir"
    }

    fn required_capabilities(
        &self,
        _input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        Ok(vec![PluginCapability::RemoteFsWrite])
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        Ok(RiskLevel::R1)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed: MkdirInput = parse_input(input, self.id())?;
        validate_remote_path(&parsed.path)?;
        require_caps(ctx, &[PluginCapability::RemoteFsWrite]).await?;
        let risk = RiskLevel::R1;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk,
            summary: format!("Create directory {}", parsed.path),
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
        let parsed: MkdirInput = parse_input(input, self.id())?;
        validate_remote_path(&parsed.path)?;
        require_caps(ctx, &[PluginCapability::RemoteFsWrite]).await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;
        let command = dynamic_program(
            "mkdir".into(),
            vec!["-p".into(), "--".into(), parsed.path.clone()],
        )?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        if result.exit_code != 0 {
            return Err(PluginError::RemoteExecution(format!(
                "mkdir failed with exit {}",
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
impl WorkflowAction for RemoteFsSymlinkAction {
    fn id(&self) -> &'static str {
        "remote.fs.symlink"
    }

    fn required_capabilities(
        &self,
        _input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        Ok(vec![PluginCapability::RemoteFsWrite])
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        Ok(RiskLevel::R2)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed: SymlinkInput = parse_input(input, self.id())?;
        validate_remote_path(&parsed.target)?;
        validate_remote_path(&parsed.link_path)?;
        require_caps(ctx, &[PluginCapability::RemoteFsWrite]).await?;
        let risk = RiskLevel::R2;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk,
            summary: format!("Symlink {} → {}", parsed.link_path, parsed.target),
            changes: vec![PlannedChange::RemoteFileWrite {
                path: parsed.link_path,
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
        let parsed: SymlinkInput = parse_input(input, self.id())?;
        validate_remote_path(&parsed.target)?;
        validate_remote_path(&parsed.link_path)?;
        require_caps(ctx, &[PluginCapability::RemoteFsWrite]).await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;
        let command = dynamic_program(
            "ln".into(),
            vec![
                "-sfn".into(),
                "--".into(),
                parsed.target.clone(),
                parsed.link_path.clone(),
            ],
        )?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        if result.exit_code != 0 {
            return Err(PluginError::RemoteExecution(format!(
                "ln -sfn failed with exit {}",
                result.exit_code
            )));
        }
        Ok(ActionResult {
            outputs: json!({
                "target": parsed.target,
                "linkPath": parsed.link_path,
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
    fn mkdir_r1_symlink_r2() {
        assert_eq!(
            RemoteFsMkdirAction
                .risk(&json!({"path": "/opt/app"}))
                .unwrap(),
            RiskLevel::R1
        );
        assert_eq!(
            RemoteFsSymlinkAction
                .risk(&json!({
                    "target": "/opt/app/releases/1",
                    "linkPath": "/opt/app/current"
                }))
                .unwrap(),
            RiskLevel::R2
        );
    }
}
