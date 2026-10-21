use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::{
    risk_requires_approval, ActionContext, ActionPlan, ActionResult, PlannedChange, WorkflowAction,
};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::tools::RiskLevel;

const MAX_READ_BYTES: usize = 1024 * 1024;
const MAX_WRITE_BYTES: usize = 1024 * 1024;

pub struct RemoteFsReadAction;
pub struct RemoteFsWriteAction;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadInput {
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteInput {
    path: String,
    content: String,
    #[serde(default)]
    mode: Option<String>,
}

#[async_trait]
impl WorkflowAction for RemoteFsReadAction {
    fn id(&self) -> &'static str {
        "remote.fs.read"
    }

    fn required_capabilities(
        &self,
        _input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        Ok(vec![PluginCapability::RemoteFsRead])
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        Ok(RiskLevel::R0)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed: ReadInput = parse(input, "remote.fs.read")?;
        validate_remote_path(&parsed.path)?;
        ctx.permission_service
            .require(&ctx.plugin_id, PluginCapability::RemoteFsRead)
            .await?;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk: RiskLevel::R0,
            summary: format!("Read {}", parsed.path),
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
        let parsed: ReadInput = parse(input, "remote.fs.read")?;
        validate_remote_path(&parsed.path)?;
        ctx.permission_service
            .require(&ctx.plugin_id, PluginCapability::RemoteFsRead)
            .await?;

        let (sessions, session_id) = require_session(ctx)?;
        let sftp = sessions
            .sftp_channel(session_id)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        let (path, content) = sftp
            .read_text(&parsed.path)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        if content.len() > MAX_READ_BYTES {
            return Err(PluginError::RemoteExecution(
                "remote file exceeds read size limit".into(),
            ));
        }
        let size = content.len();
        Ok(ActionResult::outputs_only(json!({
            "path": path,
            "content": content,
            "size": size,
        }))
        .sanitize())
    }
}

#[async_trait]
impl WorkflowAction for RemoteFsWriteAction {
    fn id(&self) -> &'static str {
        "remote.fs.write"
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
        let parsed: WriteInput = parse(input, "remote.fs.write")?;
        validate_remote_path(&parsed.path)?;
        if parsed.content.len() > MAX_WRITE_BYTES {
            return Err(PluginError::WorkflowValidation(
                "write content exceeds size limit".into(),
            ));
        }
        let _ = parsed.mode;
        ctx.permission_service
            .require(&ctx.plugin_id, PluginCapability::RemoteFsWrite)
            .await?;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk: RiskLevel::R2,
            summary: format!("Create or replace {}", parsed.path),
            changes: vec![PlannedChange::RemoteFileWrite { path: parsed.path }],
            requires_approval: risk_requires_approval(RiskLevel::R2),
        })
    }

    async fn execute(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionResult> {
        ctx.check_cancelled()?;
        let parsed: WriteInput = parse(input, "remote.fs.write")?;
        validate_remote_path(&parsed.path)?;
        if parsed.content.len() > MAX_WRITE_BYTES || parsed.content.contains('\0') {
            return Err(PluginError::WorkflowValidation(
                "write content invalid or too large".into(),
            ));
        }
        ctx.permission_service
            .require(&ctx.plugin_id, PluginCapability::RemoteFsWrite)
            .await?;

        let (sessions, session_id) = require_session(ctx)?;
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

fn require_session(
    ctx: &ActionContext,
) -> PluginResult<(
    &std::sync::Arc<crate::ssh::ServerSessionManager>,
    crate::domain::SessionId,
)> {
    match (ctx.sessions.as_ref(), ctx.session_id) {
        (Some(s), Some(id)) => Ok((s, id)),
        _ => Err(PluginError::RemoteExecution(
            "remote.fs actions require an active server session with SFTP".into(),
        )),
    }
}

fn parse<T: for<'de> Deserialize<'de>>(input: &serde_json::Value, action: &str) -> PluginResult<T> {
    serde_json::from_value(input.clone())
        .map_err(|err| PluginError::WorkflowValidation(format!("invalid {action} input: {err}")))
}

fn validate_remote_path(path: &str) -> PluginResult<()> {
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
