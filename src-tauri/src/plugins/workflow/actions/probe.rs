use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::util::{
    dynamic_program, is_safe_command_name, parse_input, require_caps, require_session,
};
use super::{risk_requires_approval, ActionContext, ActionPlan, ActionResult, WorkflowAction};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::tools::RiskLevel;

pub struct SystemCommandExistsAction;
pub struct SystemDiskAction;
pub struct SystemMemoryAction;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandExistsInput {
    command: String,
}

#[async_trait]
impl WorkflowAction for SystemCommandExistsAction {
    fn id(&self) -> &'static str {
        "system.command_exists"
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
        let parsed: CommandExistsInput = parse_input(input, self.id())?;
        if !is_safe_command_name(&parsed.command) {
            return Err(PluginError::WorkflowValidation(
                "command must be a safe program name".into(),
            ));
        }
        require_caps(ctx, &[PluginCapability::HostRead]).await?;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk: RiskLevel::R0,
            summary: format!("Check whether {} exists", parsed.command),
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
        let parsed: CommandExistsInput = parse_input(input, self.id())?;
        if !is_safe_command_name(&parsed.command) {
            return Err(PluginError::WorkflowValidation(
                "command must be a safe program name".into(),
            ));
        }
        require_caps(ctx, &[PluginCapability::HostRead]).await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;
        let command = dynamic_program("which".into(), vec![parsed.command.clone()])?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        let path = result.stdout.lines().next().unwrap_or("").trim().to_owned();
        let exists = result.exit_code == 0 && !path.is_empty();
        Ok(ActionResult {
            outputs: json!({
                "command": parsed.command,
                "exists": exists,
                "path": if exists { path } else { String::new() },
            }),
            stdout: Some(result.stdout),
            stderr: Some(result.stderr),
            exit_code: Some(result.exit_code as i32),
        }
        .sanitize())
    }
}

#[async_trait]
impl WorkflowAction for SystemDiskAction {
    fn id(&self) -> &'static str {
        "system.disk"
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
            summary: "Read free disk space for /".into(),
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
        let (sessions, session_id) = require_session(ctx, self.id())?;
        let command = dynamic_program("df".into(), vec!["-B1".into(), "/".into()])?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        if result.exit_code != 0 {
            return Err(PluginError::RemoteExecution(format!(
                "df failed with exit {}",
                result.exit_code
            )));
        }
        let free_bytes = parse_df_available(&result.stdout)
            .ok_or_else(|| PluginError::RemoteExecution("failed to parse df output".into()))?;
        Ok(ActionResult {
            outputs: json!({
                "path": "/",
                "freeBytes": free_bytes,
            }),
            stdout: Some(result.stdout),
            stderr: Some(result.stderr),
            exit_code: Some(result.exit_code as i32),
        }
        .sanitize())
    }
}

#[async_trait]
impl WorkflowAction for SystemMemoryAction {
    fn id(&self) -> &'static str {
        "system.memory"
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
            summary: "Read host memory statistics".into(),
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
        let (sessions, session_id) = require_session(ctx, self.id())?;
        let command = dynamic_program("free".into(), vec!["-b".into()])?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        if result.exit_code != 0 {
            return Err(PluginError::RemoteExecution(format!(
                "free failed with exit {}",
                result.exit_code
            )));
        }
        let (total, available) = parse_free_bytes(&result.stdout)
            .ok_or_else(|| PluginError::RemoteExecution("failed to parse free output".into()))?;
        Ok(ActionResult {
            outputs: json!({
                "totalBytes": total,
                "freeBytes": available,
            }),
            stdout: Some(result.stdout),
            stderr: Some(result.stderr),
            exit_code: Some(result.exit_code as i32),
        }
        .sanitize())
    }
}

fn parse_df_available(stdout: &str) -> Option<u64> {
    for line in stdout.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 4 {
            return cols[3].parse().ok();
        }
    }
    None
}

fn parse_free_bytes(stdout: &str) -> Option<(u64, u64)> {
    for line in stdout.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.first().is_some_and(|c| *c == "Mem:") && cols.len() >= 7 {
            let total = cols[1].parse().ok()?;
            let available = cols[6].parse().ok()?;
            return Some((total, available));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_df_available() {
        let out = "Filesystem     1B-blocks        Used  Available Use% Mounted on\n\
/dev/sda1     100000000000  25000000000 75000000000  25% /\n";
        assert_eq!(parse_df_available(out), Some(75_000_000_000));
    }

    #[test]
    fn parses_free_mem() {
        let out =
            "              total        used        free      shared  buff/cache   available\n\
Mem:     4294967296  1000000000  500000000   10000000  2794967296  3000000000\n";
        assert_eq!(parse_free_bytes(out), Some((4_294_967_296, 3_000_000_000)));
    }

    #[test]
    fn command_exists_is_r0() {
        assert_eq!(
            SystemCommandExistsAction
                .risk(&json!({"command": "python3"}))
                .unwrap(),
            RiskLevel::R0
        );
        assert!(!is_safe_command_name("foo;bar"));
    }
}
