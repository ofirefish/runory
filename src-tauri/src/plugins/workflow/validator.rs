use std::collections::HashSet;

use super::ast::WorkflowDefinition;
use crate::plugins::error::{PluginError, PluginResult};

const SUPPORTED_API_VERSION: &str = "runory/v1";
const SUPPORTED_KIND: &str = "Workflow";
const MAX_RETRY_ATTEMPTS: u32 = 5;

const ALLOWED_ACTIONS: &[&str] = &[
    "system.detect",
    "system.command_exists",
    "system.disk",
    "system.memory",
    "system.preflight_report",
    "network.tcp_check",
    "ssh.command",
    "remote.fs.read",
    "remote.fs.write",
    "remote.fs.write_env",
    "remote.fs.mkdir",
    "remote.fs.symlink",
    "template.render",
    "git.clone",
    "git.rev_parse",
    "git.fetch",
    "git.checkout",
    "python.detect",
    "python.venv",
    "python.pip.install",
    "systemd.install",
    "systemd.daemon_reload",
    "systemd.enable",
    "systemd.restart",
    "systemd.reload",
    "systemd.status",
    "nginx.test",
    "nginx.reload",
    "http.check",
    "database.migration",
    "changeset.checkpoint",
    "release.activate",
    #[cfg(test)]
    "test.sleep",
];

pub fn validate_workflow(definition: &WorkflowDefinition) -> PluginResult<()> {
    if definition.api_version != SUPPORTED_API_VERSION {
        return Err(PluginError::WorkflowValidation(format!(
            "unsupported apiVersion: {}",
            definition.api_version
        )));
    }
    if definition.kind != SUPPORTED_KIND {
        return Err(PluginError::WorkflowValidation(format!(
            "unsupported kind: {}",
            definition.kind
        )));
    }
    if definition.metadata.id.trim().is_empty() {
        return Err(PluginError::WorkflowValidation(
            "metadata.id must be non-empty".into(),
        ));
    }
    if definition.metadata.name.trim().is_empty() {
        return Err(PluginError::WorkflowValidation(
            "metadata.name must be non-empty".into(),
        ));
    }
    if definition.steps.is_empty() {
        return Err(PluginError::WorkflowValidation(
            "workflow must contain at least one step".into(),
        ));
    }

    let mut step_ids = HashSet::new();
    for step in &definition.steps {
        if step.id.trim().is_empty() {
            return Err(PluginError::WorkflowValidation(
                "step id must be non-empty".into(),
            ));
        }
        if !step_ids.insert(step.id.clone()) {
            return Err(PluginError::WorkflowValidation(format!(
                "duplicate step id: {}",
                step.id
            )));
        }
        if !is_allowed_action(&step.uses) {
            return Err(PluginError::WorkflowValidation(format!(
                "unknown action: {}",
                step.uses
            )));
        }
        if let Some(retry) = &step.retry {
            if retry.max_attempts == 0 {
                return Err(PluginError::WorkflowValidation(format!(
                    "step {}: maxAttempts must be >= 1",
                    step.id
                )));
            }
            if retry.max_attempts > MAX_RETRY_ATTEMPTS {
                return Err(PluginError::WorkflowValidation(format!(
                    "step {}: maxAttempts must be <= {MAX_RETRY_ATTEMPTS}",
                    step.id
                )));
            }
        }
        if let Some(timeout) = &step.timeout {
            if timeout.trim().is_empty() {
                return Err(PluginError::WorkflowValidation(format!(
                    "step {}: timeout must be non-empty when set",
                    step.id
                )));
            }
        }
    }

    Ok(())
}

pub fn is_allowed_action(action_id: &str) -> bool {
    ALLOWED_ACTIONS.contains(&action_id) || (cfg!(test) && action_id == "test.sleep")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::workflow::parser::parse_workflow_yaml;

    fn sample() -> WorkflowDefinition {
        parse_workflow_yaml(
            r#"
apiVersion: runory/v1
kind: Workflow
metadata:
  id: hello.inspect
  name: Inspect
steps:
  - id: detect
    uses: system.detect
"#,
        )
        .expect("parse")
    }

    #[test]
    fn accepts_valid_workflow() {
        validate_workflow(&sample()).expect("valid");
    }

    #[test]
    fn rejects_empty_steps() {
        let mut def = sample();
        def.steps.clear();
        assert!(validate_workflow(&def).is_err());
    }

    #[test]
    fn rejects_unknown_action() {
        let mut def = sample();
        def.steps[0].uses = "evil.shell".into();
        assert!(validate_workflow(&def).is_err());
    }

    #[test]
    fn rejects_high_retry() {
        let mut def = sample();
        def.steps[0].retry = Some(crate::plugins::workflow::ast::RetryPolicy {
            max_attempts: 6,
            delay: None,
        });
        assert!(validate_workflow(&def).is_err());
    }

    #[test]
    fn rejects_wrong_api_version() {
        let mut def = sample();
        def.api_version = "v2".into();
        assert!(validate_workflow(&def).is_err());
    }
}
