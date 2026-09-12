//! Teleport BastionProvider — uses official `tsh` as auth + stdio transport helper.

use std::sync::Arc;

use async_trait::async_trait;

use crate::connection::bastion::account::BastionAccount;
use crate::connection::bastion::asset::{AssetPage, AssetQuery, BastionProtocol};
use crate::connection::bastion::auth::{
    AuthChallengeResponse, AuthSession, AuthStepResult, BastionCredential,
};
use crate::connection::bastion::capabilities::BastionCapabilities;
use crate::connection::bastion::errors::BastionError;
use crate::connection::bastion::provider::BastionProvider;
use crate::connection::bastion::session::{
    BastionConnectRequest, BastionConnection, BastionContext, BastionEndpoint, BastionProbeResult,
};
use crate::connection::{
    AuditContext, CommandSpec, HelperProcessId, HostIdentityPolicy, LogicalTarget,
    PreparedConnection, ProviderHostIdentity, ProviderSessionHandle, SshAuthPlan, SshHandshakePlan,
    TransportPlan,
};
use crate::helper::{
    DefaultExternalHelperManager, ExternalHelperManager, HelperError, VersionConstraint,
};

use super::auth::{self, TeleportSessionState};
use super::tsh::{TeleportConnectParams, TshClient};

/// Teleport access via `tsh` (no private protocol reimplementation).
pub struct TeleportProvider {
    helpers: Arc<dyn ExternalHelperManager>,
}

impl TeleportProvider {
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
            HelperError::SpawnFailed
            | HelperError::ReadyTimeout
            | HelperError::EndpointDiscoveryFailed
            | HelperError::Crashed => BastionError::HelperProxyFailed,
            HelperError::Cancelled => BastionError::Cancelled,
            HelperError::Unsupported => BastionError::CapabilityUnavailable,
        }
    }

    fn tsh_client(&self, override_path: Option<&str>) -> Result<TshClient, BastionError> {
        let binary = self
            .helpers
            .locate_binary_with_override(override_path, &["tsh", "tsh.exe"])
            .map_err(Self::map_helper)?;
        let _version = self
            .helpers
            .check_version(&binary, &VersionConstraint::at_least(14, 0))
            .map_err(Self::map_helper)?;
        Ok(TshClient::new(binary))
    }

    fn session_params(session: &AuthSession) -> Result<(TeleportSessionState, TeleportConnectParams), BastionError> {
        let state = TeleportSessionState::decode(&session.provider_state).ok_or(
            BastionError::AuthenticationExpired,
        )?;
        let mut params = state.connect_params();
        if params.cluster_name.is_none() {
            // Prefer cluster from principal display_name if stored there.
            if let Some(cluster) = session.principal.display_name.clone() {
                params.cluster_name = Some(cluster);
            }
        }
        Ok((state, params))
    }
}

impl Default for TeleportProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BastionProvider for TeleportProvider {
    fn id(&self) -> &'static str {
        "teleport"
    }

    fn display_name(&self) -> &'static str {
        "Teleport"
    }

    fn capabilities(&self) -> BastionCapabilities {
        BastionCapabilities::SSH
            | BastionCapabilities::SFTP
            | BastionCapabilities::ASSET_DISCOVERY
            | BastionCapabilities::ACCOUNT_DISCOVERY
            | BastionCapabilities::BROWSER_SSO
            | BastionCapabilities::PASSWORD_AUTH
            | BastionCapabilities::MFA
            | BastionCapabilities::SSH_CERTIFICATE
            | BastionCapabilities::SHORT_LIVED_CREDENTIALS
            | BastionCapabilities::EXTERNAL_HELPER
            | BastionCapabilities::SESSION_RECORDING
            | BastionCapabilities::COMMAND_AUDIT
    }

    async fn probe(&self, endpoint: &BastionEndpoint) -> Result<BastionProbeResult, BastionError> {
        if endpoint.provider != "teleport" {
            return Err(BastionError::ProviderUnavailable);
        }
        let override_path = crate::connection::bastion::provider_cli_path(&endpoint.provider_config);
        let client = self.tsh_client(override_path.as_deref())?;
        let version = client.version_string().map_err(Self::map_helper)?;
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

    fn spawn_external_login(&self, session: &AuthSession) -> Result<(), BastionError> {
        let state = TeleportSessionState::decode(&session.provider_state)
            .ok_or(BastionError::AuthenticationExpired)?;
        let client = self.tsh_client(session.helper_cli_override.as_deref())?;
        auth::spawn_interactive_login(&client, &state.connect_params())
    }

    async fn list_assets(
        &self,
        session: &AuthSession,
        query: AssetQuery,
    ) -> Result<AssetPage, BastionError> {
        if session.provider != "teleport" {
            return Err(BastionError::AuthenticationExpired);
        }
        let (_state, params) = Self::session_params(session)?;
        let client = self.tsh_client(session.helper_cli_override.as_deref())?;
        let mut assets = client
            .list_nodes_as_assets(&params)
            .map_err(Self::map_helper)?;
        if let Some(search) = query.search.as_ref().map(|s| s.to_ascii_lowercase()) {
            assets.retain(|a| {
                a.name.to_ascii_lowercase().contains(&search)
                    || a.remote_id.to_ascii_lowercase().contains(&search)
                    || a.labels
                        .values()
                        .any(|v| v.to_ascii_lowercase().contains(&search))
            });
        }
        let page_size = query.page_size.max(1) as usize;
        let start = (query.page as usize).saturating_mul(page_size);
        let slice: Vec<_> = assets.into_iter().skip(start).take(page_size + 1).collect();
        let has_more = slice.len() > page_size;
        let items = slice.into_iter().take(page_size).collect();
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
        asset: &crate::connection::bastion::asset::BastionAsset,
    ) -> Result<Vec<BastionAccount>, BastionError> {
        if session.provider != "teleport" {
            return Err(BastionError::AuthenticationExpired);
        }
        let (state, params) = Self::session_params(session)?;
        let mut logins = state.os_logins.clone();
        if logins.is_empty() {
            // Refresh from live status when session snapshot lacked logins.
            let client = self.tsh_client(session.helper_cli_override.as_deref())?;
            if let Ok(status) = client.status(&params) {
                logins = status.os_logins;
            }
        }
        if logins.is_empty() {
            // Last resort: allow the profile-bound OS username if present in asset metadata.
            if let Some(hint) = asset
                .metadata
                .get("osLogin")
                .or_else(|| asset.metadata.get("login"))
                .and_then(|v| v.as_str())
            {
                logins.push(hint.to_string());
            }
        }
        // Never hard-fail discovery with PermissionDenied — empty list lets the UI prompt.
        Ok(logins
            .into_iter()
            .map(|username| BastionAccount {
                remote_id: Some(username.clone()),
                username: username.clone(),
                display_name: Some(format!("{} @ {}", username, asset.name)),
                privileged: username == "root",
                secret_managed_by_bastion: true,
                metadata: serde_json::json!({
                    "kind": "osLogin",
                    "teleportUser": state.teleport_user,
                }),
            })
            .collect())
    }

    async fn connect(
        &self,
        session: &AuthSession,
        request: BastionConnectRequest,
    ) -> Result<BastionConnection, BastionError> {
        let cols = request.terminal.as_ref().map(|t| t.cols).unwrap_or(120);
        let rows = request.terminal.as_ref().map(|t| t.rows).unwrap_or(40);
        let prepared = self.prepare_connection(session, request).await?;
        let opened = crate::connection::open_prepared_ssh_session(prepared, cols, rows).await?;
        Ok(opened.connection)
    }

    async fn prepare_connection(
        &self,
        session: &AuthSession,
        request: BastionConnectRequest,
    ) -> Result<PreparedConnection, BastionError> {
        if session.provider != "teleport" {
            return Err(BastionError::AuthenticationExpired);
        }
        if request.protocol != BastionProtocol::Ssh {
            return Err(BastionError::ProtocolUnsupported);
        }
        let (state, params) = Self::session_params(session)?;
        // Re-check auth before opening transport.
        let client = self.tsh_client(session.helper_cli_override.as_deref())?;
        let status = client.status(&params).map_err(Self::map_helper)?;
        if !status.is_valid() {
            return Err(BastionError::AuthenticationExpired);
        }
        let os_login = request.account.username.trim();
        if os_login.is_empty() {
            return Err(BastionError::PermissionDenied);
        }
        if !state.os_logins.is_empty()
            && !state
                .os_logins
                .iter()
                .any(|login| login.eq_ignore_ascii_case(os_login))
            && !status
                .os_logins
                .iter()
                .any(|login| login.eq_ignore_ascii_case(os_login))
        {
            return Err(BastionError::PermissionDenied);
        }

        let node = request.asset.remote_id.trim();
        let args = TshClient::proxy_ssh_args(&params, os_login, node).map_err(Self::map_helper)?;
        let command = CommandSpec::new(client.binary().display().to_string(), args);

        let identity = client
            .resolve_identity_files(&params, &state.teleport_user)
            .map_err(Self::map_helper)?;
        let key_material = std::fs::read(&identity.identity_file)
            .map_err(|_| BastionError::AuthenticationFailed)?;
        let certificate = std::fs::read(&identity.certificate_file)
            .map_err(|_| BastionError::AuthenticationFailed)?;

        let session_id = uuid::Uuid::new_v4().to_string();
        Ok(PreparedConnection {
            logical_target: LogicalTarget::new(
                request.asset.remote_id.clone(),
                request.asset.name.clone(),
            )
            .with_alias(format!("teleport:{}", request.asset.remote_id)),
            transport: TransportPlan::StdioProxy { command },
            ssh: SshHandshakePlan {
                auth: SshAuthPlan::OpenSshCert {
                    username: os_login.to_string(),
                    key_material: zeroize::Zeroizing::new(key_material),
                    certificate,
                },
                host_identity: HostIdentityPolicy::ProviderManaged {
                    provider_id: "teleport".into(),
                    identity: ProviderHostIdentity {
                        label: request.asset.name.clone(),
                        fingerprint_hint: None,
                    },
                },
                bastion_gateway: false,
                password_only: false,
            },
            lifecycle: Some(ProviderSessionHandle {
                provider_id: "teleport".into(),
                session_id,
                expires_at: state.valid_until_ms.or(status.valid_until_ms),
                helper_process: Some(HelperProcessId::new("pending")),
            }),
            audit: Some(AuditContext {
                provider: Some("teleport".into()),
                asset_id: Some(request.asset.remote_id),
                account_id: request.account.remote_id.clone(),
                recording: Some(true),
            }),
            profile_id: None,
        })
    }

    async fn disconnect(&self, connection: &BastionConnection) -> Result<(), BastionError> {
        let _ = connection;
        Ok(())
    }
}
