//! Boundary BastionProvider — LocalTcpProxy via `boundary connect`.

use std::sync::Arc;

use async_trait::async_trait;

use crate::connection::bastion::account::BastionAccount;
use crate::connection::bastion::asset::{AssetPage, AssetQuery, BastionAsset, BastionProtocol};
use crate::connection::bastion::auth::{
    AuthChallengeResponse, AuthSession, AuthStepResult, BastionCredential,
};
use crate::connection::bastion::capabilities::BastionCapabilities;
use crate::connection::bastion::errors::BastionError;
use crate::connection::bastion::provider::BastionProvider;
use crate::connection::bastion::session::{
    BastionConnectRequest, BastionConnection, BastionContext, BastionEndpoint, BastionProbeResult,
};
use crate::connection::transport::LocalEndpointStrategy;
use crate::connection::{
    AuditContext, CommandSpec, HelperProcessId, HostIdentityPolicy, LogicalTarget,
    PreparedConnection, ProviderSessionHandle, SshAuthPlan, SshHandshakePlan, TransportPlan,
};
use crate::helper::{
    DefaultExternalHelperManager, ExternalHelperManager, HelperError, VersionConstraint,
};

use super::auth;
use super::cli::BoundaryCli;

pub struct BoundaryProvider {
    helpers: Arc<dyn ExternalHelperManager>,
}

impl BoundaryProvider {
    pub fn new() -> Self {
        Self {
            helpers: Arc::new(DefaultExternalHelperManager::new()),
        }
    }

    pub fn with_helpers(helpers: Arc<dyn ExternalHelperManager>) -> Self {
        Self { helpers }
    }

    fn map_helper(error: HelperError) -> BastionError {
        match error {
            HelperError::Missing => BastionError::HelperMissing,
            HelperError::VersionMismatch => BastionError::HelperVersionMismatch,
            HelperError::Cancelled => BastionError::Cancelled,
            HelperError::Unsupported => BastionError::CapabilityUnavailable,
            _ => BastionError::HelperProxyFailed,
        }
    }

    fn cli(&self, override_path: Option<&str>) -> Result<BoundaryCli, BastionError> {
        let binary = self
            .helpers
            .locate_binary_with_override(override_path, &["boundary", "boundary.exe"])
            .map_err(Self::map_helper)?;
        let _ = self
            .helpers
            .check_version(&binary, &VersionConstraint::at_least(0, 15))
            .map_err(Self::map_helper)?;
        Ok(BoundaryCli::new(binary))
    }
}

impl Default for BoundaryProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BastionProvider for BoundaryProvider {
    fn id(&self) -> &'static str {
        "boundary"
    }

    fn display_name(&self) -> &'static str {
        "HashiCorp Boundary"
    }

    fn capabilities(&self) -> BastionCapabilities {
        BastionCapabilities::SSH
            | BastionCapabilities::SFTP
            | BastionCapabilities::EXTERNAL_HELPER
            | BastionCapabilities::SHORT_LIVED_CREDENTIALS
            | BastionCapabilities::MANAGED_CREDENTIALS
            | BastionCapabilities::PASSWORD_AUTH
            | BastionCapabilities::TOKEN_AUTH
    }

    async fn probe(&self, endpoint: &BastionEndpoint) -> Result<BastionProbeResult, BastionError> {
        if endpoint.provider != "boundary" {
            return Err(BastionError::ProviderUnavailable);
        }
        let override_path = crate::connection::bastion::provider_cli_path(&endpoint.provider_config);
        let cli = self.cli(override_path.as_deref())?;
        let version = cli.version_string().map_err(Self::map_helper)?;
        Ok(BastionProbeResult {
            reachable: true,
            api_version: Some(version),
            server_name: Some(endpoint.host.clone()),
        })
    }

    async fn start_auth(
        &self,
        ctx: &BastionContext,
        credential: &BastionCredential,
    ) -> Result<AuthStepResult, BastionError> {
        auth::start_auth(&*self.helpers, ctx, credential).await
    }

    async fn continue_auth(
        &self,
        session: &AuthSession,
        response: AuthChallengeResponse,
    ) -> Result<AuthStepResult, BastionError> {
        auth::continue_auth(&*self.helpers, session, response).await
    }

    async fn list_assets(
        &self,
        session: &AuthSession,
        query: AssetQuery,
    ) -> Result<AssetPage, BastionError> {
        if session.provider != "boundary" {
            return Err(BastionError::AuthenticationExpired);
        }
        let state = auth::BoundarySessionState::decode(&session.provider_state)
            .ok_or(BastionError::AuthenticationExpired)?;
        let cli = self.cli(session.helper_cli_override.as_deref())?;
        let items = cli
            .list_targets_as_assets(&state, query.search.as_deref())
            .map_err(Self::map_helper)?;
        let page_size = query.page_size.max(1) as usize;
        let start = (query.page as usize).saturating_mul(page_size);
        let has_more = items.len() > start.saturating_add(page_size);
        let items = items.into_iter().skip(start).take(page_size).collect();
        Ok(AssetPage {
            items,
            page: query.page,
            page_size: query.page_size,
            has_more,
        })
    }

    async fn list_accounts(
        &self,
        session: &AuthSession,
        _asset: &BastionAsset,
    ) -> Result<Vec<BastionAccount>, BastionError> {
        if session.provider != "boundary" {
            return Err(BastionError::AuthenticationExpired);
        }
        Ok(vec![BastionAccount {
            remote_id: Some(session.principal.username.clone()),
            username: session.principal.username.clone(),
            display_name: None,
            privileged: false,
            secret_managed_by_bastion: false,
            metadata: serde_json::json!({}),
        }])
    }

    async fn connect(
        &self,
        session: &AuthSession,
        request: BastionConnectRequest,
    ) -> Result<BastionConnection, BastionError> {
        let cols = request.terminal.as_ref().map(|t| t.cols).unwrap_or(120);
        let rows = request.terminal.as_ref().map(|t| t.rows).unwrap_or(40);
        let mut prepared = self.prepare_connection(session, request.clone()).await?;

        let ctx = crate::connection::TransportContext::with_timeout(45);
        let opened = crate::connection::transport::TransportFactory::open(
            &prepared.transport,
            &ctx,
        )
        .await
        .map_err(|error| match error {
            crate::domain::AppError::ConnectionTimeout => BastionError::Timeout,
            crate::domain::AppError::ConnectionRefused => BastionError::Network,
            _ => BastionError::HelperProxyFailed,
        })?;

        // SSH auth for the local Boundary proxy:
        // - UI target password → use UI username (or brokered username) + that password
        // - else brokered JSON pair → use as a whole (never mix UI user with brokered password)
        // - else fail closed
        let brokered = opened
            .helper_ready_payload
            .as_ref()
            .and_then(|payload| BoundaryCli::parse_brokered_ssh_credentials(payload));
        let account_username = request.account.username.trim();
        if let Some(password) = request.options.transient_ssh_password.as_ref() {
            let username = if !account_username.is_empty() && account_username != "boundary" {
                account_username.to_string()
            } else if let Some((brokered_user, _)) = brokered.as_ref() {
                brokered_user.clone()
            } else {
                account_username.to_string()
            };
            if username.is_empty() {
                return Err(BastionError::AuthenticationFailed);
            }
            prepared.ssh.auth = crate::connection::SshAuthPlan::Password {
                username,
                password: password.clone(),
            };
            prepared.ssh.password_only = true;
        } else if let Some((brokered_user, brokered_password)) = brokered {
            prepared.ssh.auth = crate::connection::SshAuthPlan::Password {
                username: brokered_user,
                password: brokered_password,
            };
            prepared.ssh.password_only = true;
        } else {
            return Err(BastionError::AuthenticationFailed);
        }

        let opened = crate::connection::open_prepared_ssh_session_on_transport(
            prepared, opened, cols, rows,
        )
        .await?;
        Ok(opened.connection)
    }

    async fn prepare_connection(
        &self,
        session: &AuthSession,
        request: BastionConnectRequest,
    ) -> Result<PreparedConnection, BastionError> {
        if session.provider != "boundary" {
            return Err(BastionError::AuthenticationExpired);
        }
        if request.protocol != BastionProtocol::Ssh {
            return Err(BastionError::ProtocolUnsupported);
        }
        let cli = self.cli(session.helper_cli_override.as_deref())?;
        let state = auth::BoundarySessionState::decode(&session.provider_state)
            .ok_or(BastionError::AuthenticationExpired)?;
        let target_id = request.asset.remote_id.clone();
        let listen_port =
            crate::helper::reserve_loopback_port().map_err(Self::map_helper)?;
        let mut command = CommandSpec::new(
            cli.binary().display().to_string(),
            BoundaryCli::connect_args(&target_id, listen_port),
        );
        command.env = BoundaryCli::connect_env(&state);
        // Prefer explicit -addr so CLI does not default to https://127.0.0.1:9200.
        command.args.insert(1, "-addr".into());
        command.args.insert(2, state.addr.clone());
        let session_id = uuid::Uuid::new_v4().to_string();
        Ok(PreparedConnection {
            logical_target: LogicalTarget::new(target_id.clone(), request.asset.name.clone())
                .with_alias(format!("boundary:{target_id}")),
            transport: TransportPlan::LocalTcpProxy {
                command,
                endpoint: LocalEndpointStrategy::Fixed {
                    host: "127.0.0.1".into(),
                    port: listen_port,
                },
            },
            ssh: SshHandshakePlan {
                auth: SshAuthPlan::Deferred {
                    username: request.account.username.clone(),
                },
                host_identity: HostIdentityPolicy::KnownHost {
                    hostname: target_id.clone(),
                    scope: Some(format!("boundary:{target_id}")),
                },
                bastion_gateway: false,
                password_only: false,
            },
            lifecycle: Some(ProviderSessionHandle {
                provider_id: "boundary".into(),
                session_id,
                expires_at: None,
                helper_process: Some(HelperProcessId::new("pending")),
            }),
            audit: Some(AuditContext {
                provider: Some("boundary".into()),
                asset_id: Some(target_id),
                account_id: request.account.remote_id.clone(),
                recording: Some(false),
            }),
            profile_id: None,
        })
    }

    async fn disconnect(&self, connection: &BastionConnection) -> Result<(), BastionError> {
        let _ = connection;
        Ok(())
    }
}
