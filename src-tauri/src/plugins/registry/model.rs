use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPluginRecord {
    pub id: String,
    pub version: String,
    pub name: String,
    pub publisher_id: String,
    pub install_path: String,
    pub enabled: bool,
    pub installed_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDto {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher_id: String,
    pub enabled: bool,
    pub description: Option<String>,
    pub category: Option<crate::plugins::manifest::model::PluginWorkspaceCategory>,
    /// Package-relative logo path from the active manifest (not a filesystem path).
    pub logo: Option<String>,
    /// Safe data URL when the logo file resolves; never a local path.
    pub logo_url: Option<String>,
    pub permissions: Vec<PluginPermissionDto>,
    pub commands: Vec<PluginCommandDto>,
    pub host_menus: Vec<PluginHostMenuDto>,
    pub workflows: Vec<PluginWorkflowDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPermissionDto {
    pub permission: String,
    pub granted: bool,
    pub hosts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCommandDto {
    pub id: String,
    pub title: String,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginHostMenuDto {
    pub plugin_id: String,
    pub command: String,
    pub group: Option<String>,
    pub when: Option<String>,
    pub title: String,
    pub workflow_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginWorkflowDto {
    pub id: String,
    pub file: String,
}
