use std::sync::Arc;

use crate::domain::ConnectionRoute;

use super::bastion::BastionRegistry;
use super::errors::ConnectionError;
use super::provider::{ConnectionContext, ConnectionRequest, ResolvedConnection};
use super::route::{ConnectionRouteSummary, SessionIntent};
use super::transport::TransportPlan;

/// Resolves a host `ConnectionRoute` into a concrete connection plan.
///
/// Direct / JumpHost stay on the legacy SSH path but now carry a secret-free
/// [`TransportPlan`] sketch. Bastion routes are validated against the registry;
/// full [`PreparedConnection`] is produced after provider auth/connect.
#[derive(Clone)]
pub struct ConnectionResolver {
    bastions: Arc<BastionRegistry>,
}

impl ConnectionResolver {
    pub fn new(bastions: Arc<BastionRegistry>) -> Self {
        Self { bastions }
    }

    pub fn bastions(&self) -> &BastionRegistry {
        &self.bastions
    }

    pub fn summarize(route: &ConnectionRoute) -> ConnectionRouteSummary {
        match route {
            ConnectionRoute::Direct => ConnectionRouteSummary::Direct,
            ConnectionRoute::JumpHost { profile_id } => ConnectionRouteSummary::JumpHost {
                jump_host_id: *profile_id,
            },
            ConnectionRoute::Bastion {
                bastion_id,
                provider,
                asset_id,
                account_id,
                ..
            } => ConnectionRouteSummary::Bastion {
                bastion_id: *bastion_id,
                provider: provider.clone(),
                asset_id: asset_id.clone(),
                account_id: account_id.clone(),
            },
        }
    }

    pub async fn resolve(
        &self,
        ctx: &ConnectionContext,
        request: ConnectionRequest,
    ) -> Result<ResolvedConnection, ConnectionError> {
        match &request.route {
            ConnectionRoute::Direct => Ok(ResolvedConnection::LegacySsh {
                route: ConnectionRoute::Direct,
                // Host/port filled by SessionManager from the profile; placeholder
                // keeps the TransportPlan discriminant available to callers.
                transport: TransportPlan::Tcp {
                    host: String::new(),
                    port: 0,
                },
            }),
            ConnectionRoute::JumpHost { profile_id } => Ok(ResolvedConnection::LegacySsh {
                route: ConnectionRoute::JumpHost {
                    profile_id: *profile_id,
                },
                transport: TransportPlan::SshJump {
                    hops: Vec::new(),
                    target_host: String::new(),
                    target_port: 0,
                },
            }),
            ConnectionRoute::Bastion {
                bastion_id,
                provider,
                asset_id,
                account_id,
                ..
            } => {
                if self.bastions.get(provider).is_none() {
                    tracing::warn!(
                        profile_id = %ctx.profile_id,
                        provider = %provider,
                        intent = ?ctx.intent,
                        "bastion provider not registered"
                    );
                    return Err(ConnectionError::BastionProviderNotFound);
                }
                // Validate only. Opening a target session through
                // ServerSessionManager / TransportFactory happens after auth.
                if matches!(request.intent, SessionIntent::PortForward) {
                    return Err(ConnectionError::BastionUnavailable);
                }
                Ok(ResolvedConnection::Bastion {
                    bastion_id: *bastion_id,
                    provider: provider.clone(),
                    asset_id: asset_id.clone(),
                    account_id: account_id.clone(),
                })
            }
        }
    }
}
