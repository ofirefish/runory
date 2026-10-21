use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::util::{
    dynamic_program, is_safe_command_name, parse_input, path_under_base, require_caps,
    require_session, validate_remote_path,
};
use super::{
    risk_requires_approval, ActionContext, ActionPlan, ActionResult, PlannedChange, WorkflowAction,
};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::PluginCapability;
use crate::plugins::workflow::ast::PrivilegeMode;
use crate::tools::RiskLevel;

pub struct PythonDetectAction;
pub struct PythonVenvAction;
pub struct PythonPipInstallAction;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetectInput {
    #[serde(default = "default_python")]
    python: String,
}

fn default_python() -> String {
    "python3".into()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VenvInput {
    #[serde(default = "default_python")]
    python: String,
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PipInstallInput {
    #[serde(alias = "venv")]
    venv_path: String,
    requirements: RequirementsSpec,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RequirementsSpec {
    Path(String),
    File { file: String },
}

impl RequirementsSpec {
    fn path(&self) -> &str {
        match self {
            Self::Path(p) | Self::File { file: p } => p,
        }
    }
}

#[async_trait]
impl WorkflowAction for PythonDetectAction {
    fn id(&self) -> &'static str {
        "python.detect"
    }

    fn required_capabilities(
        &self,
        _input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        Ok(vec![PluginCapability::HostRead, PluginCapability::SshExec])
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        Ok(RiskLevel::R0)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed: DetectInput = parse_input(input, self.id())?;
        if !is_safe_command_name(&parsed.python) {
            return Err(PluginError::WorkflowValidation(
                "python must be a safe program name".into(),
            ));
        }
        require_caps(
            ctx,
            &[PluginCapability::HostRead, PluginCapability::SshExec],
        )
        .await?;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk: RiskLevel::R0,
            summary: format!("Detect {} version", parsed.python),
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
        let parsed: DetectInput = parse_input(input, self.id())?;
        if !is_safe_command_name(&parsed.python) {
            return Err(PluginError::WorkflowValidation(
                "python must be a safe program name".into(),
            ));
        }
        require_caps(
            ctx,
            &[PluginCapability::HostRead, PluginCapability::SshExec],
        )
        .await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;
        let command = dynamic_program(parsed.python.clone(), vec!["--version".into()])?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        let version_line = if result.stdout.trim().is_empty() {
            result.stderr.trim().to_owned()
        } else {
            result.stdout.trim().to_owned()
        };
        Ok(ActionResult {
            outputs: json!({
                "python": parsed.python,
                "version": version_line,
                "available": result.exit_code == 0,
            }),
            stdout: Some(result.stdout),
            stderr: Some(result.stderr),
            exit_code: Some(result.exit_code as i32),
        }
        .sanitize())
    }
}

#[async_trait]
impl WorkflowAction for PythonVenvAction {
    fn id(&self) -> &'static str {
        "python.venv"
    }

    fn required_capabilities(
        &self,
        _input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        Ok(vec![PluginCapability::SshExec])
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        Ok(RiskLevel::R2)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed: VenvInput = parse_input(input, self.id())?;
        validate_venv(&parsed)?;
        require_caps(ctx, &[PluginCapability::SshExec]).await?;
        let risk = RiskLevel::R2;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk,
            summary: format!("Create venv at {}", parsed.path),
            changes: vec![PlannedChange::RemoteCommand {
                executable: parsed.python.clone(),
                args: vec!["-m".into(), "venv".into(), parsed.path.clone()],
                privilege: PrivilegeMode::None,
            }],
            requires_approval: risk_requires_approval(risk),
        })
    }

    async fn execute(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionResult> {
        ctx.check_cancelled()?;
        let parsed: VenvInput = parse_input(input, self.id())?;
        validate_venv(&parsed)?;
        require_caps(ctx, &[PluginCapability::SshExec]).await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;
        let command = dynamic_program(
            parsed.python.clone(),
            vec!["-m".into(), "venv".into(), parsed.path.clone()],
        )?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        if result.exit_code != 0 {
            return Err(PluginError::RemoteExecution(format!(
                "python -m venv failed with exit {}",
                result.exit_code
            )));
        }
        Ok(ActionResult {
            outputs: json!({ "path": parsed.path }),
            stdout: Some(result.stdout),
            stderr: Some(result.stderr),
            exit_code: Some(result.exit_code as i32),
        }
        .sanitize())
    }
}

#[async_trait]
impl WorkflowAction for PythonPipInstallAction {
    fn id(&self) -> &'static str {
        "python.pip.install"
    }

    fn required_capabilities(
        &self,
        _input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        Ok(vec![PluginCapability::SshExec])
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        Ok(RiskLevel::R3)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        let parsed: PipInstallInput = parse_input(input, self.id())?;
        let python = venv_python(&parsed.venv_path)?;
        validate_remote_path(parsed.requirements.path())?;
        require_caps(ctx, &[PluginCapability::SshExec]).await?;
        let risk = RiskLevel::R3;
        let args = pip_args(parsed.requirements.path());
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk,
            summary: format!(
                "pip install -r {} via {}",
                parsed.requirements.path(),
                python
            ),
            changes: vec![PlannedChange::RemoteCommand {
                executable: "python3".into(),
                args,
                privilege: PrivilegeMode::None,
            }],
            requires_approval: risk_requires_approval(risk),
        })
    }

    async fn execute(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionResult> {
        ctx.check_cancelled()?;
        let parsed: PipInstallInput = parse_input(input, self.id())?;
        let python = venv_python(&parsed.venv_path)?;
        validate_remote_path(parsed.requirements.path())?;
        require_caps(ctx, &[PluginCapability::SshExec]).await?;
        let (sessions, session_id) = require_session(ctx, self.id())?;
        // Path executables are allowed by RemoteCommand::dynamic_program (Core is_safe_program).
        let command = dynamic_program(python.clone(), pip_args(parsed.requirements.path()))?;
        let result = sessions
            .exec(session_id, command)
            .await
            .map_err(|err| PluginError::RemoteExecution(format!("{err:?}")))?;
        if result.exit_code != 0 {
            return Err(PluginError::RemoteExecution(format!(
                "pip install failed with exit {}",
                result.exit_code
            )));
        }
        Ok(ActionResult {
            outputs: json!({
                "venvPath": parsed.venv_path,
                "requirements": parsed.requirements.path(),
            }),
            stdout: Some(result.stdout),
            stderr: Some(result.stderr),
            exit_code: Some(result.exit_code as i32),
        }
        .sanitize())
    }
}

fn validate_venv(parsed: &VenvInput) -> PluginResult<()> {
    if !is_safe_command_name(&parsed.python) {
        return Err(PluginError::WorkflowValidation(
            "python must be a safe program name".into(),
        ));
    }
    validate_remote_path(&parsed.path)
}

fn venv_python(venv_path: &str) -> PluginResult<String> {
    validate_remote_path(venv_path)?;
    let python = format!("{}/bin/python", venv_path.trim_end_matches('/'));
    path_under_base(venv_path.trim_end_matches('/'), &python)?;
    Ok(python)
}

fn pip_args(requirements: &str) -> Vec<String> {
    vec![
        "-m".into(),
        "pip".into(),
        "install".into(),
        "-r".into(),
        requirements.to_owned(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_is_r0_venv_r2_pip_r3() {
        assert_eq!(PythonDetectAction.risk(&json!({})).unwrap(), RiskLevel::R0);
        assert_eq!(
            PythonVenvAction
                .risk(&json!({"python": "python3", "path": "/opt/a/.venv"}))
                .unwrap(),
            RiskLevel::R2
        );
        assert_eq!(
            PythonPipInstallAction
                .risk(&json!({
                    "venvPath": "/opt/a/.venv",
                    "requirements": "/opt/a/requirements.txt"
                }))
                .unwrap(),
            RiskLevel::R3
        );
    }

    #[test]
    fn venv_python_must_stay_under_venv() {
        assert!(venv_python("/opt/app/.venv").is_ok());
        assert!(venv_python("/opt/../etc").is_err());
    }
}
