use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::{
    risk_requires_approval, ActionContext, ActionPlan, ActionResult, PlannedChange, WorkflowAction,
};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::tools::RiskLevel;

/// V1 stub: simple `{{ var }}` string replace over a package-relative template file.
/// Full Handlebars is intentionally not required for hello workflows.
pub struct TemplateRenderAction;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TemplateInput {
    template: String,
    #[serde(default)]
    destination: Option<String>,
    #[serde(default)]
    variables: serde_json::Value,
}

#[async_trait]
impl WorkflowAction for TemplateRenderAction {
    fn id(&self) -> &'static str {
        "template.render"
    }

    fn required_capabilities(
        &self,
        input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        let parsed: TemplateInput = parse(input)?;
        if parsed.destination.is_some() {
            Ok(vec![PluginCapability::RemoteFsWrite])
        } else {
            Ok(Vec::new())
        }
    }

    fn risk(&self, input: &serde_json::Value) -> PluginResult<RiskLevel> {
        let parsed: TemplateInput = parse(input)?;
        if parsed.destination.is_some() {
            Ok(RiskLevel::R2)
        } else {
            Ok(RiskLevel::R0)
        }
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed = parse(input)?;
        for cap in self.required_capabilities(input)? {
            ctx.permission_service.require(&ctx.plugin_id, cap).await?;
        }
        let risk = self.risk(input)?;
        let mut changes = Vec::new();
        let summary = if let Some(dest) = &parsed.destination {
            changes.push(PlannedChange::RemoteFileWrite { path: dest.clone() });
            format!("Render template {} → {dest}", parsed.template)
        } else {
            format!("Render template {}", parsed.template)
        };
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk,
            summary,
            changes,
            requires_approval: risk_requires_approval(risk),
        })
    }

    async fn execute(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionResult> {
        ctx.check_cancelled()?;
        let parsed = parse(input)?;
        for cap in self.required_capabilities(input)? {
            ctx.permission_service.require(&ctx.plugin_id, cap).await?;
        }

        let root = ctx.plugin_root.as_ref().ok_or_else(|| {
            PluginError::WorkflowValidation("plugin_root required for template.render".into())
        })?;
        let template_path = root.join(&parsed.template);
        if !template_path.starts_with(root) {
            return Err(PluginError::WorkflowValidation(
                "template path escapes plugin package".into(),
            ));
        }
        let raw = std::fs::read_to_string(&template_path).map_err(|_| {
            PluginError::WorkflowValidation(format!("template file not found: {}", parsed.template))
        })?;

        let rendered = simple_replace(&raw, &parsed.variables)?;

        if let Some(dest) = &parsed.destination {
            use super::util::{require_session, validate_remote_path};
            validate_remote_path(dest)?;
            if rendered.len() > 1024 * 1024 {
                return Err(PluginError::RemoteExecution(
                    "rendered template exceeds write size limit".into(),
                ));
            }
            let (sessions, session_id) = require_session(ctx, self.id())?;
            let sftp = sessions
                .sftp_channel(session_id)
                .await
                .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
            sftp.write_text(dest, &rendered)
                .await
                .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
            return Ok(ActionResult::outputs_only(json!({
                "destination": dest,
                "bytes": rendered.len(),
            }))
            .sanitize());
        }

        Ok(ActionResult::outputs_only(json!({
            "content": rendered,
        }))
        .sanitize())
    }
}

fn parse(input: &serde_json::Value) -> PluginResult<TemplateInput> {
    serde_json::from_value(input.clone()).map_err(|err| {
        PluginError::WorkflowValidation(format!("invalid template.render input: {err}"))
    })
}

fn simple_replace(template: &str, variables: &serde_json::Value) -> PluginResult<String> {
    let mut out = template.to_owned();
    let Some(obj) = variables.as_object() else {
        return Ok(out);
    };
    for (key, value) in obj {
        let needle = format!("{{{{ {key} }}}}");
        let needle_tight = format!("{{{{{key}}}}}");
        let replacement = match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => String::new(),
            other => other.to_string(),
        };
        out = out.replace(&needle, &replacement);
        out = out.replace(&needle_tight, &replacement);
    }
    Ok(out)
}
