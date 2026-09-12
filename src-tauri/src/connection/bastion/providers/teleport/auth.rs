//! Teleport authentication via `tsh status` / external `tsh login`.
//!
//! Runory never stores Teleport passwords, OTP seeds, or private keys.
//! Identity material stays in `~/.tsh` managed by the official CLI.
//!
//! Working model (restored):
//! 1. If `tsh status` shows a valid short-lived cert → authenticate immediately.
//! 2. Otherwise ask the user to finish `tsh login` in a real terminal (Teleport
//!    requires a TTY for local password auth; in-app PTY automation was unstable).
//! 3. "I have signed in" re-checks `tsh status` and continues.

use std::time::Duration;

use uuid::Uuid;

use crate::connection::bastion::auth::{
    provider_cli_path, AuthChallengeResponse, AuthSession, AuthStepResult, BastionCredential,
    BastionPrincipal, ExternalAuthAction, ProtectedProviderState,
};
use crate::connection::bastion::errors::BastionError;
use crate::connection::bastion::session::BastionContext;
use crate::helper::{ExternalHelperManager, HelperError, VersionConstraint};

use super::tsh::{TeleportConnectParams, TshClient, TshStatus};

const PROVIDER: &str = "teleport";

/// Non-secret session snapshot kept in ProtectedProviderState.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeleportSessionState {
    pub proxy_addr: String,
    pub teleport_user: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_name: Option<String>,
    #[serde(default)]
    pub insecure: bool,
    #[serde(default)]
    pub os_logins: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until_ms: Option<i64>,
}

impl TeleportSessionState {
    pub fn encode(&self) -> ProtectedProviderState {
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        ProtectedProviderState::new(bytes)
    }

    pub fn decode(state: &ProtectedProviderState) -> Option<Self> {
        serde_json::from_slice(state.as_bytes()).ok()
    }

    pub fn from_status(params: &TeleportConnectParams, status: &TshStatus, username: &str) -> Self {
        Self {
            proxy_addr: params.proxy_addr.clone(),
            teleport_user: status
                .user
                .clone()
                .filter(|u| !u.is_empty())
                .unwrap_or_else(|| username.to_string()),
            cluster_name: status
                .cluster
                .clone()
                .or_else(|| params.cluster_name.clone()),
            insecure: params.insecure,
            os_logins: status.os_logins.clone(),
            valid_until_ms: status.valid_until_ms,
        }
    }

    pub fn connect_params(&self) -> TeleportConnectParams {
        TeleportConnectParams {
            proxy_addr: self.proxy_addr.clone(),
            teleport_user: Some(self.teleport_user.clone()),
            cluster_name: self.cluster_name.clone(),
            insecure: self.insecure,
        }
    }
}

pub async fn start_auth(
    helpers: &dyn ExternalHelperManager,
    ctx: &BastionContext,
    credential: &BastionCredential,
) -> Result<AuthStepResult, BastionError> {
    let cli_override = provider_cli_path(&ctx.endpoint.provider_config);
    let client = tsh_client(helpers, cli_override.as_deref())?;
    let username = credential_username(credential);
    let params = TeleportConnectParams::from_endpoint(&ctx.endpoint, Some(username.as_str()));
    if params.proxy_addr.trim().is_empty() {
        return Err(BastionError::ProviderUnavailable);
    }

    let status = client.status(&params).map_err(map_helper)?;
    if status.is_valid() {
        tracing::info!(
            proxy = %params.proxy_addr,
            user = status.user.as_deref().unwrap_or(""),
            "[TeleportAuth] reusing valid tsh session"
        );
        return Ok(authenticated_session(
            ctx.endpoint.id,
            &params,
            &status,
            &username,
            cli_override,
            None,
        ));
    }

    // Any auth mode: require an interactive `tsh login` in a real console window.
    tracing::info!(
        proxy = %params.proxy_addr,
        user = %username,
        "[TeleportAuth] no valid tsh session; opening tsh login console"
    );
    let _ = spawn_interactive_login(&client, &params);
    Ok(AuthStepResult::ExternalAction {
        pending: pending_session(ctx, &params, &username, cli_override),
        action: ExternalAuthAction::OpenBrowser {
            url: login_hint_url(&params),
            callback_uri: None,
        },
    })
}

/// Open a real OS console running `tsh login` so the user can enter password / MFA.
pub fn spawn_interactive_login(
    client: &TshClient,
    params: &TeleportConnectParams,
) -> Result<(), BastionError> {
    let args = TshClient::login_args(params).map_err(map_helper)?;
    tracing::info!(
        proxy = %params.proxy_addr,
        user = params.teleport_user.as_deref().unwrap_or(""),
        binary = %client.binary().display(),
        "[TeleportAuth] spawning interactive tsh login console"
    );

    #[cfg(windows)]
    {
        use std::process::Command;
        // `start "title" program args...` opens a new console with a real TTY.
        let mut command = Command::new("cmd.exe");
        command
            .arg("/C")
            .arg("start")
            .arg("Runory tsh login")
            .arg(client.binary())
            .args(&args);
        command.spawn().map_err(|error| {
            tracing::warn!(error = %error, "[TeleportAuth] failed to open tsh login console");
            BastionError::HelperProxyFailed
        })?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let binary = client.binary().display().to_string();
        let joined = std::iter::once(binary.as_str())
            .chain(args.iter().map(String::as_str))
            .map(|part| format!("'{}'", part.replace('\'', "'\\''")))
            .collect::<Vec<_>>()
            .join(" ");
        let script = format!("tell application \"Terminal\" to do script \"{joined}\"");
        Command::new("osascript")
            .arg("-e")
            .arg(script)
            .spawn()
            .map_err(|_| BastionError::HelperProxyFailed)?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use std::process::Command;
        // Best-effort: common terminal emulators.
        for term in ["x-terminal-emulator", "gnome-terminal", "konsole", "xterm"] {
            let mut command = Command::new(term);
            if term == "gnome-terminal" {
                command.arg("--");
            } else if term == "konsole" {
                command.arg("-e");
            } else if term == "x-terminal-emulator" {
                command.arg("-e");
            }
            command.arg(client.binary()).args(&args);
            if command.spawn().is_ok() {
                return Ok(());
            }
        }
        return Err(BastionError::HelperProxyFailed);
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = (client, params);
        Err(BastionError::CapabilityUnavailable)
    }
}

pub async fn continue_auth(
    helpers: &dyn ExternalHelperManager,
    session: &AuthSession,
    response: AuthChallengeResponse,
) -> Result<AuthStepResult, BastionError> {
    if session.provider != PROVIDER {
        return Err(BastionError::ProviderUnavailable);
    }
    let pending = TeleportSessionState::decode(&session.provider_state).unwrap_or(
        TeleportSessionState {
            proxy_addr: String::new(),
            teleport_user: session.principal.username.clone(),
            cluster_name: None,
            insecure: false,
            os_logins: Vec::new(),
            valid_until_ms: None,
        },
    );
    let params = if pending.proxy_addr.trim().is_empty() {
        TeleportConnectParams {
            proxy_addr: String::new(),
            teleport_user: Some(session.principal.username.clone()),
            cluster_name: pending.cluster_name.clone(),
            insecure: pending.insecure,
        }
    } else {
        pending.connect_params()
    };

    match response {
        AuthChallengeResponse::Cancel { .. } => Err(BastionError::Cancelled),
        AuthChallengeResponse::ExternalCompleted { .. }
        | AuthChallengeResponse::Password { .. }
        | AuthChallengeResponse::Confirm { .. } => {
            let client = tsh_client(helpers, session.helper_cli_override.as_deref())?;
            wait_for_valid_status(&client, &params, Duration::from_secs(90))
                .await
                .map(|status| {
                    authenticated_session(
                        session.bastion_id,
                        &params,
                        &status,
                        &session.principal.username,
                        session.helper_cli_override.clone(),
                        Some(session.id),
                    )
                })
        }
        AuthChallengeResponse::Totp { .. } | AuthChallengeResponse::SmsCode { .. } => {
            Err(BastionError::MfaRequired)
        }
        _ => Err(BastionError::ProviderProtocolError),
    }
}

fn tsh_client(
    helpers: &dyn ExternalHelperManager,
    override_path: Option<&str>,
) -> Result<TshClient, BastionError> {
    let binary = helpers
        .locate_binary_with_override(override_path, &["tsh", "tsh.exe"])
        .map_err(map_helper)?;
    let _ = helpers
        .check_version(&binary, &VersionConstraint::at_least(14, 0))
        .map_err(map_helper)?;
    Ok(TshClient::new(binary))
}

fn credential_username(credential: &BastionCredential) -> String {
    match credential {
        BastionCredential::BrowserSso { username_hint } => username_hint
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "teleport-user".into()),
        BastionCredential::Password { username, .. } => username.clone(),
        BastionCredential::ExternalAgent { username } => username.clone(),
        _ => "teleport-user".into(),
    }
}

fn pending_session(
    ctx: &BastionContext,
    params: &TeleportConnectParams,
    username: &str,
    cli_override: Option<String>,
) -> AuthSession {
    let state = TeleportSessionState {
        proxy_addr: params.proxy_addr.clone(),
        teleport_user: username.to_string(),
        cluster_name: params.cluster_name.clone(),
        insecure: params.insecure,
        os_logins: Vec::new(),
        valid_until_ms: None,
    };
    AuthSession {
        id: Uuid::new_v4(),
        bastion_id: ctx.endpoint.id,
        provider: PROVIDER.into(),
        principal: BastionPrincipal {
            username: username.to_string(),
            display_name: None,
        },
        provider_state: state.encode(),
        expires_at: None,
        helper_cli_override: cli_override,
    }
}

fn authenticated_session(
    bastion_id: Uuid,
    params: &TeleportConnectParams,
    status: &TshStatus,
    username: &str,
    cli_override: Option<String>,
    session_id: Option<Uuid>,
) -> AuthStepResult {
    let state = TeleportSessionState::from_status(params, status, username);
    AuthStepResult::Authenticated(AuthSession {
        id: session_id.unwrap_or_else(Uuid::new_v4),
        bastion_id,
        provider: PROVIDER.into(),
        principal: BastionPrincipal {
            username: state.teleport_user.clone(),
            display_name: status.cluster.clone(),
        },
        provider_state: state.encode(),
        expires_at: state.valid_until_ms,
        helper_cli_override: cli_override,
    })
}

fn login_hint_url(params: &TeleportConnectParams) -> String {
    let addr = params.proxy_addr.trim();
    if addr.starts_with("https://") || addr.starts_with("http://") {
        addr.to_string()
    } else {
        format!("https://{addr}")
    }
}

async fn wait_for_valid_status(
    client: &TshClient,
    params: &TeleportConnectParams,
    timeout: Duration,
) -> Result<TshStatus, BastionError> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let status = client.status(params).map_err(map_helper)?;
        if status.is_valid() {
            return Ok(status);
        }
        if std::time::Instant::now() >= deadline {
            tracing::warn!(
                proxy = %params.proxy_addr,
                "[TeleportAuth] still not logged in — run: tsh login --proxy={} --user={}{}",
                params.proxy_addr,
                params.teleport_user.as_deref().unwrap_or("USER"),
                if params.insecure { " --insecure" } else { "" }
            );
            return Err(BastionError::AuthenticationFailed);
        }
        tokio::time::sleep(Duration::from_millis(750)).await;
    }
}

fn map_helper(error: HelperError) -> BastionError {
    match error {
        HelperError::Missing => BastionError::HelperMissing,
        HelperError::VersionMismatch => BastionError::HelperVersionMismatch,
        HelperError::Cancelled => BastionError::Cancelled,
        _ => BastionError::HelperProxyFailed,
    }
}
