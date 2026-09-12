use async_trait::async_trait;

use super::account::BastionAccount;
use super::asset::{AssetPage, AssetQuery, BastionAsset};
use super::auth::{
    AuthChallengeResponse, AuthSession, AuthStepResult, BastionCredential,
};
use super::capabilities::{BastionCapabilities, ProviderLimits};
use super::errors::BastionError;
use super::session::{
    BastionConnectRequest, BastionConnection, BastionContext, BastionEndpoint, BastionProbeResult,
};

/// Vendor-agnostic bastion adapter. Implementations must not touch UI, Policy,
/// ChangeSet, CredentialVault plaintext, or Agent command execution.
#[async_trait]
pub trait BastionProvider: Send + Sync {
    fn id(&self) -> &'static str;

    fn display_name(&self) -> &'static str;

    fn capabilities(&self) -> BastionCapabilities;

    fn limits(&self) -> ProviderLimits {
        ProviderLimits::default()
    }

    async fn probe(
        &self,
        endpoint: &BastionEndpoint,
    ) -> Result<BastionProbeResult, BastionError>;

    async fn start_auth(
        &self,
        ctx: &BastionContext,
        credential: &BastionCredential,
    ) -> Result<AuthStepResult, BastionError>;

    async fn continue_auth(
        &self,
        session: &AuthSession,
        response: AuthChallengeResponse,
    ) -> Result<AuthStepResult, BastionError>;

    async fn list_assets(
        &self,
        session: &AuthSession,
        query: AssetQuery,
    ) -> Result<AssetPage, BastionError>;

    async fn list_accounts(
        &self,
        session: &AuthSession,
        asset: &BastionAsset,
    ) -> Result<Vec<BastionAccount>, BastionError>;

    async fn connect(
        &self,
        session: &AuthSession,
        request: BastionConnectRequest,
    ) -> Result<BastionConnection, BastionError>;

    /// Resolve the SSH gateway host:port used for host-key verification and KoKo login.
    /// JumpServer uses connection-token `client-url` smart endpoints when available.
    async fn resolve_ssh_gateway(
        &self,
        session: &AuthSession,
        asset: &BastionAsset,
        account: &BastionAccount,
    ) -> Result<(String, u16), BastionError> {
        let _ = (session, asset, account);
        Err(BastionError::CapabilityUnavailable)
    }

    /// Re-open an interactive external login helper (e.g. Teleport `tsh login` console).
    fn spawn_external_login(&self, session: &AuthSession) -> Result<(), BastionError> {
        let _ = session;
        Err(BastionError::CapabilityUnavailable)
    }

    /// Produce a vendor-agnostic [`PreparedConnection`] for TransportFactory + SSH Core.
    ///
    /// Default: unsupported. JumpServer may keep returning [`BastionConnection`] via
    /// `connect` until fully migrated; Teleport / Boundary implement this path.
    async fn prepare_connection(
        &self,
        session: &AuthSession,
        request: BastionConnectRequest,
    ) -> Result<crate::connection::PreparedConnection, BastionError> {
        let _ = (session, request);
        Err(BastionError::CapabilityUnavailable)
    }

    async fn release(
        &self,
        handle: &crate::connection::ProviderSessionHandle,
    ) -> Result<(), BastionError> {
        let _ = handle;
        Ok(())
    }

    async fn disconnect(&self, connection: &BastionConnection) -> Result<(), BastionError>;
}
