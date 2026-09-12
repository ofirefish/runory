use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BastionProtocol {
    Ssh,
    Sftp,
    Scp,
    Rdp,
    Vnc,
    Telnet,
    Custom(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetProtocol {
    pub protocol: BastionProtocol,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    pub enabled: bool,
}

/// Remote asset identity from a bastion. `remote_id` is the stable key.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BastionAsset {
    pub provider: String,
    #[serde(rename = "remoteId")]
    pub remote_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    pub protocols: Vec<AssetProtocol>,
    #[serde(rename = "nodePath", skip_serializing_if = "Option::is_none")]
    pub node_path: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<BastionProtocol>,
    pub page: u32,
    #[serde(rename = "pageSize")]
    pub page_size: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetPage {
    pub items: Vec<BastionAsset>,
    pub page: u32,
    #[serde(rename = "pageSize")]
    pub page_size: u32,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
}
