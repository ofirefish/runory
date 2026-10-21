use super::ast::WorkflowDefinition;
use crate::plugins::error::{PluginError, PluginResult};

pub fn parse_workflow_yaml(yaml: &str) -> PluginResult<WorkflowDefinition> {
    serde_yaml::from_str(yaml).map_err(|err| {
        PluginError::WorkflowValidation(format!("failed to parse workflow YAML: {err}"))
    })
}

pub fn parse_workflow_bytes(bytes: &[u8]) -> PluginResult<WorkflowDefinition> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| PluginError::WorkflowValidation("workflow YAML is not valid UTF-8".into()))?;
    parse_workflow_yaml(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hello_workflow() {
        let yaml = r#"
apiVersion: runory/v1
kind: Workflow
metadata:
  id: hello.inspect
  name: Inspect Linux Host
steps:
  - id: detect
    uses: system.detect
  - id: uname
    uses: ssh.command
    with:
      executable: uname
      args:
        - -a
"#;
        let def = parse_workflow_yaml(yaml).expect("parse");
        assert_eq!(def.api_version, "runory/v1");
        assert_eq!(def.kind, "Workflow");
        assert_eq!(def.steps.len(), 2);
        assert_eq!(def.steps[1].uses, "ssh.command");
    }

    #[test]
    fn renames_if_to_condition() {
        let yaml = r#"
apiVersion: runory/v1
kind: Workflow
metadata:
  id: cond
  name: Cond
steps:
  - id: a
    uses: system.detect
    if: host.os == "linux"
"#;
        let def = parse_workflow_yaml(yaml).expect("parse");
        assert_eq!(
            def.steps[0].condition.as_deref(),
            Some("host.os == \"linux\"")
        );
    }
}
