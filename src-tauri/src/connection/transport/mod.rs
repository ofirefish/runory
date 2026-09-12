//! Transport plans and factory that open AsyncRead + AsyncWrite streams for SSH Core.
//!
//! SSH Core must not know whether the bytes come from TCP, SSH direct-tcpip,
//! a Teleport stdio proxy, or a Boundary local TCP proxy.

mod local_proxy;
pub mod ssh_jump;
mod stdio_proxy;
mod tcp;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::domain::{AppError, AppResult};

pub use local_proxy::LocalEndpointStrategy;
#[allow(unused_imports)]
pub use ssh_jump::JumpHopCredentialRef;

/// Opaque credential reference for a jump hop (resolved by the caller before open).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialRef {
    pub id: String,
}

/// One hop in an OpenSSH ProxyJump chain.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JumpHop {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub credential_ref: CredentialRef,
    /// Known-host scope for this hop (independent of the final target).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_key_scope: Option<String>,
}

/// External process used as a transport helper (tsh, boundary, …).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSpec {
    pub executable: String,
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<(String, String)>,
}

impl CommandSpec {
    pub fn new(executable: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            executable: executable.into(),
            args,
            working_directory: None,
            env: Vec::new(),
        }
    }
}

/// How bytes reach the SSH peer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TransportPlan {
    Tcp {
        host: String,
        port: u16,
    },
    SshJump {
        hops: Vec<JumpHop>,
        target_host: String,
        target_port: u16,
    },
    StdioProxy {
        command: CommandSpec,
    },
    LocalTcpProxy {
        command: CommandSpec,
        endpoint: LocalEndpointStrategy,
    },
}

/// Bidirectional byte stream consumed by russh `connect_stream`.
pub trait AsyncTransport: AsyncRead + AsyncWrite + Send + Unpin {}

impl<T> AsyncTransport for T where T: AsyncRead + AsyncWrite + Send + Unpin {}

/// Owned transport plus optional lifecycle cleanup (helper process, jump clients).
pub struct OpenedTransport {
    pub stream: Box<dyn AsyncTransport>,
    pub cleanup: Option<TransportCleanup>,
    /// Helper ready payload (e.g. Boundary `connect -format=json`). Never log.
    pub helper_ready_payload: Option<zeroize::Zeroizing<String>>,
}

impl std::fmt::Debug for OpenedTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenedTransport")
            .field("cleanup", &self.cleanup.is_some())
            .finish()
    }
}

/// Resources that must be released when the SSH session ends.
pub enum TransportCleanup {
    /// External helper process managed by `helper::ExternalHelperManager`.
    HelperProcess(crate::helper::HelperProcess),
    /// Nested SSH jump clients (kept alive for the duration of the target session).
    JumpChain(Box<dyn JumpChainGuard>),
}

/// Keeps jump-host SSH clients alive until dropped / explicitly closed.
pub trait JumpChainGuard: Send + Sync {
    fn is_alive(&self) -> bool;
}

/// Context passed into TransportFactory::open (timeouts, cancel, helper manager).
#[derive(Clone, Debug, Default)]
pub struct TransportContext {
    pub connect_timeout_secs: Option<u64>,
}

impl TransportContext {
    pub fn with_timeout(secs: u64) -> Self {
        Self {
            connect_timeout_secs: Some(secs),
        }
    }

    pub fn timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.connect_timeout_secs.unwrap_or(30))
    }
}

/// Opens a TransportPlan into an AsyncRead + AsyncWrite stream.
pub struct TransportFactory;

impl TransportFactory {
    pub async fn open(plan: &TransportPlan, ctx: &TransportContext) -> AppResult<OpenedTransport> {
        match plan {
            TransportPlan::Tcp { host, port } => tcp::open(host, *port, ctx).await,
            TransportPlan::SshJump {
                hops,
                target_host,
                target_port,
            } => {
                // Secret-free plan cannot open alone; SessionManager must call
                // `ssh_jump::open_resolved` with hop credentials. Keep total match.
                let _ = (hops, target_host, target_port);
                ssh_jump::open(hops, target_host, *target_port, ctx).await
            }
            TransportPlan::StdioProxy { command } => stdio_proxy::open(command, ctx).await,
            TransportPlan::LocalTcpProxy { command, endpoint } => {
                local_proxy::open(command, endpoint, ctx).await
            }
        }
    }
}

/// Errors that originate from transport opening (mapped to AppError).
#[async_trait]
pub trait TransportOpener: Send + Sync {
    async fn open(&self, plan: &TransportPlan, ctx: &TransportContext) -> AppResult<OpenedTransport>;
}

pub(crate) fn map_io_connect_error(error: std::io::Error) -> AppError {
    match error.kind() {
        std::io::ErrorKind::TimedOut => AppError::ConnectionTimeout,
        std::io::ErrorKind::ConnectionRefused
        | std::io::ErrorKind::NotFound
        | std::io::ErrorKind::AddrNotAvailable => AppError::ConnectionRefused,
        _ => AppError::ConnectionRefused,
    }
}
