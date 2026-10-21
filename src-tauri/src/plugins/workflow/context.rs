use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Runtime context for a workflow run. Never store secret plaintext here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowContext {
    pub run_id: String,
    pub plugin_id: String,
    pub plugin_version: String,
    pub workflow_id: String,
    pub host_id: Option<String>,
    pub host_os: Option<String>,
    #[serde(default)]
    pub inputs: serde_json::Value,
    #[serde(default)]
    pub step_outputs: HashMap<String, serde_json::Value>,
}

impl WorkflowContext {
    pub fn new(
        run_id: impl Into<String>,
        plugin_id: impl Into<String>,
        plugin_version: impl Into<String>,
        workflow_id: impl Into<String>,
        host_id: Option<String>,
        host_os: Option<String>,
        inputs: serde_json::Value,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            plugin_id: plugin_id.into(),
            plugin_version: plugin_version.into(),
            workflow_id: workflow_id.into(),
            host_id,
            host_os,
            inputs,
            step_outputs: HashMap::new(),
        }
    }

    pub fn record_step_outputs(&mut self, step_id: impl Into<String>, outputs: serde_json::Value) {
        self.step_outputs.insert(step_id.into(), outputs);
    }
}
