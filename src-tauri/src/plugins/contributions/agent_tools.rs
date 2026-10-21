//! Dynamic Agent Tool contributions from enabled plugins (PP9).

use serde::Serialize;

use crate::plugins::error::PluginResult;
use crate::plugins::manifest::model::AgentToolContribution;
use crate::plugins::registry::PluginRegistryService;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAgentToolDto {
    pub plugin_id: String,
    pub plugin_version: String,
    pub name: String,
    pub description: String,
    pub workflow: String,
    pub risk: String,
    pub requires_approval: bool,
}

pub async fn list_enabled_plugin_agent_tools(
    registry: &PluginRegistryService,
) -> PluginResult<Vec<PluginAgentToolDto>> {
    let installed = registry.list_installed().await?;
    let mut out = Vec::new();
    for plugin in installed.into_iter().filter(|p| p.enabled) {
        // Fail closed on revoked plugins.
        if registry
            .repository()
            .is_revoked("plugin", &plugin.id, None)
            .unwrap_or(false)
            || registry
                .repository()
                .get_plugin_security_status(&plugin.id, &plugin.version)
                .ok()
                .flatten()
                .as_deref()
                == Some("revoked")
        {
            continue;
        }
        let Ok((_record, manifest)) = registry.require_enabled(&plugin.id) else {
            continue;
        };
        for tool in manifest.contributes.agent_tools {
            out.push(to_dto(&plugin.id, &plugin.version, tool));
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn to_dto(
    plugin_id: &str,
    plugin_version: &str,
    tool: AgentToolContribution,
) -> PluginAgentToolDto {
    PluginAgentToolDto {
        plugin_id: plugin_id.to_owned(),
        plugin_version: plugin_version.to_owned(),
        name: tool.name,
        description: tool.description,
        workflow: tool.workflow,
        risk: format!("{:?}", tool.risk),
        requires_approval: tool.requires_approval,
    }
}
