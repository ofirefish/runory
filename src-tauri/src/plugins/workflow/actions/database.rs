use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::util::{
    dynamic_program, freeform_quoted, parse_input, require_caps, require_session, shell_quote,
    validate_remote_path,
};
use super::{ActionContext, ActionPlan, ActionResult, PlannedChange, WorkflowAction};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::tools::RiskLevel;

pub struct DatabaseMigrationAction;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MigrationInput {
    /// Prefer a venv python path or bare `python3`.
    executable: String,
    #[serde(default = "default_migrate_args")]
    args: Vec<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

fn default_migrate_args() -> Vec<String> {
    vec!["manage.py".into(), "migrate".into()]
}

#[async_trait]
impl WorkflowAction for DatabaseMigrationAction {
    fn id(&self) -> &'static str {
        "database.migration"
    }

    fn required_capabilities(
        &self,
        _input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        Ok(vec![PluginCapability::SshExec])
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        // Always R4 — never auto-rollback reverse migrations.
        Ok(RiskLevel::R4)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed: MigrationInput = parse_input(input, self.id())?;
        validate_migration(&parsed)?;
        require_caps(ctx, &[PluginCapability::SshExec]).await?;
        let risk = RiskLevel::R4;
        let description = parsed
            .description
            .clone()
            .unwrap_or_else(|| format!("{} {}", parsed.executable, parsed.args.join(" ")));
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk,
            summary: format!("Database migration: {description}"),
            changes: vec![PlannedChange::DatabaseMigration { description }],
            // Always true regardless of risk helper — migrations are fail-closed.
            requires_approval: true,
        })
    }

    async fn execute(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionResult> {
        ctx.check_cancelled()?;
        let parsed: MigrationInput = parse_input(input, self.id())?;
        validate_migration(&parsed)?;
        require_caps(ctx, &[PluginCapability::SshExec]).await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;

        let command = if let Some(cwd) = &parsed.cwd {
            let mut script = format!("cd {} && ", shell_quote(cwd)?);
            script.push_str(&shell_quote(&parsed.executable)?);
            for arg in &parsed.args {
                script.push(' ');
                script.push_str(&shell_quote(arg)?);
            }
            crate::ssh::RemoteCommand::freeform(script)
                .map_err(|_| PluginError::RemoteExecution("invalid migration command".into()))?
        } else if parsed.executable.contains('/') {
            dynamic_program(parsed.executable.clone(), parsed.args.clone())?
        } else {
            freeform_quoted(
                &parsed.executable,
                &parsed.args.iter().map(String::as_str).collect::<Vec<_>>(),
            )?
        };

        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        if result.exit_code != 0 {
            return Err(PluginError::RemoteExecution(format!(
                "database migration failed with exit {}",
                result.exit_code
            )));
        }
        Ok(ActionResult {
            outputs: json!({
                "migrated": true,
                // Explicit: rollback of migrations is never automatic.
                "rollbackSupported": false,
            }),
            stdout: Some(result.stdout),
            stderr: Some(result.stderr),
            exit_code: Some(result.exit_code as i32),
        }
        .sanitize())
    }
}

fn validate_migration(parsed: &MigrationInput) -> PluginResult<()> {
    if parsed.executable.trim().is_empty() || parsed.executable.contains('\0') {
        return Err(PluginError::WorkflowValidation(
            "invalid migration executable".into(),
        ));
    }
    if parsed.executable.contains("..") {
        return Err(PluginError::WorkflowValidation(
            "migration executable must not contain ..".into(),
        ));
    }
    if let Some(cwd) = &parsed.cwd {
        validate_remote_path(cwd)?;
    }
    if parsed.args.is_empty() {
        return Err(PluginError::WorkflowValidation(
            "migration args must not be empty".into(),
        ));
    }
    for arg in &parsed.args {
        if arg.contains('\0') || arg.len() > 4096 {
            return Err(PluginError::WorkflowValidation(
                "invalid migration argument".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::risk_requires_approval;
    use super::*;

    #[test]
    fn always_r4_and_requires_approval_flag() {
        let action = DatabaseMigrationAction;
        assert_eq!(
            action
                .risk(&json!({
                    "executable": "/opt/app/.venv/bin/python",
                    "args": ["manage.py", "migrate"],
                    "cwd": "/opt/app"
                }))
                .unwrap(),
            RiskLevel::R4
        );
        assert!(risk_requires_approval(RiskLevel::R4));
    }

    #[test]
    fn rejects_path_escape_in_executable() {
        assert!(validate_migration(&MigrationInput {
            executable: "/opt/../etc/passwd".into(),
            args: default_migrate_args(),
            cwd: None,
            description: None,
        })
        .is_err());
    }
}
