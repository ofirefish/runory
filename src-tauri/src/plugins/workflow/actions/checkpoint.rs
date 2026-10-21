//! Deployment checkpoint — record previous `current` symlink before switch.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::util::{
    dynamic_program, parse_input, require_caps, require_session, validate_remote_path,
};
use super::{risk_requires_approval, ActionContext, ActionPlan, ActionResult, WorkflowAction};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::tools::RiskLevel;

pub struct ChangesetCheckpointAction;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckpointInput {
    /// Path to the `current` symlink (e.g. /opt/runory/apps/shop/current).
    current_link: String,
    #[serde(default)]
    service: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[async_trait]
impl WorkflowAction for ChangesetCheckpointAction {
    fn id(&self) -> &'static str {
        "changeset.checkpoint"
    }

    fn required_capabilities(
        &self,
        _input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        Ok(vec![
            PluginCapability::SshExec,
            PluginCapability::RemoteFsRead,
        ])
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        Ok(RiskLevel::R0)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed: CheckpointInput = parse_input(input, self.id())?;
        validate_remote_path(&parsed.current_link)?;
        require_caps(
            ctx,
            &[PluginCapability::SshExec, PluginCapability::RemoteFsRead],
        )
        .await?;
        let name = parsed.name.unwrap_or_else(|| "checkpoint".into());
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk: RiskLevel::R0,
            summary: format!("Checkpoint {name} at {}", parsed.current_link),
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
        let parsed: CheckpointInput = parse_input(input, self.id())?;
        validate_remote_path(&parsed.current_link)?;
        require_caps(
            ctx,
            &[PluginCapability::SshExec, PluginCapability::RemoteFsRead],
        )
        .await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;

        // Resolve symlink target without following into a shell-injected path.
        let command = dynamic_program(
            "readlink".into(),
            vec!["-f".into(), "--".into(), parsed.current_link.clone()],
        )?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;

        let previous_target = result.stdout.trim().to_owned();
        let previous_release_id = previous_target.rsplit('/').next().unwrap_or("").to_owned();

        Ok(ActionResult::outputs_only(json!({
            "name": parsed.name.unwrap_or_else(|| "checkpoint".into()),
            "currentLink": parsed.current_link,
            "previousTarget": previous_target,
            "previousReleaseId": previous_release_id,
            "service": parsed.service,
            "switched": false,
        }))
        .sanitize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_is_r0() {
        assert_eq!(
            ChangesetCheckpointAction
                .risk(&json!({"currentLink": "/opt/runory/apps/x/current"}))
                .unwrap(),
            RiskLevel::R0
        );
    }
}
