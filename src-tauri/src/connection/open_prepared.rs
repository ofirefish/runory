//! Open a [`PreparedConnection`] into an interactive SSH PTY session.
//!
//! Used by Teleport / Boundary (and eventually JumpServer) so providers only
//! describe transport + auth, while SSH Core owns the handshake and shell.

use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use russh::client;
use russh::keys::{Algorithm, Certificate, EcdsaCurve, HashAlg, PrivateKeyWithHashAlg};
use russh::{ChannelReadHalf, ChannelWriteHalf, Disconnect, Preferred};
use uuid::Uuid;

use crate::connection::bastion::{
    BastionConnection, BastionError, BastionSessionMetadata, BastionSshSession,
};
use crate::connection::prepared::{PreparedConnection, SshAuthPlan};
use crate::connection::transport::{OpenedTransport, TransportContext, TransportFactory};
use crate::connection::HostIdentityPolicy;
use crate::domain::HostKeyInfo;
use crate::ssh::{map_russh_error, HostKeyHandler};

/// Result of opening a prepared connection: interactive SSH session.
pub struct OpenedPreparedSession {
    pub connection: BastionConnection,
}

pub async fn open_prepared_ssh_session(
    prepared: PreparedConnection,
    cols: u16,
    rows: u16,
) -> Result<OpenedPreparedSession, BastionError> {
    if cols == 0 || rows == 0 {
        return Err(BastionError::SessionRejected);
    }
    let ctx = TransportContext::with_timeout(45);
    let opened = TransportFactory::open(&prepared.transport, &ctx)
        .await
        .map_err(map_app_to_bastion)?;
    open_prepared_ssh_session_on_transport(prepared, opened, cols, rows).await
}

pub async fn open_prepared_ssh_session_on_transport(
    prepared: PreparedConnection,
    opened: OpenedTransport,
    cols: u16,
    rows: u16,
) -> Result<OpenedPreparedSession, BastionError> {
    if cols == 0 || rows == 0 {
        return Err(BastionError::SessionRejected);
    }
    let OpenedTransport {
        stream,
        cleanup,
        helper_ready_payload: _,
    } = opened;

    let expected = match &prepared.ssh.host_identity {
        HostIdentityPolicy::KnownHost { .. } => {
            // Fingerprint may be attached later via connect options; allow observe-first
            // when ProviderManaged / first helper connect. KnownHost without prior trust
            // still records the observed key (caller may have skipped UI verify).
            None
        }
        HostIdentityPolicy::HostCertificateAuthority { .. }
        | HostIdentityPolicy::ProviderManaged { .. } => None,
    };

    let observed = Arc::new(Mutex::new(None::<HostKeyInfo>));
    let handler = HostKeyHandler::new(expected, Arc::clone(&observed));
    let prefer_host_certs = matches!(prepared.ssh.auth, SshAuthPlan::OpenSshCert { .. })
        || matches!(
            &prepared.ssh.host_identity,
            HostIdentityPolicy::ProviderManaged { provider_id, .. } if provider_id == "teleport"
        )
        || matches!(
            &prepared.ssh.host_identity,
            HostIdentityPolicy::HostCertificateAuthority { .. }
        );
    let config = Arc::new(if prepared.ssh.bastion_gateway {
        bastion_like_config()
    } else {
        client::Config {
            inactivity_timeout: Some(Duration::from_secs(60)),
            keepalive_interval: Some(Duration::from_secs(15)),
            nodelay: true,
            preferred: if prefer_host_certs {
                teleport_like_preferred()
            } else {
                Preferred::DEFAULT
            },
            ..Default::default()
        }
    });

    let mut client = tokio::time::timeout(
        Duration::from_secs(45),
        client::connect_stream(config, stream, handler),
    )
    .await
    .map_err(|_| BastionError::Timeout)?
    .map_err(|error| {
        tracing::warn!(error = %error, "prepared connect_stream failed");
        // Teleport often fails KEX with NoCommonAlgo when host certs are disabled.
        let msg = error.to_string();
        if msg.contains("NoCommonAlgo") {
            BastionError::SessionRejected
        } else {
            BastionError::Network
        }
    })?;

    authenticate_prepared(&mut client, &prepared.ssh.auth, prepared.ssh.password_only).await?;

    let channel = client
        .channel_open_session()
        .await
        .map_err(|_| BastionError::SessionRejected)?;
    channel
        .request_pty(
            false,
            "xterm-256color",
            u32::from(cols),
            u32::from(rows),
            0,
            0,
            &[],
        )
        .await
        .map_err(|_| BastionError::SessionRejected)?;
    channel
        .request_shell(false)
        .await
        .map_err(|_| BastionError::SessionRejected)?;
    let (reader, writer): (ChannelReadHalf, ChannelWriteHalf<client::Msg>) = channel.split();

    let connection_id = prepared
        .lifecycle
        .as_ref()
        .map(|h| h.session_id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let provider = prepared
        .audit
        .as_ref()
        .and_then(|a| a.provider.clone())
        .unwrap_or_else(|| "bastion".into());
    let asset_id = prepared
        .audit
        .as_ref()
        .and_then(|a| a.asset_id.clone())
        .unwrap_or_else(|| prepared.logical_target.stable_id.clone());
    let account = match &prepared.ssh.auth {
        SshAuthPlan::Password { username, .. }
        | SshAuthPlan::PrivateKey { username, .. }
        | SshAuthPlan::ProviderSupplied { username, .. }
        | SshAuthPlan::Deferred { username }
        | SshAuthPlan::OpenSshCert { username, .. } => username.clone(),
    };
    let metadata = BastionSessionMetadata {
        session_id: Some(connection_id.clone()),
        provider,
        asset_id,
        account,
        recording: prepared
            .audit
            .as_ref()
            .and_then(|a| a.recording)
            .unwrap_or(false),
        command_audit: true,
        file_audit: false,
        started_at: now_millis(),
    };

    Ok(OpenedPreparedSession {
        connection: BastionConnection::SshInteractive {
            session: BastionSshSession {
                connection_id,
                metadata,
                client,
                writer: Arc::new(writer),
                reader: Some(reader),
                transport_cleanup: cleanup,
            },
        },
    })
}

async fn authenticate_prepared(
    client: &mut client::Handle<HostKeyHandler>,
    auth: &SshAuthPlan,
    password_only: bool,
) -> Result<(), BastionError> {
    let ok = match auth {
        SshAuthPlan::Password { username, password }
        | SshAuthPlan::ProviderSupplied { username, password } => {
            if password_only {
                client
                    .authenticate_password(username, password.as_str())
                    .await
                    .map_err(|_| BastionError::AuthenticationFailed)?
                    .success()
            } else {
                authenticate_password_or_keyboard(client, username, password.as_str()).await?
            }
        }
        SshAuthPlan::PrivateKey {
            username,
            key_material,
            passphrase,
        } => {
            let key = load_key(key_material, passphrase.as_ref())?;
            let hash = client
                .best_supported_rsa_hash()
                .await
                .map_err(|_| BastionError::AuthenticationFailed)?
                .flatten();
            client
                .authenticate_publickey(username, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                .await
                .map_err(|_| BastionError::AuthenticationFailed)?
                .success()
        }
        SshAuthPlan::Deferred { username } => {
            // Best-effort agent, then none. Prefer OpenSshCert for Teleport.
            if try_agent_auth(client, username).await? {
                true
            } else {
                client
                    .authenticate_none(username)
                    .await
                    .map_err(|_| BastionError::AuthenticationFailed)?
                    .success()
            }
        }
        SshAuthPlan::OpenSshCert {
            username,
            key_material,
            certificate,
        } => {
            let key = load_key(key_material, None)?;
            let cert = parse_openssh_certificate(certificate)?;
            client
                .authenticate_openssh_cert(username, Arc::new(key), cert)
                .await
                .map_err(|_| BastionError::AuthenticationFailed)?
                .success()
        }
    };
    if !ok {
        let _ = client
            .disconnect(Disconnect::ByApplication, "authentication failed", "en")
            .await;
        return Err(BastionError::AuthenticationFailed);
    }
    Ok(())
}

async fn authenticate_password_or_keyboard(
    client: &mut client::Handle<HostKeyHandler>,
    username: &str,
    password: &str,
) -> Result<bool, BastionError> {
    let password_auth = client
        .authenticate_password(username, password)
        .await
        .map_err(|_| BastionError::AuthenticationFailed)?;
    if password_auth.success() {
        return Ok(true);
    }
    let mut response = client
        .authenticate_keyboard_interactive_start(username, None)
        .await
        .map_err(|_| BastionError::AuthenticationFailed)?;
    for _ in 0..8 {
        match response {
            client::KeyboardInteractiveAuthResponse::Success => return Ok(true),
            client::KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
            client::KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                let answers = prompts.iter().map(|_| password.to_string()).collect();
                response = client
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                    .map_err(|_| BastionError::AuthenticationFailed)?;
            }
        }
    }
    Ok(false)
}

async fn try_agent_auth(
    client: &mut client::Handle<HostKeyHandler>,
    username: &str,
) -> Result<bool, BastionError> {
    // Agent auth is best-effort; absence is not an error for Deferred plans.
    let _ = (client, username);
    Ok(false)
}

fn load_key(
    material: &[u8],
    passphrase: Option<&zeroize::Zeroizing<String>>,
) -> Result<russh::keys::PrivateKey, BastionError> {
    let encoded = std::str::from_utf8(material).map_err(|_| BastionError::AuthenticationFailed)?;
    let passphrase_ref = passphrase.and_then(|value| {
        if value.is_empty() {
            None
        } else {
            Some(value.as_str())
        }
    });
    russh::keys::decode_secret_key(encoded, passphrase_ref)
        .map_err(|_| BastionError::AuthenticationFailed)
}

fn parse_openssh_certificate(material: &[u8]) -> Result<Certificate, BastionError> {
    let text = std::str::from_utf8(material).map_err(|_| BastionError::AuthenticationFailed)?;
    Certificate::from_openssh(text).map_err(|_| BastionError::AuthenticationFailed)
}

fn bastion_like_config() -> client::Config {
    use russh::SshId;
    client::Config {
        client_id: SshId::Standard(Cow::Borrowed("SSH-2.0-OpenSSH_9.6")),
        keepalive_interval: None,
        keepalive_max: 0,
        inactivity_timeout: None,
        nodelay: true,
        preferred: teleport_like_preferred(),
        ..Default::default()
    }
}

/// Prefer OpenSSH host-certificate algorithms so Teleport Proxy / Nodes can negotiate.
fn teleport_like_preferred() -> Preferred {
    Preferred {
        host_key_certificates: Cow::Borrowed(&[
            Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP256,
            },
            Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP384,
            },
            Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP521,
            },
            Algorithm::Ed25519,
            Algorithm::Rsa {
                hash: Some(HashAlg::Sha512),
            },
            Algorithm::Rsa {
                hash: Some(HashAlg::Sha256),
            },
        ]),
        ..Preferred::DEFAULT
    }
}

fn map_app_to_bastion(error: crate::domain::AppError) -> BastionError {
    match error {
        crate::domain::AppError::ConnectionTimeout => BastionError::Timeout,
        crate::domain::AppError::ConnectionRefused => BastionError::Network,
        crate::domain::AppError::BastionUnavailable => BastionError::HelperProxyFailed,
        crate::domain::AppError::InvalidProfile => BastionError::SessionRejected,
        _ => BastionError::HelperProxyFailed,
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// Silence unused map_russh_error import risk if auth paths expand.
#[allow(dead_code)]
fn _map(error: russh::Error) -> BastionError {
    let _ = map_russh_error(error);
    BastionError::Network
}
