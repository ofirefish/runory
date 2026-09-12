use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use uuid::Uuid;

use crate::connection::bastion::account::BastionAccount;
use crate::connection::bastion::asset::{
    AssetPage, AssetProtocol, AssetQuery, BastionAsset, BastionProtocol,
};
use crate::connection::bastion::auth::{
    AuthChallenge, AuthChallengeResponse, AuthSession, AuthStepResult, BastionCredential,
    BastionPrincipal, ProtectedProviderState,
};
use crate::connection::bastion::capabilities::BastionCapabilities;
use crate::connection::bastion::errors::BastionError;
use crate::connection::bastion::provider::BastionProvider;
use crate::connection::bastion::session::{
    BastionConnectRequest, BastionConnection, BastionContext, BastionEndpoint, BastionProbeResult,
    BastionSessionMetadata, NativeBastionSession,
};

const MOCK_TOTP_CODE: &str = "123456";
const PENDING_MARKER: &[u8] = b"pending-totp";
const AUTHED_MARKER: &[u8] = b"authed";

struct MockNativeSession {
    id: String,
    metadata: BastionSessionMetadata,
}

impl NativeBastionSession for MockNativeSession {
    fn connection_id(&self) -> &str {
        &self.id
    }

    fn metadata(&self) -> &BastionSessionMetadata {
        &self.metadata
    }
}

/// In-memory BastionProvider for UI / Runtime / contract tests.
///
/// Behaviour:
/// - `probe` succeeds for any endpoint with provider id `mock`
/// - password auth always challenges with TOTP; code `123456` succeeds
/// - assets / accounts are fixed fixtures; no target secrets are exposed
#[derive(Default)]
pub struct MockBastionProvider {
    live_sessions: Mutex<HashMap<String, BastionSessionMetadata>>,
}

impl MockBastionProvider {
    pub fn new() -> Self {
        Self::default()
    }

    fn now_millis() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    fn fixture_assets() -> Vec<BastionAsset> {
        vec![
            BastionAsset {
                provider: "mock".into(),
                remote_id: "asset-prod-db-01".into(),
                name: "prod-db-01".into(),
                address: Some("10.10.20.31".into()),
                platform: Some("linux".into()),
                protocols: vec![AssetProtocol {
                    protocol: BastionProtocol::Ssh,
                    port: Some(22),
                    enabled: true,
                }],
                node_path: Some("/Production".into()),
                labels: HashMap::from([("env".into(), "production".into())]),
                metadata: serde_json::json!({}),
            },
            BastionAsset {
                provider: "mock".into(),
                remote_id: "asset-prod-web-01".into(),
                name: "prod-web-01".into(),
                address: Some("10.10.20.11".into()),
                platform: Some("linux".into()),
                protocols: vec![
                    AssetProtocol {
                        protocol: BastionProtocol::Ssh,
                        port: Some(22),
                        enabled: true,
                    },
                    AssetProtocol {
                        protocol: BastionProtocol::Sftp,
                        port: Some(22),
                        enabled: true,
                    },
                ],
                node_path: Some("/Production".into()),
                labels: HashMap::from([("env".into(), "production".into())]),
                metadata: serde_json::json!({}),
            },
            BastionAsset {
                provider: "mock".into(),
                remote_id: "asset-test-api-01".into(),
                name: "test-api-01".into(),
                address: Some("10.20.0.5".into()),
                platform: Some("linux".into()),
                protocols: vec![AssetProtocol {
                    protocol: BastionProtocol::Ssh,
                    port: Some(22),
                    enabled: true,
                }],
                node_path: Some("/Testing".into()),
                labels: HashMap::from([("env".into(), "testing".into())]),
                metadata: serde_json::json!({}),
            },
        ]
    }
}

#[async_trait]
impl BastionProvider for MockBastionProvider {
    fn id(&self) -> &'static str {
        "mock"
    }

    fn display_name(&self) -> &'static str {
        "Mock Bastion"
    }

    fn capabilities(&self) -> BastionCapabilities {
        BastionCapabilities::SSH
            | BastionCapabilities::SFTP
            | BastionCapabilities::ASSET_DISCOVERY
            | BastionCapabilities::ACCOUNT_DISCOVERY
            | BastionCapabilities::PASSWORD_AUTH
            | BastionCapabilities::MFA
            | BastionCapabilities::SESSION_RECORDING
            | BastionCapabilities::COMMAND_AUDIT
    }

    async fn probe(
        &self,
        endpoint: &BastionEndpoint,
    ) -> Result<BastionProbeResult, BastionError> {
        if endpoint.provider != "mock" {
            return Err(BastionError::ProviderUnavailable);
        }
        if endpoint.host.trim().is_empty() {
            return Err(BastionError::Network);
        }
        Ok(BastionProbeResult {
            reachable: true,
            api_version: Some("1.0.0".into()),
            server_name: Some(endpoint.name.clone()),
        })
    }

    async fn start_auth(
        &self,
        ctx: &BastionContext,
        credential: &BastionCredential,
    ) -> Result<AuthStepResult, BastionError> {
        let username = match credential {
            BastionCredential::Password { username, .. } => username.clone(),
            BastionCredential::SshKey { username, .. } => username.clone(),
            BastionCredential::ExternalAgent { username } => username.clone(),
            BastionCredential::AccessKey { key_id, .. } => key_id.clone(),
            BastionCredential::Token { .. } | BastionCredential::BrowserSso { .. } => {
                return Err(BastionError::CapabilityUnavailable);
            }
        };
        if username.trim().is_empty() {
            return Err(BastionError::AuthenticationFailed);
        }

        tracing::info!(
            provider = "mock",
            bastion_id = %ctx.endpoint.id,
            "mock bastion auth started; totp challenge issued"
        );

        let pending = AuthSession {
            id: Uuid::new_v4(),
            bastion_id: ctx.endpoint.id,
            provider: self.id().into(),
            principal: BastionPrincipal {
                username,
                display_name: None,
            },
            provider_state: ProtectedProviderState::new(PENDING_MARKER.to_vec()),
            expires_at: Some(Self::now_millis() + 300_000),
            helper_cli_override: None,
        };
        Ok(AuthStepResult::Challenge {
            pending,
            challenge: AuthChallenge::Totp {
                id: format!("totp-{}", Uuid::new_v4()),
                message: "Enter the one-time password".into(),
            },
        })
    }

    async fn continue_auth(
        &self,
        session: &AuthSession,
        response: AuthChallengeResponse,
    ) -> Result<AuthStepResult, BastionError> {
        if session.provider != self.id() {
            return Err(BastionError::ProviderUnavailable);
        }
        match response {
            AuthChallengeResponse::Cancel { .. } => Err(BastionError::Cancelled),
            AuthChallengeResponse::Totp { code, .. } => {
                if code != MOCK_TOTP_CODE {
                    return Err(BastionError::AuthenticationFailed);
                }
                Ok(AuthStepResult::Authenticated(AuthSession {
                    id: session.id,
                    bastion_id: session.bastion_id,
                    provider: session.provider.clone(),
                    principal: session.principal.clone(),
                    provider_state: ProtectedProviderState::new(AUTHED_MARKER.to_vec()),
                    expires_at: Some(Self::now_millis() + 3_600_000),
                    helper_cli_override: session.helper_cli_override.clone(),
                }))
            }
            _ => Err(BastionError::ProviderProtocolError),
        }
    }

    async fn list_assets(
        &self,
        session: &AuthSession,
        query: AssetQuery,
    ) -> Result<AssetPage, BastionError> {
        ensure_authed(session)?;
        let page_size = query.page_size.max(1);
        let mut items = Self::fixture_assets();
        if let Some(search) = query.search.as_ref().map(|s| s.to_lowercase()) {
            items.retain(|asset| {
                asset.name.to_lowercase().contains(&search)
                    || asset.remote_id.to_lowercase().contains(&search)
            });
        }
        if let Some(node) = &query.node {
            items.retain(|asset| asset.node_path.as_deref() == Some(node.as_str()));
        }
        if let Some(protocol) = &query.protocol {
            items.retain(|asset| {
                asset
                    .protocols
                    .iter()
                    .any(|p| p.enabled && &p.protocol == protocol)
            });
        }
        let start = (query.page as usize).saturating_mul(page_size as usize);
        let end = (start + page_size as usize).min(items.len());
        let page_items = if start >= items.len() {
            Vec::new()
        } else {
            items[start..end].to_vec()
        };
        let has_more = end < items.len();
        Ok(AssetPage {
            items: page_items,
            page: query.page,
            page_size,
            has_more,
        })
    }

    async fn list_accounts(
        &self,
        session: &AuthSession,
        asset: &BastionAsset,
    ) -> Result<Vec<BastionAccount>, BastionError> {
        ensure_authed(session)?;
        if !Self::fixture_assets()
            .iter()
            .any(|item| item.remote_id == asset.remote_id)
        {
            return Err(BastionError::AssetNotFound);
        }
        Ok(vec![
            BastionAccount {
                remote_id: Some("acct-root".into()),
                username: "root".into(),
                display_name: Some("Root".into()),
                privileged: true,
                secret_managed_by_bastion: true,
                metadata: serde_json::json!({}),
            },
            BastionAccount {
                remote_id: Some("acct-deploy".into()),
                username: "deploy".into(),
                display_name: Some("Deploy".into()),
                privileged: false,
                secret_managed_by_bastion: true,
                metadata: serde_json::json!({}),
            },
        ])
    }

    async fn resolve_ssh_gateway(
        &self,
        session: &AuthSession,
        _asset: &BastionAsset,
        _account: &BastionAccount,
    ) -> Result<(String, u16), BastionError> {
        ensure_authed(session)?;
        Ok(("mock.bastion.local".into(), 2222))
    }

    async fn connect(
        &self,
        session: &AuthSession,
        request: BastionConnectRequest,
    ) -> Result<BastionConnection, BastionError> {
        ensure_authed(session)?;
        if !matches!(
            request.protocol,
            BastionProtocol::Ssh | BastionProtocol::Sftp
        ) {
            return Err(BastionError::ProtocolUnsupported);
        }
        if request.options.request_port_forward {
            return Err(BastionError::CapabilityUnavailable);
        }
        let caps = self.capabilities();
        if request.options.request_sftp && !caps.contains(BastionCapabilities::SFTP) {
            return Err(BastionError::CapabilityUnavailable);
        }

        let connection_id = format!("mock-sess-{}", Uuid::new_v4());
        let metadata = BastionSessionMetadata {
            session_id: Some(connection_id.clone()),
            provider: self.id().into(),
            asset_id: request.asset.remote_id.clone(),
            account: request.account.username.clone(),
            recording: caps.contains(BastionCapabilities::SESSION_RECORDING),
            command_audit: caps.contains(BastionCapabilities::COMMAND_AUDIT),
            file_audit: caps.contains(BastionCapabilities::FILE_AUDIT),
            started_at: Self::now_millis(),
        };
        self.live_sessions
            .lock()
            .map_err(|_| BastionError::Internal)?
            .insert(connection_id.clone(), metadata.clone());

        tracing::info!(
            provider = "mock",
            bastion_id = %session.bastion_id,
            asset_id = %request.asset.remote_id,
            account = %request.account.username,
            session_id = %connection_id,
            "mock bastion session connected"
        );

        Ok(BastionConnection::Native {
            handle: Box::new(MockNativeSession {
                id: connection_id,
                metadata: metadata.clone(),
            }),
            metadata,
        })
    }

    async fn disconnect(&self, connection: &BastionConnection) -> Result<(), BastionError> {
        let id = match connection {
            BastionConnection::Native { handle, .. } => handle.connection_id().to_string(),
            BastionConnection::SshInteractive { session } => session.connection_id.clone(),
        };
        self.live_sessions
            .lock()
            .map_err(|_| BastionError::Internal)?
            .remove(&id);
        tracing::info!(provider = "mock", session_id = %id, "mock bastion session disconnected");
        Ok(())
    }
}

fn ensure_authed(session: &AuthSession) -> Result<(), BastionError> {
    if session.provider != "mock" {
        return Err(BastionError::ProviderUnavailable);
    }
    if session.provider_state.as_bytes() != AUTHED_MARKER {
        return Err(BastionError::AuthenticationExpired);
    }
    Ok(())
}
