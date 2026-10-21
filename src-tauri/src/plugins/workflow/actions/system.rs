use async_trait::async_trait;
use serde_json::json;

use super::util::require_caps;
use super::{risk_requires_approval, ActionContext, ActionPlan, ActionResult, WorkflowAction};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::tools::RiskLevel;

pub struct SystemDetectAction;

#[async_trait]
impl WorkflowAction for SystemDetectAction {
    fn id(&self) -> &'static str {
        "system.detect"
    }

    fn required_capabilities(
        &self,
        _input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        Ok(vec![PluginCapability::HostRead])
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        Ok(RiskLevel::R0)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        _input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        require_caps(ctx, &[PluginCapability::HostRead]).await?;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk: RiskLevel::R0,
            summary: "Detect host OS".into(),
            changes: Vec::new(),
            requires_approval: risk_requires_approval(RiskLevel::R0),
        })
    }

    async fn execute(
        &self,
        ctx: &ActionContext,
        _input: &serde_json::Value,
    ) -> PluginResult<ActionResult> {
        ctx.check_cancelled()?;
        require_caps(ctx, &[PluginCapability::HostRead]).await?;

        if let (Some(sessions), Some(session_id)) = (ctx.sessions.as_ref(), ctx.session_id) {
            let info = sessions.host_system_info(session_id).await;
            let dist = sessions
                .detect_os_distribution(session_id)
                .await
                .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
            let distribution = dist
                .map(|d| d.display_label().to_ascii_lowercase())
                .or_else(|| ctx.host_distribution.clone())
                .unwrap_or_else(|| "unknown".into());
            let os = if distribution.contains("mac") {
                "macos"
            } else if distribution.contains("bsd") {
                "bsd"
            } else {
                "linux"
            };
            return Ok(ActionResult::outputs_only(json!({
                "os": os,
                "distribution": distribution,
                "version": info.version_id.or_else(|| ctx.host_version.clone()).unwrap_or_default(),
                "arch": info.architecture.or_else(|| ctx.host_arch.clone()).unwrap_or_default(),
            }))
            .sanitize());
        }

        Ok(ActionResult::outputs_only(json!({
            "os": ctx.host_os.clone().unwrap_or_else(|| "unknown".into()),
            "distribution": ctx.host_distribution.clone().unwrap_or_else(|| "unknown".into()),
            "version": ctx.host_version.clone().unwrap_or_default(),
            "arch": ctx.host_arch.clone().unwrap_or_default(),
        })))
    }
}
