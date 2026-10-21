use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::util::{dynamic_program, parse_input, require_caps, require_session, validate_http_url};
use super::{risk_requires_approval, ActionContext, ActionPlan, ActionResult, WorkflowAction};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::tools::RiskLevel;

pub struct HttpCheckAction;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpCheckInput {
    url: String,
}

#[async_trait]
impl WorkflowAction for HttpCheckAction {
    fn id(&self) -> &'static str {
        "http.check"
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
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed: HttpCheckInput = parse_input(input, self.id())?;
        validate_http_url(&parsed.url)?;
        require_caps(ctx, &[PluginCapability::HostRead]).await?;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk: RiskLevel::R0,
            summary: format!("HTTP check {}", parsed.url),
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
        let parsed: HttpCheckInput = parse_input(input, self.id())?;
        validate_http_url(&parsed.url)?;
        require_caps(ctx, &[PluginCapability::HostRead]).await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;
        let command = dynamic_program(
            "curl".into(),
            vec![
                "-sf".into(),
                "-o".into(),
                "/dev/null".into(),
                "-w".into(),
                "%{http_code}".into(),
                "--".into(),
                parsed.url.clone(),
            ],
        )?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        let code_str = result.stdout.trim();
        let status_code: u16 = code_str.parse().unwrap_or(0);
        let ok = result.exit_code == 0 && (200..400).contains(&status_code);
        Ok(ActionResult {
            outputs: json!({
                "url": parsed.url,
                "statusCode": status_code,
                "ok": ok,
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
    fn http_check_is_r0() {
        assert_eq!(
            HttpCheckAction
                .risk(&json!({"url": "https://example.com/"}))
                .unwrap(),
            RiskLevel::R0
        );
        assert!(validate_http_url("https://ok.example/health").is_ok());
        assert!(validate_http_url("javascript:alert(1)").is_err());
    }
}
