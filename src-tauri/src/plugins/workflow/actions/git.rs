use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::util::{
    dynamic_program, parse_input, require_caps, require_session, validate_git_ref,
    validate_git_repository, validate_remote_path,
};
use super::{
    risk_requires_approval, ActionContext, ActionPlan, ActionResult, PlannedChange, WorkflowAction,
};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::plugins::workflow::ast::PrivilegeMode;
use crate::tools::RiskLevel;

pub struct GitCloneAction;
pub struct GitRevParseAction;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloneInput {
    repository: String,
    branch: String,
    destination: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PathInput {
    #[serde(alias = "cwd")]
    path: String,
}

#[async_trait]
impl WorkflowAction for GitCloneAction {
    fn id(&self) -> &'static str {
        "git.clone"
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
        let parsed: CloneInput = parse_input(input, self.id())?;
        validate_clone(&parsed)?;
        require_caps(ctx, &[PluginCapability::SshExec]).await?;
        let risk = RiskLevel::R2;
        let args = clone_args(&parsed);
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk,
            summary: format!(
                "Clone {} ({}) → {}",
                parsed.repository, parsed.branch, parsed.destination
            ),
            changes: vec![PlannedChange::RemoteCommand {
                executable: "git".into(),
                args,
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
        let parsed: CloneInput = parse_input(input, self.id())?;
        validate_clone(&parsed)?;
        require_caps(ctx, &[PluginCapability::SshExec]).await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;
        let command = dynamic_program("git".into(), clone_args(&parsed))?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        if result.exit_code != 0 {
            return Err(PluginError::RemoteExecution(format!(
                "git clone failed with exit {}",
                result.exit_code
            )));
        }
        Ok(ActionResult {
            outputs: json!({
                "destination": parsed.destination,
                "branch": parsed.branch,
            }),
            stdout: Some(result.stdout),
            stderr: Some(result.stderr),
            exit_code: Some(result.exit_code as i32),
        }
        .sanitize())
    }
}

#[async_trait]
impl WorkflowAction for GitRevParseAction {
    fn id(&self) -> &'static str {
        "git.rev_parse"
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
        let parsed: PathInput = parse_input(input, self.id())?;
        validate_remote_path(&parsed.path)?;
        require_caps(ctx, &[PluginCapability::SshExec]).await?;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk: RiskLevel::R0,
            summary: format!("Resolve HEAD in {}", parsed.path),
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
        let parsed: PathInput = parse_input(input, self.id())?;
        validate_remote_path(&parsed.path)?;
        require_caps(ctx, &[PluginCapability::SshExec]).await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;
        let command = dynamic_program(
            "git".into(),
            vec![
                "-C".into(),
                parsed.path.clone(),
                "rev-parse".into(),
                "HEAD".into(),
            ],
        )?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        if result.exit_code != 0 {
            return Err(PluginError::RemoteExecution(format!(
                "git rev-parse failed with exit {}",
                result.exit_code
            )));
        }
        let sha = result.stdout.lines().next().unwrap_or("").trim().to_owned();
        if sha.len() < 7 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(PluginError::RemoteExecution(
                "git rev-parse returned invalid sha".into(),
            ));
        }
        Ok(ActionResult {
            outputs: json!({ "sha": sha }),
            stdout: Some(result.stdout),
            stderr: Some(result.stderr),
            exit_code: Some(result.exit_code as i32),
        }
        .sanitize())
    }
}

fn validate_clone(parsed: &CloneInput) -> PluginResult<()> {
    validate_git_repository(&parsed.repository)?;
    validate_git_ref(&parsed.branch)?;
    validate_remote_path(&parsed.destination)?;
    Ok(())
}

fn clone_args(parsed: &CloneInput) -> Vec<String> {
    vec![
        "clone".into(),
        "--branch".into(),
        parsed.branch.clone(),
        "--".into(),
        parsed.repository.clone(),
        parsed.destination.clone(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clone_is_r2_rev_parse_r0() {
        assert_eq!(
            GitCloneAction
                .risk(&json!({
                    "repository": "https://example.com/r.git",
                    "branch": "main",
                    "destination": "/opt/app/releases/1"
                }))
                .unwrap(),
            RiskLevel::R2
        );
        assert_eq!(
            GitRevParseAction
                .risk(&json!({"path": "/opt/app"}))
                .unwrap(),
            RiskLevel::R0
        );
        assert!(validate_git_repository("https://x$(id)").is_err());
    }
}
