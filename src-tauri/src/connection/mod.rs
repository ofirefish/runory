//! Enterprise connection framework (BastionProvider Phase A/B + Transport P0).
//!
//! Upper layers (Terminal / SFTP / Agent) resolve hosts through
//! [`ConnectionResolver`]. Vendor-specific bastion logic lives behind
//! [`bastion::BastionProvider`] and must not leak into SSH Core call sites.
//!
//! All providers ultimately describe a [`PreparedConnection`];
//! [`transport::TransportFactory`] opens the byte stream for SSH Core.

mod bastion;
mod errors;
mod flow;
mod identity;
mod open_prepared;
mod prepared;
mod provider;
mod resolver;
mod route;
mod session_bridge;
pub mod transport;

#[cfg(test)]
mod contract_tests;

// Re-exports are the public surface; production SessionManager wiring uses the
// flow service + session bridge without vendor-specific branches.
#[allow(unused_imports)]
pub use bastion::{
    AssetPage, AssetProtocol, AssetQuery, AuthChallenge, AuthChallengeResponse, AuthChoice,
    AuthSession, AuthStepResult, BastionAccount, BastionAsset, BastionCapabilities,
    BastionConnectOptions, BastionConnectRequest, BastionConnection, BastionContext,
    BastionCredential, BastionEndpoint, BastionError, BastionErrorDetail, BastionPorts,
    BastionPrincipal, BastionProbeResult, BastionProtocol, BastionProvider, BastionRegistry,
    BastionSessionMetadata, BastionSessionState, BastionTimeouts, ExternalAuthAction,
    JumpServerProvider, MockBastionProvider, NativeBastionSession, ProtectedProviderState,
    ProviderLimits, SecretRef, TeleportProvider, TerminalOptions, TlsOptions, BoundaryProvider,
};
#[allow(unused_imports)]
pub use errors::ConnectionError;
pub use flow::{BastionFlowService, BastionFlowSnapshot, BastionFlowUiState, HelperCliDefaults};
pub use identity::{HostIdentityPolicy, LogicalTarget, ProviderHostIdentity};
pub use open_prepared::{open_prepared_ssh_session, open_prepared_ssh_session_on_transport};
pub use prepared::{
    AuditContext, HelperProcessId, PreparedConnection, ProviderSessionHandle, SshAuthPlan,
    SshHandshakePlan,
};
#[allow(unused_imports)]
pub use provider::{ConnectionContext, ConnectionProvider, ConnectionRequest, ResolvedConnection};
#[allow(unused_imports)]
pub use resolver::ConnectionResolver;
#[allow(unused_imports)]
pub use route::{ConnectionRouteSummary, SessionIntent};
pub use session_bridge::BastionSessionBridge;
#[allow(unused_imports)]
pub use transport::{
    CommandSpec, CredentialRef, JumpHop, LocalEndpointStrategy, OpenedTransport, TransportContext,
    TransportFactory, TransportPlan,
};
#[allow(unused_imports)]
pub use transport::ssh_jump::{
    open_resolved as open_ssh_jump_resolved, ResolvedJumpAuth, ResolvedJumpHop, SshJumpOpenRequest,
};

use std::sync::Arc;

/// Build the default registry with Mock + JumpServer + Teleport + Boundary.
pub fn default_bastion_registry() -> BastionRegistry {
    let mut registry = BastionRegistry::new();
    registry.register(Arc::new(MockBastionProvider::new()));
    registry.register(Arc::new(JumpServerProvider::new()));
    registry.register(Arc::new(TeleportProvider::new()));
    registry.register(Arc::new(BoundaryProvider::new()));
    registry
}
