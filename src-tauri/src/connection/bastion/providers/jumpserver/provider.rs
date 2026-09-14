use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use uuid::Uuid;

use crate::connection::bastion::account::BastionAccount;
use crate::connection::bastion::asset::{
    AssetPage, AssetProtocol, AssetQuery, BastionAsset, BastionProtocol,
};
use crate::connection::bastion::auth::{
    AuthChallenge, AuthChallengeResponse, AuthSession, AuthStepResult, BastionCredential,
};
use crate::connection::bastion::capabilities::BastionCapabilities;
use crate::connection::bastion::errors::BastionError;
use crate::connection::bastion::provider::BastionProvider;
use crate::connection::bastion::session::{
    BastionConnectOptions, BastionConnectRequest, BastionConnection, BastionContext,
    BastionEndpoint, BastionProbeResult,
};

use super::api::{
    JumpServerApiAuth, JumpServerApiClient, JumpServerAuthOutcome, JumpServerHttp,
    ReqwestJumpServerHttp,
};
use super::auth::{
    auth_session, coerce_gateway_to_api_loopback, normalize_koko_host, normalize_koko_ssh_port,
    parse_api_base_url, JumpServerAuthState, JumpServerStage,
};
use super::koko::KokoClient;

const FINGERPRINT_PREFIX: &str = "fingerprint:";

/// JumpServer BastionProvider (API plane + KoKo session plane).
pub struct JumpServerProvider<H: JumpServerHttp = ReqwestJumpServerHttp> {
    http_factory: Arc<dyn Fn(&str) -> Result<H, BastionError> + Send + Sync>,
}

impl JumpServerProvider<ReqwestJumpServerHttp> {
    pub fn new() -> Self {
        Self {
            http_factory: Arc::new(|base_url| ReqwestJumpServerHttp::new(base_url)),
        }
    }
}

impl Default for JumpServerProvider<ReqwestJumpServerHttp> {
    fn default() -> Self {
        Self::new()
    }
}

impl<H: JumpServerHttp + 'static> JumpServerProvider<H> {
    pub fn with_http_factory(
        factory: impl Fn(&str) -> Result<H, BastionError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            http_factory: Arc::new(factory),
        }
    }

    fn client_for_url(&self, base_url: &str) -> Result<JumpServerApiClient<H>, BastionError> {
        Ok(JumpServerApiClient::new((self.http_factory)(base_url)?))
    }

    fn client_for_endpoint(
        &self,
        endpoint: &BastionEndpoint,
    ) -> Result<(JumpServerApiClient<H>, JumpServerAuthState), BastionError> {
        let koko_host = super::auth::normalize_koko_host(&endpoint.host)
            .ok_or(BastionError::ProviderUnavailable)?;
        let koko_port = super::auth::normalize_koko_ssh_port(endpoint.ports.ssh.unwrap_or(2222));
        let base_url =
            super::auth::resolve_api_base_url(endpoint).ok_or(BastionError::ProviderUnavailable)?;
        Ok((
            self.client_for_url(&base_url)?,
            JumpServerAuthState::Authenticated {
                base_url,
                koko_host,
                koko_port,
                bearer: String::new(),
                username: String::new(),
                password: String::new(),
                org_id: super::auth::resolve_org_id(endpoint),
            },
        ))
    }

    async fn start_password_auth(
        &self,
        ctx: &BastionContext,
        username: &str,
        transient_password: &Option<zeroize::Zeroizing<String>>,
    ) -> Result<AuthStepResult, BastionError> {
        let password = transient_password
            .as_ref()
            .map(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .ok_or(BastionError::AuthenticationFailed)?;
        if username.trim().is_empty() {
            return Err(BastionError::AuthenticationFailed);
        }

        let (client, _) = self.client_for_endpoint(&ctx.endpoint)?;
        let outcome = client.login(username, password).await?;
        match outcome {
            JumpServerAuthOutcome::Authenticated(token) => {
                let state = JumpServerAuthState::from_endpoint(
                    &ctx.endpoint,
                    JumpServerStage::Authenticated {
                        bearer: token.token,
                        username: username.to_string(),
                        password: password.to_string(),
                    },
                )
                .map_err(|_| BastionError::Internal)?;
                let session = auth_session(ctx.endpoint.id, state, Some(now_millis() + 3_600_000))
                    .map_err(|_| BastionError::Internal)?;
                tracing::info!(
                    provider = "jumpserver",
                    bastion_id = %ctx.endpoint.id,
                    auth_mode = "password",
                    "jumpserver auth succeeded"
                );
                Ok(AuthStepResult::Authenticated(session))
            }
            JumpServerAuthOutcome::MfaRequired {
                session_id,
                mfa_path,
                message,
            } => {
                let state = JumpServerAuthState::from_endpoint(
                    &ctx.endpoint,
                    JumpServerStage::PendingMfa {
                        session_id,
                        mfa_path,
                        username: username.to_string(),
                        password: password.to_string(),
                    },
                )
                .map_err(|_| BastionError::Internal)?;
                let pending = auth_session(ctx.endpoint.id, state, Some(now_millis() + 300_000))
                    .map_err(|_| BastionError::Internal)?;
                tracing::info!(
                    provider = "jumpserver",
                    bastion_id = %ctx.endpoint.id,
                    "jumpserver auth requires otp"
                );
                Ok(AuthStepResult::Challenge {
                    pending,
                    challenge: AuthChallenge::Totp {
                        id: format!("jms-otp-{}", Uuid::new_v4()),
                        message,
                    },
                })
            }
        }
    }

    async fn start_access_key_auth(
        &self,
        ctx: &BastionContext,
        key_id: &str,
        transient_secret: &Option<zeroize::Zeroizing<String>>,
    ) -> Result<AuthStepResult, BastionError> {
        let (key_id, secret) = super::signer::split_access_key_material(
            key_id,
            transient_secret
                .as_ref()
                .map(|value| value.as_str())
                .unwrap_or(""),
        );
        if key_id.is_empty() || secret.is_empty() {
            return Err(BastionError::AuthenticationFailed);
        }

        let (client, _) = self.client_for_endpoint(&ctx.endpoint)?;
        let org_id = super::auth::resolve_org_id(&ctx.endpoint);
        tracing::info!(
            provider = "jumpserver",
            bastion_id = %ctx.endpoint.id,
            auth_mode = "access_key",
            key_id_len = key_id.len(),
            org_id = org_id.as_deref().unwrap_or(""),
            api_base = %super::auth::resolve_api_base_url(&ctx.endpoint).unwrap_or_default(),
            "jumpserver access key auth starting"
        );
        eprintln!(
            "[runory jumpserver] stage=access_key_auth api_base={} org={} key_id_prefix={}",
            super::auth::resolve_api_base_url(&ctx.endpoint).unwrap_or_default(),
            org_id.as_deref().unwrap_or(""),
            key_id.chars().take(8).collect::<String>()
        );
        let api_auth = super::api::JumpServerApiAuth::AccessKey {
            key_id: key_id.clone(),
            secret: secret.clone(),
            org_id: org_id.clone(),
        };
        let user = client.get_current_user(&api_auth).await.map_err(|error| {
            eprintln!(
                "[runory jumpserver] stage=access_key_auth FAILED code={}",
                error.code()
            );
            tracing::warn!(
                provider = "jumpserver",
                bastion_id = %ctx.endpoint.id,
                error_code = error.code(),
                "jumpserver access key auth failed"
            );
            error
        })?;
        let state = JumpServerAuthState::from_endpoint(
            &ctx.endpoint,
            JumpServerStage::AuthenticatedAccessKey {
                key_id,
                secret,
                username: user.username.clone(),
            },
        )
        .map_err(|_| BastionError::Internal)?;
        let mut session =
            auth_session(ctx.endpoint.id, state, None).map_err(|_| BastionError::Internal)?;
        session.principal.display_name = user.name.or(Some(user.username));
        tracing::info!(
            provider = "jumpserver",
            bastion_id = %ctx.endpoint.id,
            auth_mode = "access_key",
            "jumpserver access key auth succeeded"
        );
        Ok(AuthStepResult::Authenticated(session))
    }
}

pub fn attach_fingerprint(
    options: BastionConnectOptions,
    fingerprint: &str,
) -> BastionConnectOptions {
    BastionConnectOptions {
        locale: Some(format!("{FINGERPRINT_PREFIX}{fingerprint}")),
        ..options
    }
}

pub fn fingerprint_from_options(options: &BastionConnectOptions) -> Option<String> {
    options
        .locale
        .as_ref()
        .filter(|value| value.starts_with(FINGERPRINT_PREFIX))
        .map(|value| value[FINGERPRINT_PREFIX.len()..].to_string())
        .filter(|value| !value.is_empty())
}

#[async_trait]
impl<H: JumpServerHttp + 'static> BastionProvider for JumpServerProvider<H> {
    fn id(&self) -> &'static str {
        "jumpserver"
    }

    fn display_name(&self) -> &'static str {
        "JumpServer"
    }

    fn capabilities(&self) -> BastionCapabilities {
        BastionCapabilities::SSH
            | BastionCapabilities::ASSET_DISCOVERY
            | BastionCapabilities::ACCOUNT_DISCOVERY
            | BastionCapabilities::PASSWORD_AUTH
            | BastionCapabilities::TOKEN_AUTH
            | BastionCapabilities::MFA
            | BastionCapabilities::SESSION_RECORDING
            | BastionCapabilities::COMMAND_AUDIT
            | BastionCapabilities::SHORT_LIVED_CREDENTIALS
            | BastionCapabilities::MANAGED_CREDENTIALS
    }

    async fn probe(&self, endpoint: &BastionEndpoint) -> Result<BastionProbeResult, BastionError> {
        if endpoint.host.trim().is_empty() {
            return Err(BastionError::Network);
        }
        let (client, _) = self.client_for_endpoint(endpoint)?;
        let version = client.probe_version().await.ok().flatten();
        Ok(BastionProbeResult {
            reachable: true,
            api_version: version,
            server_name: Some(endpoint.name.clone()),
        })
    }

    async fn start_auth(
        &self,
        ctx: &BastionContext,
        credential: &BastionCredential,
    ) -> Result<AuthStepResult, BastionError> {
        match credential {
            BastionCredential::AccessKey {
                key_id,
                transient_secret,
                ..
            } => {
                self.start_access_key_auth(ctx, key_id, transient_secret)
                    .await
            }
            BastionCredential::Password {
                username,
                transient_password,
                ..
            } => {
                self.start_password_auth(ctx, username, transient_password)
                    .await
            }
            _ => Err(BastionError::CapabilityUnavailable),
        }
    }

    async fn continue_auth(
        &self,
        session: &AuthSession,
        response: AuthChallengeResponse,
    ) -> Result<AuthStepResult, BastionError> {
        if session.provider != self.id() {
            return Err(BastionError::ProviderUnavailable);
        }
        let state =
            JumpServerAuthState::decode(&session.provider_state).ok_or(BastionError::Internal)?;
        match (state, response) {
            (
                JumpServerAuthState::PendingMfa {
                    base_url,
                    koko_host,
                    koko_port,
                    session_id,
                    mfa_path,
                    username,
                    password,
                    org_id,
                },
                AuthChallengeResponse::Totp { code, .. },
            ) => {
                let client = self.client_for_url(&base_url)?;
                let token = client
                    .complete_otp_login(&username, &password, &session_id, &mfa_path, &code)
                    .await?;
                let authenticated = JumpServerAuthState::Authenticated {
                    base_url,
                    koko_host,
                    koko_port,
                    bearer: token.token,
                    username,
                    password,
                    org_id,
                };
                let next = auth_session(
                    session.bastion_id,
                    authenticated,
                    Some(now_millis() + 3_600_000),
                )
                .map_err(|_| BastionError::Internal)?;
                Ok(AuthStepResult::Authenticated(AuthSession {
                    id: session.id,
                    ..next
                }))
            }
            (_, AuthChallengeResponse::Cancel { .. }) => Err(BastionError::Cancelled),
            _ => Err(BastionError::ProviderProtocolError),
        }
    }

    async fn list_assets(
        &self,
        session: &AuthSession,
        query: AssetQuery,
    ) -> Result<AssetPage, BastionError> {
        let state =
            JumpServerAuthState::decode(&session.provider_state).ok_or(BastionError::Internal)?;
        let auth = state
            .api_auth()
            .ok_or(BastionError::AuthenticationExpired)?;
        let client = self.client_for_url(state.base_url())?;
        let page_size = query.page_size.max(1);
        let offset = query.page.saturating_mul(page_size);
        let (items, has_more) = client
            .list_assets(&auth, query.search.as_deref(), offset, page_size)
            .await?;
        let mapped = items
            .into_iter()
            .filter(|asset| {
                query.protocol.as_ref().is_none_or(|protocol| {
                    matches!(protocol, BastionProtocol::Ssh)
                        && (asset.protocols.is_empty()
                            || asset
                                .protocols
                                .iter()
                                .any(|item| item.eq_ignore_ascii_case("ssh")))
                })
            })
            .map(|asset| BastionAsset {
                provider: self.id().into(),
                remote_id: asset.id,
                name: asset.name,
                address: asset.address,
                platform: asset.platform,
                protocols: vec![AssetProtocol {
                    protocol: BastionProtocol::Ssh,
                    port: Some(22),
                    enabled: true,
                }],
                node_path: None,
                labels: Default::default(),
                metadata: serde_json::json!({}),
            })
            .collect::<Vec<_>>();
        Ok(AssetPage {
            items: mapped,
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
        let state =
            JumpServerAuthState::decode(&session.provider_state).ok_or(BastionError::Internal)?;
        let auth = state
            .api_auth()
            .ok_or(BastionError::AuthenticationExpired)?;
        let client = self.client_for_url(state.base_url())?;
        let accounts = client.list_accounts(&auth, &asset.remote_id).await?;
        Ok(accounts
            .into_iter()
            .map(|account| BastionAccount {
                remote_id: account.id,
                username: account.username,
                display_name: account.name,
                privileged: account.privileged,
                secret_managed_by_bastion: true,
                metadata: serde_json::json!({
                    "jumpserverAlias": account.alias,
                }),
            })
            .collect())
    }

    async fn resolve_ssh_gateway(
        &self,
        session: &AuthSession,
        _asset: &BastionAsset,
        _account: &BastionAccount,
    ) -> Result<(String, u16), BastionError> {
        let state =
            JumpServerAuthState::decode(&session.provider_state).ok_or(BastionError::Internal)?;
        let auth = state
            .api_auth()
            .ok_or(BastionError::AuthenticationExpired)?;
        let client = self.client_for_url(state.base_url())?;
        let (host, port) = resolve_koko_ssh_endpoint(&client, &auth, &state).await?;
        eprintln!("[runory jumpserver] resolve_ssh_gateway host={host} port={port}");
        Ok((host, port))
    }

    async fn connect(
        &self,
        session: &AuthSession,
        request: BastionConnectRequest,
    ) -> Result<BastionConnection, BastionError> {
        if !matches!(request.protocol, BastionProtocol::Ssh) {
            return Err(BastionError::ProtocolUnsupported);
        }
        if request.options.request_port_forward {
            return Err(BastionError::CapabilityUnavailable);
        }
        let fingerprint =
            fingerprint_from_options(&request.options).ok_or(BastionError::SessionRejected)?;
        let state =
            JumpServerAuthState::decode(&session.provider_state).ok_or(BastionError::Internal)?;
        let auth = state
            .api_auth()
            .ok_or(BastionError::AuthenticationExpired)?;
        let client = self.client_for_url(state.base_url())?;
        // JumpServer connection-token looks up by Account.alias (usually `name`, not username).
        // jumpserver-client uses permed_accounts[0].name for the same reason.
        let account_candidates = connection_account_candidates(&request.account);
        let account_name = account_candidates
            .first()
            .cloned()
            .unwrap_or_else(|| connection_account_name(&request.account));
        let token = client
            .create_connection_token_for_accounts(
                &auth,
                &request.asset.remote_id,
                &account_candidates,
                "ssh",
            )
            .await?;

        let (host, port) =
            resolve_koko_ssh_endpoint_for_token(&client, &auth, &state, &token.id).await?;
        let cols = request
            .terminal
            .as_ref()
            .map(|term| term.cols)
            .unwrap_or(120);
        let rows = request
            .terminal
            .as_ref()
            .map(|term| term.rows)
            .unwrap_or(40);
        tracing::info!(
            provider = "jumpserver",
            bastion_id = %session.bastion_id,
            asset_id = %request.asset.remote_id,
            account = %account_name,
            koko_host = %host,
            koko_port = port,
            token_id = %token.id,
            "jumpserver opening koko ssh session"
        );
        eprintln!(
            "[runory jumpserver] stage=token_ssh host={host} port={port} token_id={} account={account_name}",
            token.id
        );

        let token_error = match KokoClient::connect_with_token(
            &host,
            port,
            &token,
            &fingerprint,
            &request.asset.remote_id,
            &request.account.username,
            cols,
            rows,
        )
        .await
        {
            Ok(ssh_session) => {
                tracing::info!(
                    provider = "jumpserver",
                    bastion_id = %session.bastion_id,
                    asset_id = %request.asset.remote_id,
                    account = %request.account.username,
                    session_id = %ssh_session.connection_id,
                    mode = "connection_token",
                    "jumpserver koko session connected"
                );
                return Ok(BastionConnection::SshInteractive {
                    session: ssh_session,
                });
            }
            Err(error) => error,
        };

        // Official SSH Guide fallback:
        // ssh -p2222 JumpServerUser@account@assetId@KoKoHost (password = JumpServer password).
        // See https://www.jumpserver.com/blog/connecting-via-ssh-terminal
        if let Some(password) = state.password() {
            let direct_user = format!(
                "{}@{}@{}",
                state.username().trim(),
                account_name.trim(),
                request.asset.remote_id.trim()
            );
            eprintln!(
                "[runory jumpserver] stage=token_ssh FAILED; trying direct_login user={}",
                direct_user
            );
            tracing::warn!(
                provider = "jumpserver",
                asset_id = %request.asset.remote_id,
                account = %account_name,
                "jumpserver token ssh failed; trying direct user@account@asset format"
            );
            match KokoClient::connect_with_password(
                &host,
                port,
                &direct_user,
                password,
                &fingerprint,
                &request.asset.remote_id,
                &request.account.username,
                cols,
                rows,
            )
            .await
            {
                Ok(ssh_session) => {
                    tracing::info!(
                        provider = "jumpserver",
                        bastion_id = %session.bastion_id,
                        asset_id = %request.asset.remote_id,
                        account = %request.account.username,
                        session_id = %ssh_session.connection_id,
                        mode = "direct_login",
                        "jumpserver koko session connected"
                    );
                    return Ok(BastionConnection::SshInteractive {
                        session: ssh_session,
                    });
                }
                Err(direct_error) => {
                    eprintln!(
                        "[runory jumpserver] stage=direct_login FAILED code={}",
                        direct_error.code()
                    );
                    tracing::warn!(
                        provider = "jumpserver",
                        "jumpserver direct-login ssh also failed"
                    );
                    return Err(direct_error);
                }
            }
        }

        eprintln!(
            "[runory jumpserver] stage=connect FAILED code={} (no password for direct fallback)",
            token_error.code()
        );
        Err(token_error)
    }

    async fn prepare_connection(
        &self,
        session: &AuthSession,
        request: BastionConnectRequest,
    ) -> Result<crate::connection::PreparedConnection, BastionError> {
        if !matches!(request.protocol, BastionProtocol::Ssh) {
            return Err(BastionError::ProtocolUnsupported);
        }
        let _fingerprint =
            fingerprint_from_options(&request.options).ok_or(BastionError::SessionRejected)?;
        let state =
            JumpServerAuthState::decode(&session.provider_state).ok_or(BastionError::Internal)?;
        let auth = state
            .api_auth()
            .ok_or(BastionError::AuthenticationExpired)?;
        let client = self.client_for_url(state.base_url())?;
        let account_candidates = connection_account_candidates(&request.account);
        let token = client
            .create_connection_token_for_accounts(
                &auth,
                &request.asset.remote_id,
                &account_candidates,
                "ssh",
            )
            .await?;
        let (host, port) =
            resolve_koko_ssh_endpoint_for_token(&client, &auth, &state, &token.id).await?;
        let username = format!("JMS-{}", token.id.trim());
        let session_id = Uuid::new_v4().to_string();
        Ok(crate::connection::PreparedConnection {
            logical_target: crate::connection::LogicalTarget::new(
                request.asset.remote_id.clone(),
                request.asset.name.clone(),
            )
            .with_alias(format!("jumpserver:{}", request.asset.remote_id)),
            transport: crate::connection::TransportPlan::Tcp { host, port },
            ssh: crate::connection::SshHandshakePlan {
                auth: crate::connection::SshAuthPlan::ProviderSupplied {
                    username,
                    password: zeroize::Zeroizing::new(token.value.trim().to_string()),
                },
                host_identity: crate::connection::HostIdentityPolicy::KnownHost {
                    hostname: request.asset.remote_id.clone(),
                    scope: Some("bastion".into()),
                },
                bastion_gateway: true,
                password_only: true,
            },
            lifecycle: Some(crate::connection::ProviderSessionHandle {
                provider_id: "jumpserver".into(),
                session_id,
                expires_at: None,
                helper_process: None,
            }),
            audit: Some(crate::connection::AuditContext {
                provider: Some("jumpserver".into()),
                asset_id: Some(request.asset.remote_id),
                account_id: request.account.remote_id.clone(),
                recording: Some(true),
            }),
            profile_id: None,
        })
    }

    async fn disconnect(&self, connection: &BastionConnection) -> Result<(), BastionError> {
        let BastionConnection::SshInteractive { session } = connection;
        let _ = session
            .client
            .disconnect(russh::Disconnect::ByApplication, "bastion disconnect", "en")
            .await;
        Ok(())
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn connection_account_name(account: &BastionAccount) -> String {
    connection_account_candidates(account)
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// Ordered JumpServer connection-token `account` candidates.
///
/// JumpServer `PermAssetDetailUtil.validate_permission` indexes by `Account.alias`:
/// special `@…` accounts use username; normal accounts use `name` (not OS username).
fn connection_account_candidates(account: &BastionAccount) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut push = |value: &str| {
        let value = value.trim();
        if value.is_empty() {
            return;
        }
        if candidates.iter().any(|existing| existing == value) {
            return;
        }
        candidates.push(value.to_string());
    };

    if let Some(alias) = account
        .metadata
        .get("jumpserverAlias")
        .and_then(|value| value.as_str())
    {
        push(alias);
    }
    if let Some(name) = account.display_name.as_deref() {
        push(name);
    }
    push(&account.username);
    candidates
}

/// Resolve KoKo SSH host/port the same way as jumpserver-client:
/// 1. `GET /terminal/endpoints/smart/?protocol=ssh`
/// 2. else hostname from JumpServer API URL + profile/default SSH port
///
/// When the API itself is reached via loopback (`localhost` / `127.0.0.1`), smart
/// endpoints often advertise LAN/public hosts that are unreachable from this client
/// (local install or SSH port-forward). In that case keep the smart SSH port but
/// coerce the host back to the API loopback host.
async fn resolve_koko_ssh_endpoint<H: JumpServerHttp>(
    client: &JumpServerApiClient<H>,
    auth: &JumpServerApiAuth,
    state: &JumpServerAuthState,
) -> Result<(String, u16), BastionError> {
    if let Some((host, port)) = client.fetch_smart_ssh_endpoint(auth).await? {
        let port = normalize_koko_ssh_port(port);
        let advertised = host.clone();
        let (host, port) = coerce_gateway_to_api_loopback(state.base_url(), host, port);
        if advertised != host {
            tracing::info!(
                api_host = %host,
                advertised_host = %advertised,
                koko_port = port,
                "jumpserver coercing koko host to API loopback"
            );
            eprintln!(
                "[runory jumpserver] koko coerce advertised_host={advertised} -> api_loopback={host} port={port}"
            );
        }
        tracing::info!(
            provider = "jumpserver",
            koko_host = %host,
            koko_port = port,
            "jumpserver using smart terminal endpoint"
        );
        eprintln!("[runory jumpserver] koko via smart endpoint host={host} port={port}");
        return Ok((host, port));
    }

    let (profile_host, profile_port) = state.koko_endpoint();
    let port = normalize_koko_ssh_port(profile_port);
    if let Some(parsed) = parse_api_base_url(state.base_url()) {
        tracing::info!(
            provider = "jumpserver",
            koko_host = %parsed.host,
            koko_port = port,
            "jumpserver using API URL host as koko fallback"
        );
        eprintln!(
            "[runory jumpserver] koko via api-url host={} port={port}",
            parsed.host
        );
        return Ok((parsed.host, port));
    }

    let host = normalize_koko_host(profile_host).ok_or(BastionError::ProviderUnavailable)?;
    tracing::info!(
        provider = "jumpserver",
        koko_host = %host,
        koko_port = port,
        "jumpserver using profile koko endpoint"
    );
    eprintln!("[runory jumpserver] koko via profile host={host} port={port}");
    Ok((host, port))
}

/// Prefer token `client-url` endpoint when present, then fall back to smart/API resolution.
async fn resolve_koko_ssh_endpoint_for_token<H: JumpServerHttp>(
    client: &JumpServerApiClient<H>,
    auth: &JumpServerApiAuth,
    state: &JumpServerAuthState,
    token_id: &str,
) -> Result<(String, u16), BastionError> {
    if let Some((host, port)) = client.fetch_token_client_endpoint(auth, token_id).await? {
        let port = normalize_koko_ssh_port(port);
        let advertised = host.clone();
        let (host, port) = coerce_gateway_to_api_loopback(state.base_url(), host, port);
        if advertised != host {
            eprintln!(
                "[runory jumpserver] koko coerce client-url host={advertised} -> api_loopback={host} port={port}"
            );
        }
        eprintln!("[runory jumpserver] koko via token client-url host={host} port={port}");
        return Ok((host, port));
    }
    resolve_koko_ssh_endpoint(client, auth, state).await
}
