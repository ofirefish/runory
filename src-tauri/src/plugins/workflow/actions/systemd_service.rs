use async_trait::async_trait;

use super::systemd::{exec_systemctl, planned_remote, ServiceInput};
use super::util::{parse_input, require_caps, validate_service_name};
use super::{risk_requires_approval, ActionContext, ActionPlan, ActionResult, WorkflowAction};
use crate::plugins::error::PluginResult;
use crate::plugins::permissions::PluginCapability;
use crate::tools::RiskLevel;

pub struct SystemdEnableAction;
pub struct SystemdRestartAction;
pub struct SystemdReloadAction;

macro_rules! systemd_service_action {
    ($name:ident, $id:expr, $verb:expr, $risk:expr) => {
        #[async_trait]
        impl WorkflowAction for $name {
            fn id(&self) -> &'static str {
                $id
            }

            fn required_capabilities(
                &self,
                _input: &serde_json::Value,
            ) -> PluginResult<Vec<PluginCapability>> {
                Ok(vec![
                    PluginCapability::SshExec,
                    PluginCapability::SshExecSudo,
                ])
            }

            fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
                Ok($risk)
            }

            async fn plan(
                &self,
                ctx: &ActionContext,
                input: &serde_json::Value,
            ) -> PluginResult<ActionPlan> {
                let parsed: ServiceInput = parse_input(input, self.id())?;
                validate_service_name(&parsed.service)?;
                require_caps(
                    ctx,
                    &[PluginCapability::SshExec, PluginCapability::SshExecSudo],
                )
                .await?;
                let risk = $risk;
                Ok(ActionPlan {
                    action_id: self.id().into(),
                    risk,
                    summary: format!("systemctl {} {}", $verb, parsed.service),
                    changes: planned_remote($verb, &parsed.service),
                    requires_approval: risk_requires_approval(risk),
                })
            }

            async fn execute(
                &self,
                ctx: &ActionContext,
                input: &serde_json::Value,
            ) -> PluginResult<ActionResult> {
                ctx.check_cancelled()?;
                let parsed: ServiceInput = parse_input(input, self.id())?;
                validate_service_name(&parsed.service)?;
                require_caps(
                    ctx,
                    &[PluginCapability::SshExec, PluginCapability::SshExecSudo],
                )
                .await?;
                exec_systemctl(ctx, self.id(), vec![$verb.into(), parsed.service.clone()]).await
            }
        }
    };
}

systemd_service_action!(
    SystemdEnableAction,
    "systemd.enable",
    "enable",
    RiskLevel::R2
);
systemd_service_action!(
    SystemdRestartAction,
    "systemd.restart",
    "restart",
    RiskLevel::R3
);
systemd_service_action!(
    SystemdReloadAction,
    "systemd.reload",
    "reload",
    RiskLevel::R2
);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn restart_is_r3() {
        assert_eq!(
            SystemdRestartAction
                .risk(&json!({"service": "app.service"}))
                .unwrap(),
            RiskLevel::R3
        );
        assert!(validate_service_name("app;rm").is_err());
    }
}
