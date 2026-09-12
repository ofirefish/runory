use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Why the caller wants a session. Providers may negotiate different channels
/// (PTY vs SFTP vs exec) from the same route without upper layers caring which
/// bastion vendor is involved.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionIntent {
    Terminal,
    ExecuteCommand,
    Sftp,
    PortForward,
    AgentTool,
}

/// Content-free summary of how a host is reached. Safe for Policy / Audit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ConnectionRouteSummary {
    Direct,
    JumpHost {
        #[serde(rename = "jumpHostId")]
        jump_host_id: Uuid,
    },
    Bastion {
        #[serde(rename = "bastionId")]
        bastion_id: Uuid,
        provider: String,
        #[serde(rename = "assetId")]
        asset_id: String,
        #[serde(rename = "accountId", skip_serializing_if = "Option::is_none")]
        account_id: Option<String>,
    },
}
