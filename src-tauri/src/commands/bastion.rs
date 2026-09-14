use serde::Deserialize;
use tauri::{ipc::Channel, State};
use uuid::Uuid;

use crate::connection::{
    AssetQuery, AuthChallengeResponse, BastionFlowService, BastionFlowSnapshot,
    BastionSessionBridge, HelperCliDefaults,
};
use crate::credentials::CredentialService;
use crate::domain::{
    AppError, AppResult, ConnectResponse, ConnectionRoute, CredentialInput, CredentialKind,
    HostVerification, TerminalEvent,
};
use crate::known_hosts::KnownHostService;
use crate::profiles::ProfileService;
use crate::settings::SettingsService;
use crate::ssh::SshService;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BastionStartRequest {
    pub profile_id: Uuid,
    /// `password` (default), `accessKey`, `browserSso`, or `token`.
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub access_key_id: String,
    #[serde(default)]
    pub access_key_secret: String,
    /// JumpServer Access Key as CredentialInput (`id:secret` in the secret field).
    #[serde(default)]
    pub access_key_credential: Option<CredentialInput>,
    #[serde(default)]
    pub token: String,
    /// Preferred Boundary token input (session-only / remember-securely / stored).
    #[serde(default)]
    pub token_credential: Option<CredentialInput>,
}

fn default_auth_mode() -> String {
    "password".into()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BastionFlowIdRequest {
    pub flow_id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BastionContinueAuthRequest {
    pub flow_id: Uuid,
    pub response: AuthChallengeResponse,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BastionListAssetsRequest {
    pub flow_id: Uuid,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

fn default_page_size() -> u32 {
    20
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BastionSelectAssetRequest {
    pub flow_id: Uuid,
    pub asset_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BastionSelectAccountRequest {
    pub flow_id: Uuid,
    pub account: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BastionConnectFlowRequest {
    pub flow_id: Uuid,
    pub profile_id: Uuid,
    pub cols: u32,
    pub rows: u32,
    #[serde(default)]
    pub verification_attempt_id: Option<Uuid>,
    /// Optional one-shot SSH password for Boundary local-proxy targets (legacy).
    #[serde(default)]
    pub ssh_password: Option<String>,
    /// Preferred target SSH password input (session-only / remember-securely / stored).
    #[serde(default)]
    pub ssh_password_credential: Option<CredentialInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BastionPrepareHostRequest {
    pub profile_id: Uuid,
    pub flow_id: Uuid,
}

#[tauri::command]
pub async fn bastion_prepare_host(
    request: BastionPrepareHostRequest,
    profiles: State<'_, ProfileService>,
    known_hosts: State<'_, KnownHostService>,
    flows: State<'_, BastionFlowService>,
) -> AppResult<HostVerification> {
    let profile = profiles.get(request.profile_id).await?;
    if !matches!(profile.connection_route, ConnectionRoute::Bastion { .. }) {
        return Err(AppError::InvalidProfile);
    }
    let snap = flows.snapshot(request.flow_id).await?;
    if snap.profile_id != request.profile_id {
        return Err(AppError::InvalidProfile);
    }
    let (host, port) = flows.resolve_gateway_endpoint(request.flow_id).await?;
    eprintln!(
        "[runory bastion] stage=prepare_host profile_id={} flow_id={} host={} port={}",
        request.profile_id, request.flow_id, host, port
    );
    tracing::info!(
        profile_id = %request.profile_id,
        flow_id = %request.flow_id,
        host = %host,
        port,
        "bastion prepare host key"
    );
    let observed = match SshService::scan_bastion_gateway_host_key(&host, port).await {
        Ok(observed) => observed,
        Err(error) => {
            eprintln!(
                "[runory bastion] stage=prepare_host FAILED code={} host={} port={} endpoint={} hint=confirm_koko_ssh_port_mapping",
                error.code(),
                host,
                port,
                error.endpoint().unwrap_or("n/a"),
            );
            return Err(error);
        }
    };
    known_hosts
        .prepare_scoped("bastion", &host, port, observed)
        .await
}

#[tauri::command]
pub async fn bastion_start(
    request: BastionStartRequest,
    profiles: State<'_, ProfileService>,
    flows: State<'_, BastionFlowService>,
    settings: State<'_, SettingsService>,
    credentials: State<'_, CredentialService>,
) -> AppResult<BastionFlowSnapshot> {
    let profile = profiles.get(request.profile_id).await?;
    if !matches!(profile.connection_route, ConnectionRoute::Bastion { .. }) {
        return Err(AppError::InvalidProfile);
    }
    let mut token_to_remember: Option<zeroize::Zeroizing<String>> = None;
    let mut access_key_to_remember: Option<zeroize::Zeroizing<String>> = None;
    let credential = match request.auth_mode.trim().to_ascii_lowercase().as_str() {
        "accesskey" | "access_key" | "access-key" => {
            let combined = if request.access_key_id.trim().is_empty() {
                request.access_key_secret
            } else if request.access_key_secret.trim().is_empty() {
                request.access_key_id
            } else {
                format!(
                    "{}:{}",
                    request.access_key_id.trim(),
                    request.access_key_secret.trim()
                )
            };
            let input = request
                .access_key_credential
                .unwrap_or(CredentialInput::SessionOnly { secret: combined });
            let resolved = credentials
                .resolve_for_profile(profile.id, CredentialKind::BastionAccessKey, input, false)
                .await?;
            if resolved.remember_after_auth {
                access_key_to_remember = Some(resolved.secret.clone());
            }
            let (key_id, secret) = split_access_key_material(resolved.secret.as_str());
            crate::connection::BastionCredential::access_key(key_id, secret)
        }
        "browsersso" | "browser_sso" | "browser-sso" | "sso" => {
            crate::connection::BastionCredential::browser_sso(non_empty_string(request.username))
        }
        "token" => {
            let input = request
                .token_credential
                .unwrap_or(CredentialInput::SessionOnly {
                    secret: request.token,
                });
            let resolved = credentials
                .resolve_for_profile(profile.id, CredentialKind::BastionToken, input, false)
                .await?;
            if resolved.remember_after_auth {
                token_to_remember = Some(resolved.secret.clone());
            }
            crate::connection::BastionCredential::token_with_username(
                resolved.secret.as_str(),
                non_empty_string(request.username),
            )
        }
        _ => crate::connection::BastionCredential::password(request.username, request.password),
    };
    let helper_defaults = match settings.get().await {
        Ok(app_settings) => HelperCliDefaults::from_settings(&app_settings),
        Err(error) => {
            tracing::warn!(
                error_code = error.code(),
                "bastion start could not load helper CLI settings; falling back to PATH"
            );
            HelperCliDefaults::default()
        }
    };
    let snap = flows.start(&profile, credential, helper_defaults).await?;
    let should_save_secret = !matches!(
        snap.state,
        crate::connection::BastionFlowUiState::Failed
            | crate::connection::BastionFlowUiState::Cancelled
            | crate::connection::BastionFlowUiState::Authenticating
    );
    if let Some(secret) = token_to_remember {
        if should_save_secret {
            if let Err(error) = credentials
                .remember(profile.id, CredentialKind::BastionToken, secret)
                .await
            {
                tracing::warn!(
                    profile_id = %profile.id,
                    error_code = error.code(),
                    "could not remember bastion token"
                );
            }
        }
    }
    if let Some(secret) = access_key_to_remember {
        if should_save_secret {
            if let Err(error) = credentials
                .remember(profile.id, CredentialKind::BastionAccessKey, secret)
                .await
            {
                tracing::warn!(
                    profile_id = %profile.id,
                    error_code = error.code(),
                    "could not remember bastion access key"
                );
            }
        }
    }
    Ok(snap)
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Split JumpServer vault material (`id:secret`) for BastionCredential.
fn split_access_key_material(combined: &str) -> (String, String) {
    let combined = combined.trim();
    if let Some((id, secret)) = combined.split_once(':') {
        let id = id.trim();
        let secret = secret.trim();
        if !id.is_empty() && !secret.is_empty() {
            return (id.to_string(), secret.to_string());
        }
    }
    (combined.to_string(), String::new())
}

#[cfg(test)]
mod tests {
    use super::split_access_key_material;

    #[test]
    fn splits_combined_access_key_material() {
        let (id, secret) = split_access_key_material("  ak-id:super-secret  ");
        assert_eq!(id, "ak-id");
        assert_eq!(secret, "super-secret");
    }

    #[test]
    fn keeps_colon_inside_secret() {
        let (id, secret) = split_access_key_material("id:sec:ret");
        assert_eq!(id, "id");
        assert_eq!(secret, "sec:ret");
    }
}

#[tauri::command]
pub async fn bastion_continue_auth(
    request: BastionContinueAuthRequest,
    flows: State<'_, BastionFlowService>,
) -> AppResult<BastionFlowSnapshot> {
    flows.continue_auth(request.flow_id, request.response).await
}

#[tauri::command]
pub async fn bastion_open_external_browser(
    request: BastionFlowIdRequest,
    flows: State<'_, BastionFlowService>,
) -> AppResult<()> {
    flows.open_external_browser(request.flow_id).await
}

#[tauri::command]
pub async fn bastion_list_assets(
    request: BastionListAssetsRequest,
    flows: State<'_, BastionFlowService>,
) -> AppResult<crate::connection::AssetPage> {
    flows
        .list_assets(
            request.flow_id,
            AssetQuery {
                search: request.search,
                node: None,
                protocol: None,
                page: request.page,
                page_size: request.page_size.max(1),
            },
        )
        .await
}

#[tauri::command]
pub async fn bastion_select_asset(
    request: BastionSelectAssetRequest,
    flows: State<'_, BastionFlowService>,
) -> AppResult<BastionFlowSnapshot> {
    flows.select_asset(request.flow_id, request.asset_id).await
}

#[tauri::command]
pub async fn bastion_list_accounts(
    request: BastionFlowIdRequest,
    flows: State<'_, BastionFlowService>,
) -> AppResult<Vec<crate::connection::BastionAccount>> {
    flows.list_accounts(request.flow_id).await
}

#[tauri::command]
pub async fn bastion_select_account(
    request: BastionSelectAccountRequest,
    flows: State<'_, BastionFlowService>,
) -> AppResult<BastionFlowSnapshot> {
    flows.select_account(request.flow_id, request.account).await
}

#[tauri::command]
pub async fn bastion_connect_flow(
    request: BastionConnectFlowRequest,
    on_event: Channel<TerminalEvent>,
    flows: State<'_, BastionFlowService>,
    bridge: State<'_, BastionSessionBridge>,
    profiles: State<'_, ProfileService>,
    known_hosts: State<'_, KnownHostService>,
    credentials: State<'_, CredentialService>,
) -> AppResult<ConnectResponse> {
    let profile = profiles.get(request.profile_id).await?;
    let snap = flows.snapshot(request.flow_id).await?;
    if snap.profile_id != request.profile_id {
        return Err(AppError::InvalidProfile);
    }
    eprintln!(
        "[runory bastion] stage=connect_flow flow_id={} provider={} host={} port={} cols={} rows={}",
        request.flow_id,
        snap.provider,
        profile.host,
        profile.port,
        request.cols,
        request.rows
    );
    tracing::info!(
        flow_id = %request.flow_id,
        profile_id = %request.profile_id,
        provider = %snap.provider,
        host = %profile.host,
        port = profile.port,
        "bastion connect flow start"
    );
    let fingerprint = if snap.provider == "teleport" || snap.provider == "boundary" {
        None
    } else {
        let attempt_id = request
            .verification_attempt_id
            .ok_or(AppError::HostVerificationExpired)?;
        let (host, port) = flows
            .gateway_endpoint(request.flow_id)
            .await
            .unwrap_or((profile.host.clone(), profile.port));
        Some(
            known_hosts
                .consume_scoped(attempt_id, "bastion", &host, port)
                .await?,
        )
    };
    let cols = u16::try_from(request.cols).unwrap_or(120);
    let rows = u16::try_from(request.rows).unwrap_or(40);

    let mut password_to_remember: Option<zeroize::Zeroizing<String>> = None;
    let ssh_password = if let Some(input) = request.ssh_password_credential {
        let resolved = credentials
            .resolve_for_profile(
                profile.id,
                CredentialKind::BastionTargetPassword,
                input,
                false,
            )
            .await?;
        if resolved.remember_after_auth {
            password_to_remember = Some(resolved.secret.clone());
        }
        Some(resolved.secret)
    } else {
        request.ssh_password.map(zeroize::Zeroizing::new)
    };

    let snap = match flows
        .connect(request.flow_id, cols, rows, fingerprint, ssh_password)
        .await
    {
        Ok(snap) => snap,
        Err(error) => {
            eprintln!(
                "[runory bastion] stage=provider_connect FAILED code={}",
                error.code()
            );
            let (host, port) = flows
                .gateway_endpoint(request.flow_id)
                .await
                .unwrap_or((profile.host.clone(), profile.port));
            return Err(enrich_connect_gateway_error(&host, port, error));
        }
    };
    if snap.state != crate::connection::BastionFlowUiState::Connected {
        eprintln!("[runory bastion] stage=provider_connect unexpected state (not connected)");
        return Err(AppError::BastionUnavailable);
    }
    let (connection, provider) = flows
        .take_connection(request.flow_id, request.profile_id)
        .await?;
    let session_id = match bridge
        .open(request.profile_id, provider, connection, on_event)
        .await
    {
        Ok(session_id) => session_id,
        Err(error) => {
            eprintln!(
                "[runory bastion] stage=session_bridge FAILED code={}",
                error.code()
            );
            return Err(error);
        }
    };
    let mut credential_saved = true;
    if let Some(secret) = password_to_remember {
        if let Err(error) = credentials
            .remember(profile.id, CredentialKind::BastionTargetPassword, secret)
            .await
        {
            tracing::warn!(
                profile_id = %profile.id,
                error_code = error.code(),
                "could not remember bastion target password"
            );
            credential_saved = false;
        }
    }
    eprintln!("[runory bastion] stage=connect_flow OK session_id={session_id}");
    Ok(ConnectResponse {
        session_id,
        credential_saved,
    })
}

#[tauri::command]
pub async fn bastion_cancel_flow(
    request: BastionFlowIdRequest,
    flows: State<'_, BastionFlowService>,
) -> AppResult<()> {
    flows.cancel(request.flow_id).await;
    Ok(())
}

fn enrich_connect_gateway_error(host: &str, port: u16, error: AppError) -> AppError {
    let endpoint = crate::domain::format_ssh_endpoint(host, port);
    match error {
        AppError::ConnectionRefused | AppError::ConnectionTimeout => {
            AppError::BastionGatewayUnreachable { endpoint }
        }
        other => other,
    }
}
