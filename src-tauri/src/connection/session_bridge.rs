use std::collections::HashMap;
use std::sync::Arc;

use russh::{ChannelMsg, Disconnect};
use tauri::ipc::Channel;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::connection::bastion::{BastionConnection, BastionRegistry};
use crate::domain::{
    AppError, AppResult, RemoteImagePreview, RemoteTextPreview, SessionId, SessionState,
    SftpDirectory, SftpMetadata, TerminalEvent,
};
use crate::ssh::SftpChannel;

struct BastionTerminalSession {
    profile_id: Uuid,
    provider: String,
    connection: BastionConnection,
    output: Option<Channel<TerminalEvent>>,
    writer: Arc<russh::ChannelWriteHalf<russh::client::Msg>>,
    sftp: Arc<Mutex<Option<Arc<SftpChannel>>>>,
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
        let BastionConnection::SshInteractive { session } = &mut connection;
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

        self.sessions.write().await.insert(
            session_id,
            BastionTerminalSession {
                profile_id,
                provider,
                connection,
                output: Some(output.clone()),
                writer,
                sftp: Arc::new(Mutex::new(None)),
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
        session
            .writer
            .data(&data[..])
            .await
            .map_err(|_| AppError::ConnectionLost)
    }

    pub async fn resize(&self, session_id: SessionId, cols: u32, rows: u32) -> AppResult<()> {
        if cols == 0 || rows == 0 || cols > 1000 || rows > 1000 {
            return Err(AppError::InvalidProfile);
        }
        let sessions = self.sessions.read().await;
        let session = sessions.get(&session_id).ok_or(AppError::SessionNotFound)?;
        session
            .writer
            .window_change(cols, rows, 0, 0)
            .await
            .map_err(|_| AppError::ConnectionLost)
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
        {
            let mut sftp = session.sftp.lock().await;
            if let Some(channel) = sftp.take() {
                channel.close().await;
            }
        }
        let BastionConnection::SshInteractive { session: ssh } = &session.connection;
        let _ = ssh
            .client
            .disconnect(Disconnect::ByApplication, "user disconnect", "en")
            .await;
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

    pub async fn sftp_open(&self, session_id: SessionId) -> AppResult<SftpDirectory> {
        let sftp_state = {
            let sessions = self.sessions.read().await;
            Arc::clone(
                &sessions
                    .get(&session_id)
                    .ok_or(AppError::SessionNotFound)?
                    .sftp,
            )
        };
        let mut sftp = sftp_state.lock().await;
        if sftp.is_none() {
            let channel = {
                let sessions = self.sessions.read().await;
                let session = sessions.get(&session_id).ok_or(AppError::SessionNotFound)?;
                let BastionConnection::SshInteractive { session: ssh } = &session.connection;
                if ssh.client.is_closed() {
                    return Err(AppError::ConnectionLost);
                }
                ssh.client.channel_open_session().await.map_err(|error| {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %error,
                        "bastion SFTP channel_open_session failed"
                    );
                    AppError::SftpOperationFailed
                })?
            };
            channel
                .request_subsystem(true, "sftp")
                .await
                .map_err(|error| {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %error,
                        "bastion SFTP subsystem request failed"
                    );
                    AppError::SftpOperationFailed
                })?;
            let session = russh_sftp::client::SftpSession::new(channel.into_stream())
                .await
                .map_err(|error| {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %error,
                        "bastion SFTP protocol init failed"
                    );
                    AppError::SftpOperationFailed
                })?;
            *sftp = Some(Arc::new(SftpChannel::new(session).await?));
            tracing::info!(
                session_id = %session_id,
                "opened SFTP channel on bastion SSH session"
            );
        }
        sftp.as_ref().ok_or(AppError::SftpNotOpen)?.refresh().await
    }

    pub async fn sftp_list_directory(
        &self,
        session_id: SessionId,
        path: String,
    ) -> AppResult<SftpDirectory> {
        self.sftp_channel(session_id)
            .await?
            .list_directory(&path)
            .await
    }

    pub async fn sftp_stat(&self, session_id: SessionId, path: String) -> AppResult<SftpMetadata> {
        self.sftp_channel(session_id).await?.stat(&path).await
    }

    pub async fn sftp_change_directory(
        &self,
        session_id: SessionId,
        path: String,
    ) -> AppResult<SftpDirectory> {
        self.sftp_channel(session_id)
            .await?
            .change_directory(&path)
            .await
    }

    pub async fn sftp_refresh(&self, session_id: SessionId) -> AppResult<SftpDirectory> {
        self.sftp_channel(session_id).await?.refresh().await
    }

    pub async fn sftp_read_image_preview(
        &self,
        session_id: SessionId,
        path: String,
    ) -> AppResult<RemoteImagePreview> {
        self.sftp_open(session_id).await?;
        self.sftp_channel(session_id)
            .await?
            .read_image_preview(&path)
            .await
    }

    pub async fn sftp_read_text_preview(
        &self,
        session_id: SessionId,
        path: String,
    ) -> AppResult<RemoteTextPreview> {
        self.sftp_open(session_id).await?;
        self.sftp_channel(session_id)
            .await?
            .read_text_preview(&path)
            .await
    }

    pub async fn sftp_create_directory(
        &self,
        session_id: SessionId,
        parent: String,
        name: String,
    ) -> AppResult<SftpDirectory> {
        self.sftp_channel(session_id)
            .await?
            .create_directory(&parent, &name)
            .await
    }

    pub async fn sftp_rename(
        &self,
        session_id: SessionId,
        path: String,
        new_name: String,
    ) -> AppResult<SftpDirectory> {
        self.sftp_channel(session_id)
            .await?
            .rename(&path, &new_name)
            .await
    }

    pub async fn sftp_delete(
        &self,
        session_id: SessionId,
        path: String,
        recursive: bool,
    ) -> AppResult<SftpDirectory> {
        self.sftp_channel(session_id)
            .await?
            .delete(&path, recursive)
            .await
    }

    pub async fn sftp_channel(&self, session_id: SessionId) -> AppResult<Arc<SftpChannel>> {
        let state = {
            let sessions = self.sessions.read().await;
            Arc::clone(
                &sessions
                    .get(&session_id)
                    .ok_or(AppError::SessionNotFound)?
                    .sftp,
            )
        };
        let sftp = state.lock().await;
        sftp.as_ref().cloned().ok_or(AppError::SftpNotOpen)
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
        {
            let mut sftp = session.sftp.lock().await;
            if let Some(channel) = sftp.take() {
                channel.close().await;
            }
        }
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
