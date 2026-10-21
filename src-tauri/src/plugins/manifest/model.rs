use serde::{Deserialize, Serialize};

use crate::plugins::permissions::capability::PluginCapability;
use crate::tools::RiskLevel;

/// Fixed top-level workspace areas owned by Runory. Plugins may only place
/// entries inside these categories; they cannot create new top-level tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginWorkspaceCategory {
    Terminal,
    Files,
    Dashboard,
    Operations,
    Deployment,
}

impl PluginWorkspaceCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Files => "files",
            Self::Dashboard => "dashboard",
            Self::Operations => "operations",
            Self::Deployment => "deployment",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub publisher: PluginPublisher,
    pub api_version: String,
    pub min_runory_version: Option<String>,
    pub max_runory_version: Option<String>,
    pub icon: Option<String>,
    /// Packaged logo (PNG/WebP). Optional; package-relative only.
    pub logo: Option<String>,
    /// Workspace placement category. Optional for backward compatibility.
    /// Missing category means workspace entries are not exposed.
    pub category: Option<PluginWorkspaceCategory>,
    #[serde(default)]
    pub permissions: Vec<PermissionRequest>,
    #[serde(default)]
    pub contributes: PluginContributions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPublisher {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginContributions {
    #[serde(default)]
    pub commands: Vec<CommandContribution>,
    #[serde(default)]
    pub host_menus: Vec<HostMenuContribution>,
    #[serde(default)]
    pub workflows: Vec<WorkflowContribution>,
    #[serde(default)]
    pub agent_tools: Vec<AgentToolContribution>,
    #[serde(default)]
    pub workspace_entries: Vec<WorkspaceEntryContribution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEntryContribution {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub command: String,
    #[serde(default)]
    pub actions: Vec<String>,
    pub order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandContribution {
    pub id: String,
    pub title: String,
    pub category: Option<String>,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostMenuContribution {
    pub command: String,
    pub group: Option<String>,
    pub when: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowContribution {
    pub id: String,
    pub file: String,
    pub input_schema: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolContribution {
    pub name: String,
    pub description: String,
    pub workflow: String,
    pub risk: RiskLevel,
    #[serde(default)]
    pub requires_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PermissionRequest {
    Simple(String),
    Scoped {
        permission: String,
        #[serde(default)]
        hosts: Vec<String>,
    },
}

impl PermissionRequest {
    pub fn permission_id(&self) -> &str {
        match self {
            Self::Simple(value) => value.as_str(),
            Self::Scoped { permission, .. } => permission.as_str(),
        }
    }

    pub fn hosts(&self) -> &[String] {
        match self {
            Self::Simple(_) => &[],
            Self::Scoped { hosts, .. } => hosts.as_slice(),
        }
    }

    pub fn capability(&self) -> Result<PluginCapability, String> {
        PluginCapability::parse(self.permission_id())
    }
}
