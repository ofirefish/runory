use std::collections::HashMap;
use std::sync::Arc;

use russh::{ChannelMsg, Disconnect};
use tauri::ipc::Channel;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::connection::bastion::{BastionConnection, BastionRegistry};
use crate::domain::{AppError, AppResult, SessionId, SessionState, TerminalEvent};

enum SessionIo {
    MockEcho,
    Ssh {
        writer: Arc<russh::ChannelWriteHalf<russh::client::Msg>>,
    },
}

struct BastionTerminalSession {
    profile_id: Uuid,
    provider: String,
    connection: BastionConnection,
    output: Option<Channel<TerminalEvent>>,
    io: SessionIo,
}

/// Holds bastion sessions that expose the same Terminal Channel contract as SSH
/// without rewriting ServerSessionManager.
#[derive(Default)]
pub struct BastionSessionBridge {
    sessions: Arc<RwLock<HashMap<SessionId, BastionTerminalSession>>>,
}

impl BastionSessionBridge {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn has(&self, session_id: SessionId) -> bool {
        self.sessions.read().await.contains_key(&session_id)
    }

    pub async fn open(
        &self,
        profile_id: Uuid,
        provider: String,
        mut connection: BastionConnection,
        output: Channel<TerminalEvent>,
    ) -> AppResult<SessionId> {
        let _ = output.send(TerminalEvent::State {
            state: SessionState::Connecting,
        });
        let _ = output.send(TerminalEvent::State {
            state: SessionState::Authenticating,
        });
        let _ = output.send(TerminalEvent::State {
            state: SessionState::OpeningShell,
        });

        let session_id = Uuid::new_v4();
        let metadata = connection.metadata().clone();
        let io = match &mut connection {
            BastionConnection::Native { .. } => {
                let banner = format!(
                    "\r\nRunory Bastion Session ({provider})\r\nAsset: {}\r\nAccount: {}\r\nRecording: {}\r\nCommand Audit: {}\r\n\r\n[mock] Interactive shell is simulated. Input is not forwarded to a real host.\r\n\r\n",
                    metadata.asset_id,
                    metadata.account,
                    if metadata.recording { "ON" } else { "OFF" },
                    if metadata.command_audit { "ON" } else { "OFF" },
                );
                let _ = output.send(TerminalEvent::Output {
                    bytes: banner.into_bytes(),
                });
                SessionIo::MockEcho
            }
            BastionConnection::SshInteractive { session } => {
                let reader = session.reader.take().ok_or(AppError::BastionUnavailable)?;
                let writer = Arc::clone(&session.writer);
                let banner = format!(
                    "\r\nRunory Bastion Session ({})\r\nAsset: {}\r\nAccount: {}\r\nRecording: {}\r\n\r\n",
                    metadata.provider,
                    metadata.asset_id,
                    metadata.account,
                    if metadata.recording { "ON" } else { "OFF" },
                );
                let _ = output.send(TerminalEvent::Output {
                    bytes: banner.into_bytes(),
                });
                tokio::spawn(stream_bastion_output(
                    reader,
                    output.clone(),
                    Arc::clone(&self.sessions),
                    session_id,
                ));
                SessionIo::Ssh { writer }
            }
        };

        self.sessions.write().await.insert(
            session_id,
            BastionTerminalSession {
                profile_id,
                provider,
                connection,
                output: Some(output.clone()),
                io,
            },
        );
        let _ = output.send(TerminalEvent::State {
            state: SessionState::Connected,
        });
        tracing::info!(
            session_id = %session_id,
            profile_id = %profile_id,
            "opened bastion terminal session"
        );
        Ok(session_id)
    }

    pub async fn write(&self, session_id: SessionId, data: Vec<u8>) -> AppResult<()> {
        if data.len() > 64 * 1024 {
            return Err(AppError::InvalidProfile);
        }
        let sessions = self.sessions.read().await;
        let session = sessions.get(&session_id).ok_or(AppError::SessionNotFound)?;
        match &session.io {
            SessionIo::MockEcho => {
                if let Some(output) = session.output.as_ref() {
                    let _ = output.send(TerminalEvent::Output { bytes: data });
                }
                Ok(())
            }
            SessionIo::Ssh { writer } => writer
                .data(&data[..])
                .await
                .map_err(|_| AppError::ConnectionLost),
        }
    }

    pub async fn resize(&self, session_id: SessionId, cols: u32, rows: u32) -> AppResult<()> {
        if cols == 0 || rows == 0 || cols > 1000 || rows > 1000 {
            return Err(AppError::InvalidProfile);
        }
        let sessions = self.sessions.read().await;
        let session = sessions.get(&session_id).ok_or(AppError::SessionNotFound)?;
        match &session.io {
            SessionIo::MockEcho => Ok(()),
            SessionIo::Ssh { writer } => writer
                .window_change(cols, rows, 0, 0)
                .await
                .map_err(|_| AppError::ConnectionLost),
        }
    }

    pub async fn disconnect(
        &self,
        session_id: SessionId,
        registry: &BastionRegistry,
    ) -> AppResult<()> {
        let session = self
            .sessions
            .write()
            .await
            .remove(&session_id)
            .ok_or(AppError::SessionNotFound)?;
        tracing::info!(
            session_id = %session_id,
            profile_id = %session.profile_id,
            provider = %session.provider,
            "disconnecting bastion session"
        );
        if let BastionConnection::SshInteractive { session: ssh } = &session.connection {
            let _ = ssh
                .client
                .disconnect(Disconnect::ByApplication, "user disconnect", "en")
                .await;
        }
        if let Some(provider) = registry.get(&session.provider) {
            let _ = provider.disconnect(&session.connection).await;
        }
        if let Some(output) = session.output.as_ref() {
            let _ = output.send(TerminalEvent::State {
                state: SessionState::Disconnected,
            });
        }
        Ok(())
    }

    pub async fn profile_id(&self, session_id: SessionId) -> AppResult<Uuid> {
        self.sessions
            .read()
            .await
            .get(&session_id)
            .map(|session| session.profile_id)
            .ok_or(AppError::SessionNotFound)
    }
}

async fn stream_bastion_output(
    mut reader: russh::ChannelReadHalf,
    output: Channel<TerminalEvent>,
    sessions: Arc<RwLock<HashMap<SessionId, BastionTerminalSession>>>,
    session_id: SessionId,
) {
    while let Some(message) = reader.wait().await {
        match message {
            ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                if output
                    .send(TerminalEvent::Output {
                        bytes: data.to_vec(),
                    })
                    .is_err()
                {
                    break;
                }
            }
            ChannelMsg::Close | ChannelMsg::Eof => break,
            _ => {}
        }
    }
    if let Some(session) = sessions.write().await.remove(&session_id) {
        if let Some(channel) = session.output.as_ref() {
            let _ = channel.send(TerminalEvent::Closed {
                reason: Some("remote-closed".into()),
            });
            let _ = channel.send(TerminalEvent::State {
                state: SessionState::Disconnected,
            });
        }
    }
}
