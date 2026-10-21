use serde::{Deserialize, Serialize};

use super::capability::PluginCapability;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionGrant {
    pub plugin_id: String,
    pub capability: PluginCapability,
    pub granted: bool,
    pub hosts: Vec<String>,
}
