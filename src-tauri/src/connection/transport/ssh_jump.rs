//! SSH ProxyJump transport: chain of direct-tcpip hops ending at the target.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client;
use russh::keys::PrivateKeyWithHashAlg;
use russh::Disconnect;
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

use crate::domain::{AppError, AppResult, HostKeyInfo};
use crate::ssh::{map_russh_error, HostKeyHandler};

use super::{JumpChainGuard, JumpHop, OpenedTransport, TransportCleanup, TransportContext};

/// Credential material for one hop, resolved outside the transport plan.
#[derive(Clone, Debug)]
pub struct JumpHopCredentialRef {
    pub hop_index: usize,
    pub expected_fingerprint: String,
}

/// Explicit open request used by SessionManager once hop secrets are resolved.
/// Kept separate from [`super::TransportPlan::SshJump`] so plans remain secret-free.
#[derive(Clone, Debug)]
pub struct SshJumpOpenRequest {
    pub hops: Vec<ResolvedJumpHop>,
    pub target_host: String,
    pub target_port: u16,
}

#[derive(Clone, Debug)]
pub struct ResolvedJumpHop {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub expected_fingerprint: String,
    pub auth: ResolvedJumpAuth,
    #[allow(dead_code)]
    pub host_key_scope: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ResolvedJumpAuth {
    Password(zeroize::Zeroizing<String>),
    PrivateKey {
        key_material: zeroize::Zeroizing<Vec<u8>>,
        passphrase: Option<zeroize::Zeroizing<String>>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JumpPlanSummary {
    pub hop_count: usize,
    pub target_host: String,
    pub target_port: u16,
}

struct OwnedJumpChain {
    clients: Vec<client::Handle<HostKeyHandler>>,
}

impl JumpChainGuard for OwnedJumpChain {
    fn is_alive(&self) -> bool {
        self.clients.iter().all(|c| !c.is_closed())
    }
}

/// Plan-only open: credentials are not embedded in TransportPlan.
pub async fn open(
    hops: &[JumpHop],
    _target_host: &str,
    _target_port: u16,
    _ctx: &TransportContext,
) -> AppResult<OpenedTransport> {
    if hops.is_empty() {
        return Err(AppError::InvalidJumpHost);
    }
    Err(AppError::BastionUnavailable)
}

/// Open a resolved multi-hop ProxyJump chain and return the target byte stream.
pub async fn open_resolved(
    request: &SshJumpOpenRequest,
    ctx: &TransportContext,
) -> AppResult<OpenedTransport> {
    if request.hops.is_empty() {
        return Err(AppError::InvalidJumpHost);
    }
    validate_endpoint(&request.target_host, request.target_port)?;

    let mut clients: Vec<client::Handle<HostKeyHandler>> = Vec::new();
    let first = &request.hops[0];
    validate_endpoint(&first.host, first.port)?;

    clients.push(authenticate_tcp_hop(first, ctx).await?);

    for (index, hop) in request.hops.iter().enumerate().skip(1) {
        validate_endpoint(&hop.host, hop.port)?;
        let parent = clients.last().ok_or(AppError::InvalidJumpHost)?;
        let channel = timeout(
            ctx.timeout(),
            parent.channel_open_direct_tcpip(
                hop.host.as_str(),
                u32::from(hop.port),
                "127.0.0.1",
                0,
            ),
        )
        .await
        .map_err(|_| AppError::ConnectionTimeout)?
        .map_err(map_jump_error)?;
        let client = authenticate_stream_hop(channel.into_stream(), hop).await?;
        clients.push(client);
        tracing::info!(
            hop_index = index,
            host = %hop.host,
            port = hop.port,
            "proxyjump hop authenticated"
        );
    }

    let parent = clients.last().ok_or(AppError::InvalidJumpHost)?;
    let target_channel = timeout(
        ctx.timeout(),
        parent.channel_open_direct_tcpip(
            request.target_host.as_str(),
            u32::from(request.target_port),
            "127.0.0.1",
            0,
        ),
    )
    .await
    .map_err(|_| AppError::ConnectionTimeout)?
    .map_err(map_jump_error)?;

    Ok(OpenedTransport {
        stream: Box::new(target_channel.into_stream()),
        cleanup: Some(TransportCleanup::JumpChain(Box::new(OwnedJumpChain {
            clients,
        }))),
        helper_ready_payload: None,
    })
}

async fn authenticate_tcp_hop(
    hop: &ResolvedJumpHop,
    ctx: &TransportContext,
) -> AppResult<client::Handle<HostKeyHandler>> {
    let observed = Arc::new(Mutex::new(None));
    let handler = HostKeyHandler::new(Some(hop.expected_fingerprint.clone()), Arc::clone(&observed));
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(30)),
        keepalive_interval: Some(Duration::from_secs(15)),
        ..Default::default()
    });
    let mut client = timeout(
        ctx.timeout(),
        client::connect(config, (hop.host.as_str(), hop.port), handler),
    )
    .await
    .map_err(|_| AppError::ConnectionTimeout)?
    .map_err(map_russh_error)?;
    verify_observed(&observed, &hop.expected_fingerprint)?;
    authenticate_client(&mut client, hop).await?;
    Ok(client)
}

async fn authenticate_stream_hop(
    stream: russh::ChannelStream<client::Msg>,
    hop: &ResolvedJumpHop,
) -> AppResult<client::Handle<HostKeyHandler>> {
    let observed = Arc::new(Mutex::new(None));
    let handler = HostKeyHandler::new(Some(hop.expected_fingerprint.clone()), Arc::clone(&observed));
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(30)),
        keepalive_interval: Some(Duration::from_secs(15)),
        ..Default::default()
    });
    let mut client = timeout(
        Duration::from_secs(15),
        client::connect_stream(config, stream, handler),
    )
    .await
    .map_err(|_| AppError::ConnectionTimeout)?
    .map_err(map_russh_error)?;
    verify_observed(&observed, &hop.expected_fingerprint)?;
    authenticate_client(&mut client, hop).await?;
    Ok(client)
}

async fn authenticate_client(
    client: &mut client::Handle<HostKeyHandler>,
    hop: &ResolvedJumpHop,
) -> AppResult<()> {
    let ok = match &hop.auth {
        ResolvedJumpAuth::Password(password) => client
            .authenticate_password(&hop.username, password.as_str())
            .await
            .map_err(map_russh_error)?
            .success(),
        ResolvedJumpAuth::PrivateKey {
            key_material,
            passphrase,
        } => {
            let key = load_private_key(key_material, passphrase.as_ref())?;
            let hash = client
                .best_supported_rsa_hash()
                .await
                .map_err(map_russh_error)?
                .flatten();
            client
                .authenticate_publickey(
                    &hop.username,
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                )
                .await
                .map_err(map_russh_error)?
                .success()
        }
    };
    if !ok {
        let _ = client
            .disconnect(Disconnect::ByApplication, "authentication failed", "en")
            .await;
        return Err(AppError::AuthFailed);
    }
    Ok(())
}

fn load_private_key(
    material: &[u8],
    passphrase: Option<&zeroize::Zeroizing<String>>,
) -> AppResult<russh::keys::PrivateKey> {
    let encoded = std::str::from_utf8(material).map_err(|_| AppError::PrivateKeyInvalid)?;
    let passphrase_ref = passphrase.and_then(|value| {
        if value.is_empty() {
            None
        } else {
            Some(value.as_str())
        }
    });
    russh::keys::decode_secret_key(encoded, passphrase_ref).map_err(|_| {
        if passphrase_ref.is_none() {
            AppError::PassphraseRequired
        } else {
            AppError::PrivateKeyInvalid
        }
    })
}

fn verify_observed(
    observed: &Arc<Mutex<Option<HostKeyInfo>>>,
    expected: &str,
) -> AppResult<()> {
    let actual = observed
        .lock()
        .ok()
        .and_then(|value| value.as_ref().map(|info| info.fingerprint.clone()))
        .ok_or(AppError::HostKeyUnknown)?;
    if actual == expected {
        Ok(())
    } else {
        Err(AppError::HostKeyChanged)
    }
}

fn validate_endpoint(host: &str, port: u16) -> AppResult<()> {
    if host.trim().is_empty() || host.chars().any(char::is_whitespace) || port == 0 {
        Err(AppError::InvalidProfile)
    } else {
        Ok(())
    }
}

fn map_jump_error(error: russh::Error) -> AppError {
    match error {
        russh::Error::ChannelOpenFailure(russh::ChannelOpenFailure::AdministrativelyProhibited) => {
            AppError::JumpForwardingDenied
        }
        russh::Error::ChannelOpenFailure(russh::ChannelOpenFailure::ConnectFailed) => {
            AppError::JumpTargetUnreachable
        }
        _ => AppError::JumpTargetUnreachable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::transport::{CredentialRef, JumpHop, TransportContext};

    #[tokio::test]
    async fn empty_hops_rejected() {
        let err = open(&[], "10.0.0.1", 22, &TransportContext::default())
            .await
            .expect_err("empty hops");
        assert!(matches!(err, AppError::InvalidJumpHost));
    }

    #[tokio::test]
    async fn plan_open_without_secrets_is_unavailable() {
        let hops = vec![JumpHop {
            host: "jump.example".into(),
            port: 22,
            username: "jump".into(),
            credential_ref: CredentialRef {
                id: "cred-1".into(),
            },
            host_key_scope: Some("jump:jump.example".into()),
        }];
        let err = open(&hops, "10.0.0.1", 22, &TransportContext::default())
            .await
            .expect_err("needs resolved secrets");
        assert!(matches!(err, AppError::BastionUnavailable));
    }

    #[tokio::test]
    async fn open_resolved_rejects_empty_hops() {
        let err = open_resolved(
            &SshJumpOpenRequest {
                hops: Vec::new(),
                target_host: "10.0.0.1".into(),
                target_port: 22,
            },
            &TransportContext::default(),
        )
        .await
        .expect_err("empty");
        assert!(matches!(err, AppError::InvalidJumpHost));
    }
}
