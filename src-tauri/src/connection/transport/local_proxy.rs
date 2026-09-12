//! Local TCP proxy transport (Boundary `boundary connect`).

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::domain::{AppError, AppResult};
use crate::helper::{
    DefaultExternalHelperManager, ExternalHelperManager, HelperError, LocalProxySpec, ProcessSpec,
};

use super::{CommandSpec, OpenedTransport, TransportCleanup, TransportContext};

/// How to discover the local listen address after spawning the helper.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LocalEndpointStrategy {
    /// Parse `127.0.0.1:PORT` (or similar) from helper stdout.
    Stdout,
    /// Parse from helper stderr.
    Stderr,
    /// Helper was started with an explicit bind address.
    Fixed { host: String, port: u16 },
}

pub async fn open(
    command: &CommandSpec,
    endpoint: &LocalEndpointStrategy,
    ctx: &TransportContext,
) -> AppResult<OpenedTransport> {
    if command.executable.trim().is_empty() {
        return Err(AppError::InvalidProfile);
    }
    match endpoint {
        LocalEndpointStrategy::Fixed { host, port } if host.trim().is_empty() || *port == 0 => {
            return Err(AppError::InvalidProfile);
        }
        _ => {}
    }
    let manager = DefaultExternalHelperManager::new();
    open_with_manager(&manager, command, endpoint, ctx).await
}

pub async fn open_with_manager(
    manager: &dyn ExternalHelperManager,
    command: &CommandSpec,
    endpoint: &LocalEndpointStrategy,
    ctx: &TransportContext,
) -> AppResult<OpenedTransport> {
    let process = ProcessSpec {
        executable: PathBuf::from(&command.executable),
        args: command.args.clone(),
        working_directory: command.working_directory.as_ref().map(PathBuf::from),
        env: command.env.clone(),
        stdin: false,
        stdout: true,
        stderr: true,
    };
    let spec = LocalProxySpec {
        process,
        endpoint: endpoint.clone(),
        ready_timeout: ctx.timeout().max(Duration::from_secs(15)),
    };
    let handle = manager
        .spawn_local_proxy(spec)
        .await
        .map_err(map_helper_error)?;
    let process = handle.process;
    let ready_payload = handle.ready_payload;
    // Dial the discovered loopback endpoint.
    let tcp = super::tcp::open(&handle.host, handle.port, ctx).await?;
    Ok(OpenedTransport {
        stream: tcp.stream,
        cleanup: Some(TransportCleanup::HelperProcess(process)),
        helper_ready_payload: ready_payload,
    })
}

fn map_helper_error(error: HelperError) -> AppError {
    match error {
        HelperError::Missing | HelperError::VersionMismatch => AppError::BastionUnavailable,
        HelperError::ReadyTimeout | HelperError::EndpointDiscoveryFailed => {
            AppError::ConnectionTimeout
        }
        _ => AppError::BastionUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fixed_endpoint_requires_valid_port() {
        let cmd = CommandSpec::new("boundary", vec!["connect".into()]);
        let endpoint = LocalEndpointStrategy::Fixed {
            host: "127.0.0.1".into(),
            port: 0,
        };
        let err = open(&cmd, &endpoint, &TransportContext::default())
            .await
            .expect_err("port 0");
        assert!(matches!(err, AppError::InvalidProfile));
    }
}
