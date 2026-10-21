use serde::{Deserialize, Serialize};

/// Content-free plugin audit event. Never include secrets, keys, or passwords.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAuditEvent {
    pub plugin_id: String,
    pub plugin_version: String,
    pub workflow: Option<String>,
    pub run_id: Option<String>,
    pub step_id: Option<String>,
    pub action: String,
}

pub fn log_plugin_audit(
    plugin_id: &str,
    plugin_version: &str,
    workflow: Option<&str>,
    run_id: Option<&str>,
    step_id: Option<&str>,
    action: &str,
) {
    log_plugin_audit_with_host(
        plugin_id,
        plugin_version,
        workflow,
        run_id,
        step_id,
        None,
        action,
    );
}

pub fn log_plugin_audit_with_host(
    plugin_id: &str,
    plugin_version: &str,
    workflow: Option<&str>,
    run_id: Option<&str>,
    step_id: Option<&str>,
    host_id: Option<&str>,
    action: &str,
) {
    let event = PluginAuditEvent {
        plugin_id: plugin_id.to_owned(),
        plugin_version: plugin_version.to_owned(),
        workflow: workflow.map(str::to_owned),
        run_id: run_id.map(str::to_owned),
        step_id: step_id.map(str::to_owned),
        action: action.to_owned(),
    };
    tracing::info!(
        target: "runory.plugins.audit",
        plugin_id = %event.plugin_id,
        plugin_version = %event.plugin_version,
        workflow = event.workflow.as_deref().unwrap_or(""),
        run_id = event.run_id.as_deref().unwrap_or(""),
        step_id = event.step_id.as_deref().unwrap_or(""),
        host_id = host_id.unwrap_or(""),
        action = %event.action,
        "plugin audit"
    );
}
