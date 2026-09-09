use super::service::HostKeyHandler;
use crate::domain::{AppError, AppResult};
use russh::{client, Channel, ChannelOpenFailure};
use std::sync::{Arc, Weak};

/// A non-owning, crate-internal capability for an already verified SSH transport.
/// It cannot reconnect, read credentials, execute commands, or prolong a session.
#[derive(Clone)]
pub(crate) struct ForwardTransport {
    client: Weak<client::Handle<HostKeyHandler>>,
}
impl ForwardTransport {
    pub(super) fn new(client: &Arc<client::Handle<HostKeyHandler>>) -> Self {
        Self {
            client: Arc::downgrade(client),
        }
    }
    pub fn is_closed(&self) -> bool {
        self.client
            .upgrade()
            .is_none_or(|client| client.is_closed())
    }
    pub async fn open(
        &self,
        host: &str,
        port: u16,
        origin_port: u16,
    ) -> AppResult<Channel<client::Msg>> {
        let client = self.client.upgrade().ok_or(AppError::ConnectionLost)?;
        if client.is_closed() {
            return Err(AppError::ConnectionLost);
        }
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            client.channel_open_direct_tcpip(
                host,
                u32::from(port),
                "127.0.0.1",
                u32::from(origin_port),
            ),
        )
        .await
        .map_err(|_| AppError::ConnectionTimeout)?
        .map_err(|error| match error {
            russh::Error::ChannelOpenFailure(ChannelOpenFailure::AdministrativelyProhibited) => {
                AppError::TunnelDenied
            }
            // SSH does not reliably distinguish DNS errors from refused connections.
            russh::Error::ChannelOpenFailure(ChannelOpenFailure::ConnectFailed) => {
                AppError::TunnelTargetFailed
            }
            _ if client.is_closed() => AppError::ConnectionLost,
            _ => AppError::TunnelTargetFailed,
        })
    }
}
