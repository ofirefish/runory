use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::ConnectionRoute;

use super::errors::ConnectionError;
use super::prepared::PreparedConnection;
use super::route::SessionIntent;
use super::transport::{CredentialRef, JumpHop, TransportPlan};

/// Inputs available while resolving a connection. Phase A keeps this thin;
/// SessionManager wiring will expand it without changing the trait surface.
#[derive(Clone, Debug)]
pub struct ConnectionContext {
    pub profile_id: Uuid,
    pub intent: SessionIntent,
}

#[derive(Clone, Debug)]
pub struct ConnectionRequest {
    pub route: ConnectionRoute,
    pub intent: SessionIntent,
}

/// Result of route resolution.
///
/// Prefer [`ResolvedConnection::Prepared`] once callers supply enough endpoint
/// metadata. Legacy variants remain for gradual migration of Direct / Jump /
/// Bastion UI flows.
#[derive(Debug)]
pub enum ResolvedConnection {
    /// Fully described connection ready for TransportFactory + SSH Core.
    Prepared(PreparedConnection),
    /// Existing SSH Core should open the session (Direct or prepared JumpHost).
    /// Carries a secret-free transport sketch for progressive migration.
    LegacySsh {
        route: ConnectionRoute,
        transport: TransportPlan,
    },
    /// Bastion provider accepted the route; caller continues with AuthSession /
    /// connect using the registered provider id.
    Bastion {
        bastion_id: Uuid,
        provider: String,
        asset_id: String,
        account_id: Option<String>,
    },
}

impl ResolvedConnection {
    pub fn transport_plan(&self) -> Option<&TransportPlan> {
        match self {
            Self::Prepared(prepared) => Some(&prepared.transport),
            Self::LegacySsh { transport, .. } => Some(transport),
            Self::Bastion { .. } => None,
        }
    }
}

/// Build a secret-free single-hop jump plan from a jump host profile id.
/// Hop credentials are resolved later by SessionManager / CredentialVault.
pub fn jump_host_transport_plan(
    jump_host: &str,
    jump_port: u16,
    jump_username: &str,
    jump_credential_id: impl Into<String>,
    target_host: &str,
    target_port: u16,
) -> TransportPlan {
    TransportPlan::SshJump {
        hops: vec![JumpHop {
            host: jump_host.to_string(),
            port: jump_port,
            username: jump_username.to_string(),
            credential_ref: CredentialRef {
                id: jump_credential_id.into(),
            },
            host_key_scope: Some(format!("jump:{jump_host}")),
        }],
        target_host: target_host.to_string(),
        target_port,
    }
}

#[async_trait]
pub trait ConnectionProvider: Send + Sync {
    fn id(&self) -> &'static str;

    async fn resolve(
        &self,
        ctx: &ConnectionContext,
        request: ConnectionRequest,
    ) -> Result<ResolvedConnection, ConnectionError>;
}
