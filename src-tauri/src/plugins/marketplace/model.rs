use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplacePlugin {
    pub id: String,
    pub publisher_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub latest_version: String,
    pub api_version: String,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplacePluginVersion {
    pub plugin_id: String,
    pub version: String,
    pub api_version: String,
    pub min_runory_version: Option<String>,
    pub package_sha256: String,
    pub download_url: String,
    pub status: String,
    pub changelog: Option<String>,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevocationEntry {
    pub kind: String,
    pub target_id: String,
    pub version: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceCatalog {
    pub plugins: Vec<MarketplacePlugin>,
    pub versions: Vec<MarketplacePluginVersion>,
    pub revocations: Vec<RevocationEntry>,
}
