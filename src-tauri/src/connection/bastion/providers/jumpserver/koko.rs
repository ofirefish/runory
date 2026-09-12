use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;
use zeroize::Zeroizing;

use crate::connection::bastion::errors::BastionError;
use crate::connection::bastion::session::{BastionSessionMetadata, BastionSshSession};
use crate::domain::{ConnectRequest, SshAuthentication, SshConnectionRequest};
use crate::ssh::SshService;

use super::api::JumpServerConnectionToken;

/// Opens an interactive PTY on JumpServer KoKo.
pub struct KokoClient;

impl KokoClient {
    /// Token login: username `JMS-{token.id}`, password = token value.
    ///
    /// KoKo authenticates by fetching the connection-token secret and comparing `value`.
    /// One-time tokens are consumed on that fetch — use password-only auth (no kbd-int retry).
    pub async fn connect_with_token(
        host: &str,
        port: u16,
        token: &JumpServerConnectionToken,
        expected_fingerprint: &str,
        asset_id: &str,
        account: &str,
        cols: u16,
        rows: u16,
    ) -> Result<BastionSshSession, BastionError> {
        if expected_fingerprint.trim().is_empty() {
            return Err(BastionError::SessionRejected);
        }
        if token.id.trim().is_empty() || token.value.trim().is_empty() {
            tracing::warn!(
                token_id_empty = token.id.trim().is_empty(),
                token_value_empty = token.value.trim().is_empty(),
                "jumpserver connection token missing id or value"
            );
            return Err(BastionError::SessionRejected);
        }
        let username = format!("JMS-{}", token.id.trim());
        let password = Zeroizing::new(token.value.trim().to_string());
        tracing::info!(
            koko_host = %host,
            koko_port = port,
            token_id = %token.id,
            username = %username,
            "jumpserver koko ssh authenticate (connection token)"
        );
        open_gateway(
            host,
            port,
            username,
            password,
            expected_fingerprint,
            asset_id,
            account,
            cols,
            rows,
            true,
        )
        .await
    }

    /// Direct login: `JumpServerUser@account@assetId` with JumpServer password.
    ///
    /// Documented at https://www.jumpserver.com/blog/connecting-via-ssh-terminal
    pub async fn connect_with_password(
        host: &str,
        port: u16,
        username: &str,
        password: &str,
        expected_fingerprint: &str,
        asset_id: &str,
        account: &str,
        cols: u16,
        rows: u16,
    ) -> Result<BastionSshSession, BastionError> {
        if expected_fingerprint.trim().is_empty()
            || username.trim().is_empty()
            || password.is_empty()
        {
            return Err(BastionError::SessionRejected);
        }
        tracing::info!(
            koko_host = %host,
            koko_port = port,
            username = %username,
            "jumpserver koko ssh authenticate (direct login)"
        );
        open_gateway(
            host,
            port,
            username.trim().to_string(),
            Zeroizing::new(password.to_string()),
            expected_fingerprint,
            asset_id,
            account,
            cols,
            rows,
            false,
        )
        .await
    }
}

async fn open_gateway(
    host: &str,
    port: u16,
    username: String,
    password: Zeroizing<String>,
    expected_fingerprint: &str,
    asset_id: &str,
    account: &str,
    cols: u16,
    rows: u16,
    password_only: bool,
) -> Result<BastionSshSession, BastionError> {
    let opened = if password_only {
        SshService::connect_bastion_gateway_password_only(
            ConnectRequest {
                connection: SshConnectionRequest {
                    host: host.to_string(),
                    port,
                    username,
                    authentication: SshAuthentication::Password { password },
                },
                cols: u32::from(cols.max(1)),
                rows: u32::from(rows.max(1)),
            },
            expected_fingerprint.to_string(),
        )
        .await
    } else {
        SshService::connect_bastion_gateway(
            ConnectRequest {
                connection: SshConnectionRequest {
                    host: host.to_string(),
                    port,
                    username,
                    authentication: SshAuthentication::Password { password },
                },
                cols: u32::from(cols.max(1)),
                rows: u32::from(rows.max(1)),
            },
            expected_fingerprint.to_string(),
        )
        .await
    }
    .map_err(map_ssh_error)?;

    let connection_id = format!("jms-{}", Uuid::new_v4());
    let metadata = BastionSessionMetadata {
        session_id: Some(connection_id.clone()),
        provider: "jumpserver".into(),
        asset_id: asset_id.to_string(),
        account: account.to_string(),
        recording: true,
        command_audit: true,
        file_audit: false,
        started_at: now_millis(),
    };
    Ok(BastionSshSession {
        connection_id,
        metadata,
        client: opened.client,
        writer: Arc::new(opened.writer),
        reader: Some(opened.reader),
        transport_cleanup: None,
    })
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn map_ssh_error(error: crate::domain::AppError) -> BastionError {
    eprintln!(
        "[runory jumpserver] koko ssh failed code={} ({error})",
        error.code()
    );
    tracing::warn!(
        code = error.code(),
        error = %error,
        "jumpserver koko ssh mapped to bastion error"
    );
    match error {
        crate::domain::AppError::AuthFailed => BastionError::AuthenticationFailed,
        crate::domain::AppError::ConnectionTimeout => BastionError::Timeout,
        crate::domain::AppError::ConnectionRefused => BastionError::Network,
        // KoKo often drops the channel when token/account pairing is invalid.
        crate::domain::AppError::ConnectionLost => BastionError::SessionRejected,
        crate::domain::AppError::HostKeyUnknown | crate::domain::AppError::HostKeyChanged => {
            BastionError::SessionRejected
        }
        _ => BastionError::SessionRejected,
    }
}
