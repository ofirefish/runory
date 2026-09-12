//! Vendor-agnostic connection description produced by ConnectionResolver /
//! BastionProvider and consumed by TransportFactory + SSH Core.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::identity::{HostIdentityPolicy, LogicalTarget};
use super::transport::{CommandSpec, TransportPlan};

/// How SSH Core should authenticate after the transport is open.
///
/// Secrets are referenced, never stored in prepared plans that leave Rust.
#[derive(Clone, Debug)]
pub enum SshAuthPlan {
    Password {
        username: String,
        /// Transient secret for this connect attempt only.
        password: zeroize::Zeroizing<String>,
    },
    PrivateKey {
        username: String,
        key_material: zeroize::Zeroizing<Vec<u8>>,
        passphrase: Option<zeroize::Zeroizing<String>>,
    },
    /// Auth material is already negotiated by the provider (e.g. short-lived token).
    ProviderSupplied {
        username: String,
        password: zeroize::Zeroizing<String>,
    },
    /// SSH Core will prompt / use an already-open agent session.
    Deferred {
        username: String,
    },
    /// OpenSSH certificate authentication (Teleport short-lived cert + private key).
    /// Certificate bytes are public; private key material is zeroized.
    OpenSshCert {
        username: String,
        key_material: zeroize::Zeroizing<Vec<u8>>,
        certificate: Vec<u8>,
    },
}

/// SSH handshake parameters independent of transport.
#[derive(Clone, Debug)]
pub struct SshHandshakePlan {
    pub auth: SshAuthPlan,
    pub host_identity: HostIdentityPolicy,
    /// When true, use bastion-gateway client config (no keepalive, OpenSSH id).
    pub bastion_gateway: bool,
    pub password_only: bool,
}

/// Content-free audit context attached to a prepared connection.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recording: Option<bool>,
}

/// Handle that allows Provider.release() when the ServerSession closes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSessionHandle {
    pub provider_id: String,
    pub session_id: String,
    /// Unix millis; None means no known expiry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub helper_process: Option<HelperProcessId>,
}

/// Opaque id for an external helper process managed by ExternalHelperManager.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HelperProcessId(pub String);

impl HelperProcessId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// Unified output of every connection path (Direct, ProxyJump, Bastion).
#[derive(Debug)]
pub struct PreparedConnection {
    pub logical_target: LogicalTarget,
    pub transport: TransportPlan,
    pub ssh: SshHandshakePlan,
    pub lifecycle: Option<ProviderSessionHandle>,
    pub audit: Option<AuditContext>,
    /// Profile that owns this connection (when applicable).
    pub profile_id: Option<Uuid>,
}

impl PreparedConnection {
    pub fn direct_tcp(
        profile_id: Uuid,
        host: impl Into<String>,
        port: u16,
        display_name: impl Into<String>,
        ssh: SshHandshakePlan,
    ) -> Self {
        let host = host.into();
        let display_name = display_name.into();
        Self {
            logical_target: LogicalTarget::new(profile_id.to_string(), display_name),
            transport: TransportPlan::Tcp {
                host: host.clone(),
                port,
            },
            ssh,
            lifecycle: None,
            audit: None,
            profile_id: Some(profile_id),
        }
    }
}

/// Re-export for callers that only need the command shape from prepared plans.
pub type PreparedCommandSpec = CommandSpec;
