use serde::{Deserialize, Serialize};

/// Authorized account on a bastion asset. Target secrets stay on the bastion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BastionAccount {
    #[serde(rename = "remoteId", skip_serializing_if = "Option::is_none")]
    pub remote_id: Option<String>,
    pub username: String,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub privileged: bool,
    #[serde(rename = "secretManagedByBastion")]
    pub secret_managed_by_bastion: bool,
    #[serde(default)]
    pub metadata: serde_json::Value,
}
