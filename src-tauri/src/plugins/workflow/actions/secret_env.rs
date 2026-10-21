//! Write remote env files from literal content or secretRef (never echo secrets).

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use zeroize::Zeroizing;

use super::util::{parse_input, require_session, validate_remote_path};
use super::{
    risk_requires_approval, ActionContext, ActionPlan, ActionResult, PlannedChange, WorkflowAction,
};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::plugins::secrets::{redact_secret_ref, validate_secret_ref_shape};
use crate::tools::RiskLevel;

const MAX_ENV_BYTES: usize = 256 * 1024;

pub struct RemoteFsWriteEnvAction;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteEnvInput {
    path: String,
    /// Literal env body. Mutually exclusive with secret_ref.
    #[serde(default)]
    content: Option<String>,
    /// Opaque reference only (e.g. secret://django/shop). Never logged as resolved.
    #[serde(default, alias = "secretRef")]
    secret_ref: Option<String>,
}

#[async_trait]
impl WorkflowAction for RemoteFsWriteEnvAction {
    fn id(&self) -> &'static str {
        "remote.fs.write_env"
    }

    fn required_capabilities(
        &self,
        input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        let parsed: WriteEnvInput = parse_input(input, self.id())?;
        let mut caps = vec![PluginCapability::RemoteFsWrite];
        if parsed
            .secret_ref
            .as_ref()
            .is_some_and(|r| !r.trim().is_empty())
        {
            caps.push(PluginCapability::SecretReference);
        }
        Ok(caps)
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        Ok(RiskLevel::R2)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed: WriteEnvInput = parse_input(input, self.id())?;
        validate_write_env(&parsed)?;
        for cap in self.required_capabilities(input)? {
            ctx.permission_service.require(&ctx.plugin_id, cap).await?;
        }
        let risk = RiskLevel::R2;
        let summary = if let Some(reference) = &parsed.secret_ref {
            format!(
                "Write env {} from {}",
                parsed.path,
                redact_secret_ref(reference)
            )
        } else {
            format!("Write env {}", parsed.path)
        };
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk,
            summary,
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
        let parsed: WriteEnvInput = parse_input(input, self.id())?;
        validate_write_env(&parsed)?;
        for cap in self.required_capabilities(input)? {
            ctx.permission_service.require(&ctx.plugin_id, cap).await?;
        }

        let body: Zeroizing<String> = if let Some(reference) = &parsed.secret_ref {
            let resolver = ctx.secret_resolver.as_ref().ok_or_else(|| {
                PluginError::WorkflowValidation(
                    "secretRef requires Core secret resolver (Vault binding)".into(),
                )
            })?;
            resolver.resolve(reference).await?
        } else {
            Zeroizing::new(parsed.content.clone().unwrap_or_default())
        };

        if body.len() > MAX_ENV_BYTES {
            return Err(PluginError::RemoteExecution(
                "env content exceeds size limit".into(),
            ));
        }

        let (sessions, session_id) = require_session(ctx, self.id())?;
        let sftp = sessions
            .sftp_channel(session_id)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        let (path, size) = sftp
            .write_text(&parsed.path, body.as_str())
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;

        // Never return secret body — only metadata.
        Ok(ActionResult::outputs_only(json!({
            "path": path,
            "bytes": size,
            "secretRefUsed": parsed.secret_ref.is_some(),
            "secretRef": parsed.secret_ref.as_ref().map(|r| redact_secret_ref(r)),
        }))
        .sanitize())
    }
}

fn validate_write_env(parsed: &WriteEnvInput) -> PluginResult<()> {
    validate_remote_path(&parsed.path)?;
    match (&parsed.content, &parsed.secret_ref) {
        (Some(c), None) if !c.is_empty() => Ok(()),
        (None, Some(r)) if !r.trim().is_empty() => {
            validate_secret_ref_shape(r)?;
            Ok(())
        }
        (Some(_), Some(_)) => Err(PluginError::WorkflowValidation(
            "provide either content or secretRef, not both".into(),
        )),
        _ => Err(PluginError::WorkflowValidation(
            "remote.fs.write_env requires content or secretRef".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::secrets::validate_secret_ref_shape;

    #[test]
    fn accepts_secret_ref_shape() {
        assert!(validate_secret_ref_shape("secret://django/shop-production").is_ok());
        assert!(validate_secret_ref_shape("vault://x").is_err());
        assert!(validate_secret_ref_shape("secret://").is_err());
    }

    #[test]
    fn redacts_ref() {
        let r = redact_secret_ref("secret://django/shop-production");
        assert!(!r.contains("shop-production"));
        assert!(r.starts_with("secret://djan"));
    }
}
