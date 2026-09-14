use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::account::BastionAccount;
use super::asset::{BastionAsset, BastionProtocol};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BastionPorts {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TlsOptions {
    #[serde(rename = "insecureSkipVerify", default)]
    pub insecure_skip_verify: bool,
    #[serde(rename = "caCertRef", skip_serializing_if = "Option::is_none")]
    pub ca_cert_ref: Option<String>,
}

/// Configured bastion instance endpoint (no secrets).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BastionEndpoint {
    pub id: Uuid,
    pub provider: String,
    pub name: String,
    pub host: String,
    pub ports: BastionPorts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsOptions>,
    #[serde(rename = "providerConfig", default)]
    pub provider_config: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BastionProbeResult {
    pub reachable: bool,
    #[serde(rename = "apiVersion", skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    #[serde(rename = "serverName", skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOptions {
    pub term: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Clone, Debug, Default)]
pub struct BastionConnectOptions {
    pub request_sftp: bool,
    pub request_port_forward: bool,
    pub agent_forwarding: bool,
    pub locale: Option<String>,
    /// One-shot SSH password for helper-backed targets (e.g. Boundary local proxy).
    pub transient_ssh_password: Option<zeroize::Zeroizing<String>>,
}

impl PartialEq for BastionConnectOptions {
    fn eq(&self, other: &Self) -> bool {
        self.request_sftp == other.request_sftp
            && self.request_port_forward == other.request_port_forward
            && self.agent_forwarding == other.agent_forwarding
            && self.locale == other.locale
            && self.transient_ssh_password.is_some() == other.transient_ssh_password.is_some()
    }
}

impl Eq for BastionConnectOptions {}

impl serde::Serialize for BastionConnectOptions {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("BastionConnectOptions", 4)?;
        state.serialize_field("requestSftp", &self.request_sftp)?;
        state.serialize_field("requestPortForward", &self.request_port_forward)?;
        state.serialize_field("agentForwarding", &self.agent_forwarding)?;
        state.serialize_field("locale", &self.locale)?;
        state.end()
    }
}

impl<'de> serde::Deserialize<'de> for BastionConnectOptions {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Raw {
            #[serde(default)]
            request_sftp: bool,
            #[serde(default)]
            request_port_forward: bool,
            #[serde(default)]
            agent_forwarding: bool,
            #[serde(default)]
            locale: Option<String>,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(Self {
            request_sftp: raw.request_sftp,
            request_port_forward: raw.request_port_forward,
            agent_forwarding: raw.agent_forwarding,
            locale: raw.locale,
            transient_ssh_password: None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct BastionConnectRequest {
    pub asset: BastionAsset,
    pub account: BastionAccount,
    pub protocol: BastionProtocol,
    pub terminal: Option<TerminalOptions>,
    pub options: BastionConnectOptions,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BastionSessionMetadata {
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub provider: String,
    #[serde(rename = "assetId")]
    pub asset_id: String,
    pub account: String,
    pub recording: bool,
    #[serde(rename = "commandAudit")]
    pub command_audit: bool,
    #[serde(rename = "fileAudit")]
    pub file_audit: bool,
    /// Unix millis when the target session started.
    #[serde(rename = "startedAt")]
    pub started_at: i64,
}

/// Interactive SSH-backed bastion session (e.g. JumpServer KoKo, Teleport tsh).
pub struct BastionSshSession {
    pub connection_id: String,
    pub metadata: BastionSessionMetadata,
    pub client: russh::client::Handle<crate::ssh::HostKeyHandler>,
    pub writer: std::sync::Arc<russh::ChannelWriteHalf<russh::client::Msg>>,
    pub reader: Option<russh::ChannelReadHalf>,
    /// Keeps helper process / jump chain alive for the session lifetime.
    pub transport_cleanup: Option<crate::connection::transport::TransportCleanup>,
}

/// Vendor-agnostic connection handle. Not required to be a raw TCP stream.
pub enum BastionConnection {
    /// Interactive SSH PTY owned by a bastion gateway (KoKo, tsh, boundary proxy, …).
    SshInteractive { session: BastionSshSession },
}

impl BastionConnection {
    pub fn metadata(&self) -> &BastionSessionMetadata {
        match self {
            Self::SshInteractive { session } => &session.metadata,
        }
    }

    pub fn provider_label(&self) -> &str {
        self.metadata().provider.as_str()
    }
}

impl std::fmt::Debug for BastionConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SshInteractive { session } => f
                .debug_struct("BastionConnection::SshInteractive")
                .field("connection_id", &session.connection_id)
                .field("metadata", &session.metadata)
                .finish(),
        }
    }
}

/// Shared context passed into provider methods.
#[derive(Clone, Debug)]
pub struct BastionContext {
    pub endpoint: BastionEndpoint,
    pub timeouts: super::capabilities::BastionTimeouts,
}
