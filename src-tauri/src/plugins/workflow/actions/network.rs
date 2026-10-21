use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::util::{
    deserialize_port, dynamic_program, parse_input, require_caps, require_session,
    validate_hostname, validate_port,
};
use super::{risk_requires_approval, ActionContext, ActionPlan, ActionResult, WorkflowAction};
use crate::plugins::error::PluginResult;
use crate::plugins::permissions::PluginCapability;
use crate::tools::RiskLevel;

pub struct NetworkTcpCheckAction;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TcpCheckInput {
    host: String,
    #[serde(deserialize_with = "deserialize_port")]
    port: u16,
}

#[async_trait]
impl WorkflowAction for NetworkTcpCheckAction {
    fn id(&self) -> &'static str {
        "network.tcp_check"
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
        let parsed: TcpCheckInput = parse_input(input, self.id())?;
        validate_hostname(&parsed.host)?;
        validate_port(parsed.port)?;
        require_caps(ctx, &[PluginCapability::HostRead]).await?;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk: RiskLevel::R0,
            summary: format!("TCP check {}:{}", parsed.host, parsed.port),
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
        let parsed: TcpCheckInput = parse_input(input, self.id())?;
        validate_hostname(&parsed.host)?;
        validate_port(parsed.port)?;
        require_caps(ctx, &[PluginCapability::HostRead]).await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;
        let command = dynamic_program(
            "nc".into(),
            vec![
                "-z".into(),
                "-w".into(),
                "3".into(),
                parsed.host.clone(),
                parsed.port.to_string(),
            ],
        )?;
        let result = sessions.exec(session_id, command).await.map_err(|err| {
            crate::plugins::error::PluginError::RemoteExecution(format!("{err:?}"))
        })?;
        let open = result.exit_code == 0;
        Ok(ActionResult {
            outputs: json!({
                "host": parsed.host,
                "port": parsed.port,
                "open": open,
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
    fn tcp_check_is_r0() {
        let action = NetworkTcpCheckAction;
        assert_eq!(
            action
                .risk(&json!({"host": "127.0.0.1", "port": 22}))
                .unwrap(),
            RiskLevel::R0
        );
    }

    #[test]
    fn rejects_bad_host() {
        assert!(validate_hostname("evil;host").is_err());
        assert!(validate_port(0).is_err());
    }
}
