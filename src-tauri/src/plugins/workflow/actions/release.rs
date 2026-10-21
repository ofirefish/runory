//! Activate a release with health check; on failure restore previous symlink target.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::util::{
    dynamic_program, parse_input, require_caps, require_session, sudo_program, validate_http_url,
    validate_remote_path, validate_service_name,
};
use super::{
    risk_requires_approval, ActionContext, ActionPlan, ActionResult, PlannedChange, WorkflowAction,
};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::tools::RiskLevel;

pub struct ReleaseActivateAction;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivateInput {
    current_link: String,
    release_dir: String,
    /// Previous symlink target from changeset.checkpoint (may be empty on first deploy).
    #[serde(default)]
    previous_target: String,
    service: String,
    health_url: String,
    #[serde(default)]
    migration_ran: bool,
}

#[async_trait]
impl WorkflowAction for ReleaseActivateAction {
    fn id(&self) -> &'static str {
        "release.activate"
    }

    fn required_capabilities(
        &self,
        _input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        Ok(vec![
            PluginCapability::RemoteFsWrite,
            PluginCapability::SshExec,
            PluginCapability::SshExecSudo,
            PluginCapability::HostRead,
        ])
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        Ok(RiskLevel::R3)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed: ActivateInput = parse_input(input, self.id())?;
        validate_inputs(&parsed)?;
        require_caps(
            ctx,
            &[
                PluginCapability::RemoteFsWrite,
                PluginCapability::SshExec,
                PluginCapability::SshExecSudo,
                PluginCapability::HostRead,
            ],
        )
        .await?;
        let risk = RiskLevel::R3;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk,
            summary: format!(
                "Activate {} → {} (rollback to previous on health failure)",
                parsed.release_dir, parsed.current_link
            ),
            changes: vec![
                PlannedChange::RemoteFileWrite {
                    path: parsed.current_link.clone(),
                },
                PlannedChange::ServiceRestart {
                    service: parsed.service.clone(),
                },
            ],
            requires_approval: risk_requires_approval(risk),
        })
    }

    async fn execute(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionResult> {
        ctx.check_cancelled()?;
        let parsed: ActivateInput = parse_input(input, self.id())?;
        validate_inputs(&parsed)?;
        require_caps(
            ctx,
            &[
                PluginCapability::RemoteFsWrite,
                PluginCapability::SshExec,
                PluginCapability::SshExecSudo,
                PluginCapability::HostRead,
            ],
        )
        .await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;

        switch_current(
            &sessions,
            session_id,
            &parsed.release_dir,
            &parsed.current_link,
        )
        .await?;

        reload_and_restart(&sessions, session_id, &parsed.service).await?;

        if health_ok(&sessions, session_id, &parsed.health_url).await? {
            return Ok(ActionResult::outputs_only(json!({
                "activated": true,
                "rolledBack": false,
                "currentTarget": parsed.release_dir,
                "migrationWarning": parsed.migration_ran,
            }))
            .sanitize());
        }

        // Health failed after switch — restore previous release if known.
        if !parsed.previous_target.is_empty() {
            switch_current(
                &sessions,
                session_id,
                &parsed.previous_target,
                &parsed.current_link,
            )
            .await?;
            let _ = reload_and_restart(&sessions, session_id, &parsed.service).await;
            let _ = health_ok(&sessions, session_id, &parsed.health_url).await;
            return Err(PluginError::RemoteExecution(
                "health failed after switch; restored previous release; database migration was not reversed"
                    .into(),
            ));
        }

        Err(PluginError::RemoteExecution(
            "health failed after switch; no previous target to restore; database migration was not reversed"
                .into(),
        ))
    }
}

fn validate_inputs(parsed: &ActivateInput) -> PluginResult<()> {
    validate_remote_path(&parsed.current_link)?;
    validate_remote_path(&parsed.release_dir)?;
    if !parsed.previous_target.is_empty() {
        validate_remote_path(&parsed.previous_target)?;
    }
    validate_service_name(&parsed.service)?;
    validate_http_url(&parsed.health_url)?;
    Ok(())
}

async fn switch_current(
    sessions: &std::sync::Arc<crate::ssh::ServerSessionManager>,
    session_id: crate::domain::SessionId,
    target: &str,
    link: &str,
) -> PluginResult<()> {
    let command = dynamic_program(
        "ln".into(),
        vec![
            "-sfn".into(),
            "--".into(),
            target.to_owned(),
            link.to_owned(),
        ],
    )?;
    let result = sessions
        .exec(session_id, command)
        .await
        .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
    if result.exit_code != 0 {
        return Err(PluginError::RemoteExecution(format!(
            "symlink failed with exit {}",
            result.exit_code
        )));
    }
    Ok(())
}

async fn reload_and_restart(
    sessions: &std::sync::Arc<crate::ssh::ServerSessionManager>,
    session_id: crate::domain::SessionId,
    service: &str,
) -> PluginResult<()> {
    for args in [
        vec!["daemon-reload".into()],
        vec!["enable".into(), "--".into(), service.to_owned()],
        vec!["restart".into(), "--".into(), service.to_owned()],
    ] {
        let command = sudo_program("systemctl", args)?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        if result.exit_code != 0 {
            return Err(PluginError::RemoteExecution(format!(
                "systemctl failed with exit {}",
                result.exit_code
            )));
        }
    }
    // Best-effort nginx reload after successful service restart.
    let nginx = sudo_program(
        "systemctl",
        vec!["reload".into(), "--".into(), "nginx".into()],
    )?;
    let _ = sessions.exec(session_id, nginx).await;
    Ok(())
}

async fn health_ok(
    sessions: &std::sync::Arc<crate::ssh::ServerSessionManager>,
    session_id: crate::domain::SessionId,
    url: &str,
) -> PluginResult<bool> {
    let command = dynamic_program(
        "curl".into(),
        vec![
            "-sf".into(),
            "-o".into(),
            "/dev/null".into(),
            "-w".into(),
            "%{http_code}".into(),
            "--".into(),
            url.to_owned(),
        ],
    )?;
    let result = sessions
        .exec(session_id, command)
        .await
        .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
    let code: u16 = result.stdout.trim().parse().unwrap_or(0);
    Ok(result.exit_code == 0 && (200..400).contains(&code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activate_is_r3() {
        assert_eq!(
            ReleaseActivateAction
                .risk(&json!({
                    "currentLink": "/opt/a/current",
                    "releaseDir": "/opt/a/releases/1",
                    "service": "a",
                    "healthUrl": "http://127.0.0.1:8000/health/"
                }))
                .unwrap(),
            RiskLevel::R3
        );
    }
}
