use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::Mutex;
use tokio::time::Instant;
use uuid::Uuid;

use crate::connection::bastion::{
    AssetPage, AssetQuery, AuthChallenge, AuthChallengeResponse, AuthSession, AuthStepResult,
    BastionAccount, BastionAsset, BastionConnectOptions, BastionConnectRequest, BastionConnection,
    BastionContext, BastionCredential, BastionEndpoint, BastionError, BastionPorts, BastionProtocol,
    BastionRegistry, BastionSessionMetadata, BastionSessionState, BastionTimeouts,
    ExternalAuthAction, TerminalOptions,
};
use crate::domain::{AppError, AppResult, ConnectionRoute, ServerProfile};

const FLOW_LIFETIME: Duration = Duration::from_secs(300);
const MAX_FLOWS: usize = 32;

/// Serializable snapshot for UI / Runtime. Never includes secrets or provider_state.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BastionFlowSnapshot {
    pub flow_id: Uuid,
    pub profile_id: Uuid,
    pub bastion_id: Uuid,
    pub provider: String,
    pub state: BastionFlowUiState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub challenge: Option<AuthChallenge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_action: Option<ExternalAuthAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_metadata: Option<BastionSessionMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BastionFlowUiState {
    Authenticating,
    AwaitingUser,
    Authenticated,
    DiscoveringAssets,
    SelectingAsset,
    DiscoveringAccounts,
    SelectingAccount,
    Connecting,
    Connected,
    Failed,
    Cancelled,
}

struct BastionFlow {
    profile_id: Uuid,
    bastion_id: Uuid,
    provider: String,
    preferred_asset_id: Option<String>,
    preferred_account_id: Option<String>,
    state: BastionSessionState,
    auth_session: Option<AuthSession>,
    /// Assets loaded during discovery; used so select-by-id does not re-search by UUID.
    asset_cache: HashMap<String, BastionAsset>,
    selected_asset: Option<BastionAsset>,
    selected_account: Option<BastionAccount>,
    connection: Option<BastionConnection>,
    /// Effective KoKo/gateway endpoint used for host-key verification.
    gateway_host: Option<String>,
    gateway_port: Option<u16>,
    expires_at: Instant,
}

/// Orchestrates bastion auth / discovery / connect with durable AwaitingUser resume.
pub struct BastionFlowService {
    registry: Arc<BastionRegistry>,
    flows: Mutex<HashMap<Uuid, BastionFlow>>,
}

/// Global helper CLI defaults from App Settings (profile `cliPath` still wins).
#[derive(Clone, Debug, Default)]
pub struct HelperCliDefaults {
    pub boundary_cli_path: Option<String>,
    pub teleport_cli_path: Option<String>,
}

impl HelperCliDefaults {
    pub fn from_settings(settings: &crate::domain::AppSettings) -> Self {
        Self {
            boundary_cli_path: settings.boundary_cli_path.clone(),
            teleport_cli_path: settings.teleport_cli_path.clone(),
        }
    }

    fn for_provider(&self, provider: &str) -> Option<String> {
        match provider {
            "boundary" => self.boundary_cli_path.clone(),
            "teleport" => self.teleport_cli_path.clone(),
            _ => None,
        }
    }
}

impl BastionFlowService {
    pub fn new(registry: Arc<BastionRegistry>) -> Self {
        Self {
            registry,
            flows: Mutex::new(HashMap::new()),
        }
    }

    pub fn registry(&self) -> &BastionRegistry {
        &self.registry
    }

    pub async fn start(
        &self,
        profile: &ServerProfile,
        credential: BastionCredential,
        helper_defaults: HelperCliDefaults,
    ) -> AppResult<BastionFlowSnapshot> {
        let ConnectionRoute::Bastion {
            bastion_id,
            provider,
            asset_id,
            account_id,
            ..
        } = profile.connection_route.clone()
        else {
            return Err(AppError::InvalidProfile);
        };
        let provider_impl = self
            .registry
            .get(&provider)
            .ok_or(AppError::BastionProviderNotFound)?;

        match &credential {
            BastionCredential::Password { username, transient_password, .. } => {
                if username.trim().is_empty()
                    || transient_password
                        .as_ref()
                        .map(|value| value.is_empty())
                        .unwrap_or(true)
                {
                    return Err(AppError::BastionAuthFailed);
                }
            }
            BastionCredential::AccessKey {
                key_id,
                transient_secret,
                ..
            } => {
                if key_id.trim().is_empty()
                    || transient_secret
                        .as_ref()
                        .map(|value| value.is_empty())
                        .unwrap_or(true)
                {
                    return Err(AppError::BastionAuthFailed);
                }
            }
            BastionCredential::Token { transient_token, .. } => {
                if transient_token
                    .as_ref()
                    .map(|value| value.is_empty())
                    .unwrap_or(true)
                {
                    return Err(AppError::BastionAuthFailed);
                }
            }
            BastionCredential::BrowserSso { .. } => {}
            _ => return Err(AppError::BastionUnavailable),
        }

        let endpoint = mock_or_profile_endpoint(bastion_id, &provider, profile, &helper_defaults);
        let ctx = BastionContext {
            endpoint,
            timeouts: BastionTimeouts::default(),
        };

        let step = provider_impl
            .start_auth(&ctx, &credential)
            .await
            .map_err(map_bastion_error)?;

        let credential_account = match &credential {
            BastionCredential::Token {
                username_hint: Some(hint),
                ..
            }
            | BastionCredential::BrowserSso {
                username_hint: Some(hint),
            }
            | BastionCredential::Password { username: hint, .. }
            | BastionCredential::ExternalAgent { username: hint } => non_empty(hint.clone()),
            _ => None,
        };

        // Teleport: profile.username / SSO hint are Teleport users, NOT OS logins.
        // Only an explicit route.accountId (OS Login) may auto-bind.
        let preferred_account_id = if provider == "teleport" {
            account_id.and_then(non_empty)
        } else {
            account_id
                .and_then(non_empty)
                .or(credential_account)
                .or_else(|| non_empty(profile.username.clone()))
        };

        let mut flow = BastionFlow {
            profile_id: profile.id,
            bastion_id,
            provider: provider.clone(),
            preferred_asset_id: non_empty(asset_id),
            preferred_account_id,
            state: BastionSessionState::Authenticating,
            auth_session: None,
            asset_cache: HashMap::new(),
            selected_asset: None,
            selected_account: None,
            connection: None,
            gateway_host: None,
            gateway_port: None,
            expires_at: Instant::now() + FLOW_LIFETIME,
        };

        flow_from_auth_step(&mut flow, step)?;
        let flow_id = self.insert_flow(flow).await?;
        // Access Key / password-without-MFA authenticate in start(); MFA resumes via
        // continue_auth. Both paths must advance into asset selection (or auto-bind).
        self.advance_after_authenticated(flow_id).await?;
        self.snapshot(flow_id).await
    }

    pub async fn continue_auth(
        &self,
        flow_id: Uuid,
        response: AuthChallengeResponse,
    ) -> AppResult<BastionFlowSnapshot> {
        let (provider_id, session) = {
            let mut flows = self.flows.lock().await;
            let flow = Self::get_mut(&mut flows, flow_id)?;
            match &flow.state {
                BastionSessionState::AwaitingUser { .. }
                | BastionSessionState::AwaitingExternalAuth { .. } => {}
                _ => return Err(AppError::BastionUnavailable),
            }
            let session = flow
                .auth_session
                .clone()
                .ok_or(AppError::BastionAuthFailed)?;
            (flow.provider.clone(), session)
        };
        let provider = self
            .registry
            .get(&provider_id)
            .ok_or(AppError::BastionProviderNotFound)?;
        let step = provider
            .continue_auth(&session, response)
            .await
            .map_err(map_bastion_error)?;

        {
            let mut flows = self.flows.lock().await;
            let flow = Self::get_mut(&mut flows, flow_id)?;
            flow_from_auth_step(flow, step)?;
        }
        self.advance_after_authenticated(flow_id).await?;
        self.snapshot(flow_id).await
    }

    pub async fn list_assets(
        &self,
        flow_id: Uuid,
        query: AssetQuery,
    ) -> AppResult<AssetPage> {
        let (provider_id, session) = self.require_auth_session(flow_id).await?;
        {
            let mut flows = self.flows.lock().await;
            let flow = Self::get_mut(&mut flows, flow_id)?;
            flow.state = BastionSessionState::DiscoveringAssets;
        }
        let provider = self
            .registry
            .get(&provider_id)
            .ok_or(AppError::BastionProviderNotFound)?;
        let page = provider
            .list_assets(&session, query)
            .await
            .map_err(map_bastion_error)?;
        {
            let mut flows = self.flows.lock().await;
            let flow = Self::get_mut(&mut flows, flow_id)?;
            for asset in &page.items {
                flow.asset_cache
                    .insert(asset.remote_id.clone(), asset.clone());
            }
            flow.state = BastionSessionState::SelectingAsset;
        }
        Ok(page)
    }

    pub async fn select_asset(
        &self,
        flow_id: Uuid,
        asset_id: String,
    ) -> AppResult<BastionFlowSnapshot> {
        let asset_id = asset_id.trim().to_string();
        if asset_id.is_empty() {
            return Err(AppError::BastionPermissionDenied);
        }
        let cached = {
            let flows = self.flows.lock().await;
            let flow = Self::get(&flows, flow_id)?;
            flow.asset_cache.get(&asset_id).cloned()
        };
        let asset = if let Some(asset) = cached {
            asset
        } else {
            let (provider_id, session) = self.require_auth_session(flow_id).await?;
            let provider = self
                .registry
                .get(&provider_id)
                .ok_or(AppError::BastionProviderNotFound)?;
            // Never pass a UUID as `search` — JumpServer name search will miss it.
            let page = provider
                .list_assets(
                    &session,
                    AssetQuery {
                        search: None,
                        node: None,
                        protocol: None,
                        page: 0,
                        page_size: 200,
                    },
                )
                .await
                .map_err(map_bastion_error)?;
            {
                let mut flows = self.flows.lock().await;
                let flow = Self::get_mut(&mut flows, flow_id)?;
                for item in &page.items {
                    flow.asset_cache
                        .insert(item.remote_id.clone(), item.clone());
                }
            }
            page.items
                .into_iter()
                .find(|item| item.remote_id == asset_id)
                .or_else(|| {
                    // Boundary MVP: target id is supplied on the profile without discovery.
                    if provider_id == "boundary" {
                        Some(crate::connection::BastionAsset {
                            provider: "boundary".into(),
                            remote_id: asset_id.clone(),
                            name: asset_id.clone(),
                            address: None,
                            platform: Some("linux".into()),
                            protocols: vec![crate::connection::AssetProtocol {
                                protocol: BastionProtocol::Ssh,
                                port: Some(22),
                                enabled: true,
                            }],
                            node_path: None,
                            labels: Default::default(),
                            metadata: serde_json::json!({}),
                        })
                    } else {
                        None
                    }
                })
                .ok_or(AppError::BastionPermissionDenied)?
        };
        {
            let mut flows = self.flows.lock().await;
            let flow = Self::get_mut(&mut flows, flow_id)?;
            flow.asset_cache
                .insert(asset.remote_id.clone(), asset.clone());
            flow.selected_asset = Some(asset);
            flow.state = BastionSessionState::DiscoveringAccounts;
        }
        self.advance_after_asset(flow_id).await?;
        self.snapshot(flow_id).await
    }

    pub async fn list_accounts(&self, flow_id: Uuid) -> AppResult<Vec<BastionAccount>> {
        let (provider_id, session, asset) = {
            let flows = self.flows.lock().await;
            let flow = Self::get(&flows, flow_id)?;
            let session = flow
                .auth_session
                .clone()
                .ok_or(AppError::BastionAuthFailed)?;
            let asset = flow
                .selected_asset
                .clone()
                .ok_or(AppError::BastionPermissionDenied)?;
            (flow.provider.clone(), session, asset)
        };
        let provider = self
            .registry
            .get(&provider_id)
            .ok_or(AppError::BastionProviderNotFound)?;
        let accounts = provider
            .list_accounts(&session, &asset)
            .await
            .map_err(map_bastion_error)?;
        {
            let mut flows = self.flows.lock().await;
            let flow = Self::get_mut(&mut flows, flow_id)?;
            flow.state = BastionSessionState::SelectingAccount;
        }
        Ok(accounts)
    }

    pub async fn select_account(
        &self,
        flow_id: Uuid,
        account_username: String,
    ) -> AppResult<BastionFlowSnapshot> {
        let (provider_id, accounts) = {
            let accounts = self.list_accounts(flow_id).await?;
            let flows = self.flows.lock().await;
            let provider = Self::get(&flows, flow_id)?.provider.clone();
            (provider, accounts)
        };
        let account = accounts
            .into_iter()
            .find(|item| {
                item.username == account_username
                    || item.remote_id.as_deref() == Some(account_username.as_str())
                    || item.display_name.as_deref() == Some(account_username.as_str())
                    || item
                        .metadata
                        .get("jumpserverAlias")
                        .and_then(|value| value.as_str())
                        == Some(account_username.as_str())
            })
            .or_else(|| {
                // Boundary / Teleport: allow an explicit OS login even when discovery is incomplete.
                if (provider_id == "boundary" || provider_id == "teleport")
                    && !account_username.trim().is_empty()
                {
                    Some(crate::connection::BastionAccount {
                        remote_id: Some(account_username.clone()),
                        username: account_username.clone(),
                        display_name: None,
                        privileged: account_username == "root",
                        secret_managed_by_bastion: true,
                        metadata: serde_json::json!({ "kind": "osLogin" }),
                    })
                } else {
                    None
                }
            })
            .ok_or(AppError::BastionPermissionDenied)?;
        {
            let mut flows = self.flows.lock().await;
            let flow = Self::get_mut(&mut flows, flow_id)?;
            flow.selected_account = Some(account);
            flow.state = BastionSessionState::Connecting;
        }
        Ok(self.snapshot(flow_id).await?)
    }

    pub async fn connect(
        &self,
        flow_id: Uuid,
        cols: u16,
        rows: u16,
        expected_fingerprint: Option<String>,
        ssh_password: Option<zeroize::Zeroizing<String>>,
    ) -> AppResult<BastionFlowSnapshot> {
        let (provider_id, session, asset, account) = {
            let mut flows = self.flows.lock().await;
            let flow = Self::get_mut(&mut flows, flow_id)?;
            flow.state = BastionSessionState::Connecting;
            let session = flow
                .auth_session
                .clone()
                .ok_or(AppError::BastionAuthFailed)?;
            let asset = flow
                .selected_asset
                .clone()
                .ok_or(AppError::BastionPermissionDenied)?;
            let account = flow
                .selected_account
                .clone()
                .ok_or(AppError::BastionPermissionDenied)?;
            (flow.provider.clone(), session, asset, account)
        };
        let provider = self
            .registry
            .get(&provider_id)
            .ok_or(AppError::BastionProviderNotFound)?;
        let mut options = BastionConnectOptions::default();
        let skip_host_fingerprint =
            provider_id == "mock" || provider_id == "teleport" || provider_id == "boundary";
        if !skip_host_fingerprint {
            let fingerprint = expected_fingerprint.ok_or(AppError::HostVerificationExpired)?;
            options = crate::connection::bastion::providers::attach_fingerprint(
                options,
                &fingerprint,
            );
        }
        if let Some(password) = ssh_password {
            options.transient_ssh_password = Some(password);
        }
        let connection = provider
            .connect(
                &session,
                BastionConnectRequest {
                    asset,
                    account,
                    protocol: BastionProtocol::Ssh,
                    terminal: Some(TerminalOptions {
                        term: "xterm-256color".into(),
                        cols,
                        rows,
                    }),
                    options,
                },
            )
            .await
            .map_err(map_bastion_error)?;
        {
            let mut flows = self.flows.lock().await;
            let flow = Self::get_mut(&mut flows, flow_id)?;
            flow.connection = Some(connection);
            flow.state = BastionSessionState::Connected;
        }
        self.snapshot(flow_id).await
    }

    /// Resolves the SSH gateway used for host-key verification and remembers it on the flow.
    pub async fn resolve_gateway_endpoint(
        &self,
        flow_id: Uuid,
    ) -> AppResult<(String, u16)> {
        let (provider_id, session, asset, account) = {
            let flows = self.flows.lock().await;
            let flow = Self::get(&flows, flow_id)?;
            let session = flow
                .auth_session
                .clone()
                .ok_or(AppError::BastionAuthFailed)?;
            let asset = flow
                .selected_asset
                .clone()
                .ok_or(AppError::BastionPermissionDenied)?;
            let account = flow
                .selected_account
                .clone()
                .ok_or(AppError::BastionPermissionDenied)?;
            (flow.provider.clone(), session, asset, account)
        };
        let provider = self
            .registry
            .get(&provider_id)
            .ok_or(AppError::BastionProviderNotFound)?;
        let (host, port) = provider
            .resolve_ssh_gateway(&session, &asset, &account)
            .await
            .map_err(map_bastion_error)?;
        {
            let mut flows = self.flows.lock().await;
            let flow = Self::get_mut(&mut flows, flow_id)?;
            flow.gateway_host = Some(host.clone());
            flow.gateway_port = Some(port);
        }
        eprintln!("[runory bastion] resolved gateway host={host} port={port}");
        Ok((host, port))
    }

    pub async fn gateway_endpoint(&self, flow_id: Uuid) -> AppResult<(String, u16)> {
        let flows = self.flows.lock().await;
        let flow = Self::get(&flows, flow_id)?;
        match (&flow.gateway_host, flow.gateway_port) {
            (Some(host), Some(port)) => Ok((host.clone(), port)),
            _ => Err(AppError::BastionUnavailable),
        }
    }

    /// Removes the live BastionConnection from the flow for SessionManager/bridge ownership.
    pub async fn take_connection(
        &self,
        flow_id: Uuid,
        profile_id: Uuid,
    ) -> AppResult<(BastionConnection, String)> {
        let mut flows = self.flows.lock().await;
        let flow = Self::get_mut(&mut flows, flow_id)?;
        if flow.profile_id != profile_id {
            return Err(AppError::InvalidProfile);
        }
        if !matches!(flow.state, BastionSessionState::Connected) {
            return Err(AppError::BastionUnavailable);
        }
        let connection = flow
            .connection
            .take()
            .ok_or(AppError::BastionUnavailable)?;
        let provider = flow.provider.clone();
        flows.remove(&flow_id);
        Ok((connection, provider))
    }

    pub async fn cancel(&self, flow_id: Uuid) {
        self.flows.lock().await.remove(&flow_id);
    }

    pub async fn snapshot(&self, flow_id: Uuid) -> AppResult<BastionFlowSnapshot> {
        let flows = self.flows.lock().await;
        let flow = Self::get(&flows, flow_id)?;
        Ok(snapshot_of(flow_id, flow))
    }

    /// Open the pending SSO / external-auth helper.
    ///
    /// Teleport: spawn an interactive `tsh login` console (real TTY).
    /// Other providers: open the system browser URL.
    pub async fn open_external_browser(&self, flow_id: Uuid) -> AppResult<()> {
        let (provider_id, session, url) = {
            let flows = self.flows.lock().await;
            let flow = Self::get(&flows, flow_id)?;
            let url = match &flow.state {
                BastionSessionState::AwaitingExternalAuth {
                    action: ExternalAuthAction::OpenBrowser { url, .. },
                } => url.clone(),
                BastionSessionState::AwaitingExternalAuth { .. } => String::new(),
                _ => return Err(AppError::BastionUnavailable),
            };
            (
                flow.provider.clone(),
                flow.auth_session.clone(),
                url,
            )
        };

        if provider_id == "teleport" {
            let session = session.ok_or(AppError::BastionAuthFailed)?;
            let provider = self
                .registry
                .get("teleport")
                .ok_or(AppError::BastionProviderNotFound)?;
            return provider
                .spawn_external_login(&session)
                .map_err(map_bastion_error);
        }

        let trimmed = url.trim();
        if trimmed.is_empty()
            || !(trimmed.starts_with("https://") || trimmed.starts_with("http://"))
        {
            return Err(AppError::BastionUnavailable);
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            return Err(AppError::BastionUnavailable);
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            open::that(trimmed).map_err(|error| {
                tracing::warn!(error = %error, "failed to open bastion SSO URL in system browser");
                AppError::BastionUnavailable
            })
        }
    }

    async fn advance_after_authenticated(&self, flow_id: Uuid) -> AppResult<()> {
        let preferred = {
            let flows = self.flows.lock().await;
            let flow = Self::get(&flows, flow_id)?;
            if !matches!(flow.state, BastionSessionState::Authenticated) {
                return Ok(());
            }
            flow.preferred_asset_id.clone()
        };
        if let Some(asset_id) = preferred {
            self.select_asset(flow_id, asset_id).await?;
        } else {
            let mut flows = self.flows.lock().await;
            let flow = Self::get_mut(&mut flows, flow_id)?;
            flow.state = BastionSessionState::SelectingAsset;
        }
        Ok(())
    }

    async fn advance_after_asset(&self, flow_id: Uuid) -> AppResult<()> {
        let preferred = {
            let flows = self.flows.lock().await;
            let flow = Self::get(&flows, flow_id)?;
            flow.preferred_account_id.clone()
        };
        if let Some(account) = preferred {
            match self.select_account(flow_id, account).await {
                Ok(_) => return Ok(()),
                Err(error) => {
                    // Preferred OS login may be stale / Teleport-user mistaken for login.
                    // Fall through to the account picker instead of failing the asset step.
                    tracing::info!(
                        error = %error,
                        "preferred bastion account unavailable; showing account picker"
                    );
                }
            }
        }
        let mut flows = self.flows.lock().await;
        let flow = Self::get_mut(&mut flows, flow_id)?;
        flow.state = BastionSessionState::SelectingAccount;
        Ok(())
    }

    async fn require_auth_session(
        &self,
        flow_id: Uuid,
    ) -> AppResult<(String, AuthSession)> {
        let flows = self.flows.lock().await;
        let flow = Self::get(&flows, flow_id)?;
        let session = flow
            .auth_session
            .clone()
            .ok_or(AppError::BastionAuthFailed)?;
        if !matches!(
            flow.state,
            BastionSessionState::Authenticated
                | BastionSessionState::DiscoveringAssets
                | BastionSessionState::SelectingAsset
                | BastionSessionState::DiscoveringAccounts
                | BastionSessionState::SelectingAccount
                | BastionSessionState::Connecting
                | BastionSessionState::Connected
        ) {
            return Err(AppError::BastionAwaitingUser);
        }
        Ok((flow.provider.clone(), session))
    }

    async fn insert_flow(&self, flow: BastionFlow) -> AppResult<Uuid> {
        let mut flows = self.flows.lock().await;
        flows.retain(|_, item| item.expires_at > Instant::now());
        if flows.len() >= MAX_FLOWS {
            if let Some(oldest) = flows
                .iter()
                .min_by_key(|(_, item)| item.expires_at)
                .map(|(id, _)| *id)
            {
                flows.remove(&oldest);
            }
        }
        let id = Uuid::new_v4();
        flows.insert(id, flow);
        Ok(id)
    }

    fn get<'a>(
        flows: &'a HashMap<Uuid, BastionFlow>,
        flow_id: Uuid,
    ) -> AppResult<&'a BastionFlow> {
        flows
            .get(&flow_id)
            .filter(|flow| flow.expires_at > Instant::now())
            .ok_or(AppError::BastionUnavailable)
    }

    fn get_mut<'a>(
        flows: &'a mut HashMap<Uuid, BastionFlow>,
        flow_id: Uuid,
    ) -> AppResult<&'a mut BastionFlow> {
        let expired = flows
            .get(&flow_id)
            .is_some_and(|flow| flow.expires_at <= Instant::now());
        if expired {
            flows.remove(&flow_id);
            return Err(AppError::BastionUnavailable);
        }
        flows
            .get_mut(&flow_id)
            .ok_or(AppError::BastionUnavailable)
    }
}

fn flow_from_auth_step(
    flow: &mut BastionFlow,
    step: AuthStepResult,
) -> AppResult<()> {
    match step {
        AuthStepResult::Challenge { pending, challenge } => {
            flow.auth_session = Some(pending);
            flow.state = BastionSessionState::AwaitingUser { challenge };
            Ok(())
        }
        AuthStepResult::ExternalAction { pending, action } => {
            flow.auth_session = Some(pending);
            flow.state = BastionSessionState::AwaitingExternalAuth { action };
            Ok(())
        }
        AuthStepResult::Authenticated(session) => {
            flow.auth_session = Some(session);
            flow.state = BastionSessionState::Authenticated;
            Ok(())
        }
    }
}

fn snapshot_of(flow_id: Uuid, flow: &BastionFlow) -> BastionFlowSnapshot {
    let (state, challenge, external_action, error_code) = match &flow.state {
        BastionSessionState::Authenticating | BastionSessionState::Probing => {
            (BastionFlowUiState::Authenticating, None, None, None)
        }
        BastionSessionState::AwaitingUser { challenge } => {
            (BastionFlowUiState::AwaitingUser, Some(challenge.clone()), None, None)
        }
        BastionSessionState::AwaitingExternalAuth { action } => (
            BastionFlowUiState::AwaitingUser,
            None,
            Some(action.clone()),
            None,
        ),
        BastionSessionState::Authenticated => {
            (BastionFlowUiState::Authenticated, None, None, None)
        }
        BastionSessionState::DiscoveringAssets => {
            (BastionFlowUiState::DiscoveringAssets, None, None, None)
        }
        BastionSessionState::SelectingAsset => {
            (BastionFlowUiState::SelectingAsset, None, None, None)
        }
        BastionSessionState::DiscoveringAccounts => {
            (BastionFlowUiState::DiscoveringAccounts, None, None, None)
        }
        BastionSessionState::SelectingAccount => {
            (BastionFlowUiState::SelectingAccount, None, None, None)
        }
        BastionSessionState::Connecting => (BastionFlowUiState::Connecting, None, None, None),
        BastionSessionState::Connected => (BastionFlowUiState::Connected, None, None, None),
        BastionSessionState::Disconnected | BastionSessionState::Idle => {
            (BastionFlowUiState::Cancelled, None, None, None)
        }
        BastionSessionState::Failed { code } => {
            (BastionFlowUiState::Failed, None, None, Some(*code))
        }
    };
    BastionFlowSnapshot {
        flow_id,
        profile_id: flow.profile_id,
        bastion_id: flow.bastion_id,
        provider: flow.provider.clone(),
        state,
        challenge,
        external_action,
        selected_asset_id: flow
            .selected_asset
            .as_ref()
            .map(|asset| asset.remote_id.clone()),
        selected_account: flow
            .selected_account
            .as_ref()
            .map(|account| account.username.clone()),
        session_metadata: flow.connection.as_ref().map(|c| c.metadata().clone()),
        error_code,
    }
}

fn mock_or_profile_endpoint(
    bastion_id: Uuid,
    provider: &str,
    profile: &ServerProfile,
    helper_defaults: &HelperCliDefaults,
) -> BastionEndpoint {
    let ssh_port = if provider == "jumpserver" {
        crate::connection::bastion::providers::normalize_koko_ssh_port(
            if profile.port == 0 { 2222 } else { profile.port },
        )
    } else if profile.port == 0 {
        2222
    } else {
        profile.port
    };
    let (api_base_url, org_id, cli_path, cluster_name, insecure) = match &profile.connection_route {
        ConnectionRoute::Bastion {
            api_base_url,
            org_id,
            cli_path,
            cluster_name,
            insecure,
            ..
        } => (
            api_base_url
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            org_id
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            cli_path
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .or_else(|| helper_defaults.for_provider(provider)),
            cluster_name
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            *insecure,
        ),
        _ => (None, None, helper_defaults.for_provider(provider), None, false),
    };
    let (api_port, web_port, mut provider_config) = if let Some(ref base) = api_base_url {
        let parsed = crate::connection::bastion::providers::parse_api_base_url(base);
        let api_port = parsed.as_ref().map(|value| value.port);
        let mut config = serde_json::json!({ "apiBaseUrl": base.trim_end_matches('/') });
        if let Some(org_id) = org_id {
            config
                .as_object_mut()
                .expect("object")
                .insert("orgId".into(), serde_json::Value::String(org_id));
        }
        (api_port, api_port, config)
    } else if provider == "jumpserver" {
        let mut config = serde_json::json!({});
        if let Some(org_id) = org_id {
            config
                .as_object_mut()
                .expect("object")
                .insert("orgId".into(), serde_json::Value::String(org_id));
        }
        (Some(443), Some(443), config)
    } else if provider == "teleport" {
        let port = if profile.port == 0 { 443 } else { profile.port };
        (Some(port), Some(port), serde_json::json!({}))
    } else {
        (Some(443), Some(443), serde_json::json!({}))
    };
    if let Some(cli_path) = cli_path {
        provider_config
            .as_object_mut()
            .expect("object")
            .insert("cliPath".into(), serde_json::Value::String(cli_path));
    }
    if let Some(cluster_name) = cluster_name {
        provider_config
            .as_object_mut()
            .expect("object")
            .insert("clusterName".into(), serde_json::Value::String(cluster_name));
    }
    if insecure {
        provider_config
            .as_object_mut()
            .expect("object")
            .insert("insecure".into(), serde_json::Value::Bool(true));
    }
    if provider == "teleport" && !profile.username.trim().is_empty() {
        provider_config.as_object_mut().expect("object").insert(
            "teleportUser".into(),
            serde_json::Value::String(profile.username.trim().to_string()),
        );
    }
    let host = if provider == "jumpserver" {
        crate::connection::bastion::providers::normalize_koko_host(&profile.host).unwrap_or_else(
            || {
                if profile.host.trim().is_empty() {
                    "mock.bastion.local".into()
                } else {
                    profile.host.clone()
                }
            },
        )
    } else if profile.host.trim().is_empty() {
        "mock.bastion.local".into()
    } else {
        profile.host.clone()
    };
    BastionEndpoint {
        id: bastion_id,
        provider: provider.to_string(),
        name: profile.name.clone(),
        host,
        ports: BastionPorts {
            api: api_port,
            ssh: Some(ssh_port),
            web: web_port,
        },
        tls: if insecure {
            Some(crate::connection::TlsOptions {
                insecure_skip_verify: true,
                ca_cert_ref: None,
            })
        } else {
            None
        },
        provider_config,
    }
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn map_bastion_error(error: BastionError) -> AppError {
    eprintln!(
        "[runory bastion] provider error code={} ({error})",
        error.code()
    );
    tracing::warn!(
        bastion_code = error.code(),
        error = %error,
        "bastion error mapped to app error"
    );
    match error {
        BastionError::AuthenticationFailed | BastionError::MfaRequired => {
            AppError::BastionAuthFailed
        }
        BastionError::AuthenticationExpired => AppError::BastionAuthFailed,
        BastionError::PermissionDenied
        | BastionError::AccountNotAllowed
        | BastionError::AssetNotFound => AppError::BastionPermissionDenied,
        BastionError::Cancelled => AppError::BastionUnavailable,
        BastionError::ProviderUnavailable | BastionError::ProviderVersionUnsupported => {
            AppError::BastionProviderNotFound
        }
        BastionError::Network => AppError::ConnectionRefused,
        BastionError::Timeout => AppError::ConnectionTimeout,
        BastionError::HelperMissing => AppError::HelperMissing,
        BastionError::HelperVersionMismatch => AppError::HelperVersionMismatch,
        BastionError::HelperProxyFailed => AppError::HelperProxyFailed,
        // Prefer a distinct message from generic SSH lost during direct connects.
        BastionError::SessionRejected => AppError::BastionUnavailable,
        _ => AppError::BastionUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::{default_bastion_registry, AuthChallengeResponse};
    use crate::domain::{AuthMethod, ConnectionRoute};

    fn bastion_profile(asset_id: &str, account_id: Option<&str>) -> ServerProfile {
        ServerProfile {
            id: Uuid::new_v4(),
            name: "Mock Target".into(),
            host: "mock.bastion.local".into(),
            port: 2222,
            username: "alice".into(),
            group_id: None,
            auth_method: AuthMethod::Password,
            key_source: None,
            connection_route: ConnectionRoute::Bastion {
                bastion_id: Uuid::new_v4(),
                provider: "mock".into(),
                asset_id: asset_id.into(),
                account_id: account_id.map(str::to_string),
                api_base_url: None,
                org_id: None,
                cli_path: None,
                cluster_name: None,
                insecure: false,
            },
            sort_order: 0,
            created_at: "now".into(),
            updated_at: "now".into(),
            last_connected_at: None,
            os_distribution: None,
        }
    }

    #[tokio::test]
    async fn flow_awaits_totp_then_resumes_to_bound_target() {
        let service = BastionFlowService::new(Arc::new(default_bastion_registry()));
        let profile = bastion_profile("asset-prod-db-01", Some("root"));
        let snap = service
            .start(
                &profile,
                BastionCredential::password("alice", "secret"),
                HelperCliDefaults::default(),
            )
            .await
            .expect("start");
        assert_eq!(snap.state, BastionFlowUiState::AwaitingUser);
        let challenge = snap.challenge.expect("challenge");
        let snap = service
            .continue_auth(
                snap.flow_id,
                AuthChallengeResponse::Totp {
                    id: challenge.id().into(),
                    code: "123456".into(),
                },
            )
            .await
            .expect("resume");
        assert_eq!(snap.state, BastionFlowUiState::Connecting);
        assert_eq!(snap.selected_asset_id.as_deref(), Some("asset-prod-db-01"));
        assert_eq!(snap.selected_account.as_deref(), Some("root"));
        let snap = service
            .connect(snap.flow_id, 120, 40, None, None)
            .await
            .expect("connect");
        assert_eq!(snap.state, BastionFlowUiState::Connected);
        assert!(snap.session_metadata.is_some());
    }

    #[tokio::test]
    async fn flow_without_bound_asset_stops_at_selection() {
        let service = BastionFlowService::new(Arc::new(default_bastion_registry()));
        let profile = bastion_profile("", None);
        let snap = service
            .start(
                &profile,
                BastionCredential::password("alice", "secret"),
                HelperCliDefaults::default(),
            )
            .await
            .expect("start");
        let challenge = snap.challenge.expect("challenge");
        let snap = service
            .continue_auth(
                snap.flow_id,
                AuthChallengeResponse::Totp {
                    id: challenge.id().into(),
                    code: "123456".into(),
                },
            )
            .await
            .expect("resume");
        assert_eq!(snap.state, BastionFlowUiState::SelectingAsset);
        let assets = service
            .list_assets(
                snap.flow_id,
                AssetQuery {
                    search: None,
                    node: None,
                    protocol: None,
                    page: 0,
                    page_size: 10,
                },
            )
            .await
            .expect("assets");
        assert!(!assets.items.is_empty());
        let snap = service
            .select_asset(snap.flow_id, assets.items[0].remote_id.clone())
            .await
            .expect("select asset");
        assert_eq!(snap.state, BastionFlowUiState::SelectingAccount);
    }
}
