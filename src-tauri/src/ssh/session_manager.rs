use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use russh::client;
use russh::{ChannelMsg, ChannelWriteHalf, Disconnect};
use tauri::ipc::Channel;
use tokio::sync::{Mutex, RwLock};

use super::os_detection::{parse_os_release, parse_uname};
use crate::domain::{
    AppError, AppResult, ConnectRequest, OsDistribution, SessionId, SessionState, SftpDirectory,
    SftpMetadata, TerminalEvent,
};
use crate::ssh::{ExecChannel, RemoteCommand, RemoteExecResult, SftpChannel, SshService};
use crate::transfers::TransferQueue;

const TERMINAL_CONTEXT_LIMIT: usize = 32 * 1024;

#[derive(Default)]
struct TerminalContextBuffer {
    bytes: Vec<u8>,
}

impl TerminalContextBuffer {
    fn append(&mut self, data: &[u8]) {
        if data.len() >= TERMINAL_CONTEXT_LIMIT {
            self.bytes.clear();
            self.bytes
                .extend_from_slice(&data[data.len() - TERMINAL_CONTEXT_LIMIT..]);
            return;
        }
        let overflow = self
            .bytes
            .len()
            .saturating_add(data.len())
            .saturating_sub(TERMINAL_CONTEXT_LIMIT);
        if overflow > 0 {
            self.bytes.drain(..overflow);
        }
        self.bytes.extend_from_slice(data);
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

struct SessionHandle {
    profile_id: uuid::Uuid,
    client: Arc<client::Handle<super::service::HostKeyHandler>>,
    writer: Arc<ChannelWriteHalf<client::Msg>>,
    sftp: Arc<Mutex<Option<Arc<SftpChannel>>>>,
    terminal_context: Arc<Mutex<TerminalContextBuffer>>,
    output: Option<Channel<TerminalEvent>>,
}

pub struct ServerSessionManager {
    sessions: Arc<RwLock<HashMap<SessionId, SessionHandle>>>,
    transfers: TransferQueue,
}

impl Default for ServerSessionManager {
    fn default() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            transfers: TransferQueue::default(),
        }
    }
}

impl ServerSessionManager {
    pub async fn connect(
        &self,
        profile_id: uuid::Uuid,
        request: ConnectRequest,
        expected_fingerprint: String,
        output: Channel<TerminalEvent>,
    ) -> AppResult<SessionId> {
        let _ = output.send(TerminalEvent::State {
            state: SessionState::Connecting,
        });
        let _ = output.send(TerminalEvent::State {
            state: SessionState::VerifyingHost,
        });
        let _ = output.send(TerminalEvent::State {
            state: SessionState::Authenticating,
        });
        let opened = match SshService::connect(request, expected_fingerprint).await {
            Ok(opened) => opened,
            Err(error) => {
                let _ = output.send(TerminalEvent::State {
                    state: SessionState::Error,
                });
                return Err(error);
            }
        };
        let _ = output.send(TerminalEvent::State {
            state: SessionState::OpeningShell,
        });
        let session_id = uuid::Uuid::new_v4();
        let writer = Arc::new(opened.writer);
        let terminal_context = Arc::new(Mutex::new(TerminalContextBuffer::default()));
        self.sessions.write().await.insert(
            session_id,
            SessionHandle {
                profile_id,
                client: Arc::new(opened.client),
                writer: Arc::clone(&writer),
                sftp: Arc::new(Mutex::new(None)),
                terminal_context: Arc::clone(&terminal_context),
                output: Some(output.clone()),
            },
        );
        let _ = output.send(TerminalEvent::State {
            state: SessionState::Connected,
        });
        tokio::spawn(stream_output(
            opened.reader,
            output,
            Arc::clone(&self.sessions),
            self.transfers.clone(),
            terminal_context,
            session_id,
        ));
        Ok(session_id)
    }

    pub async fn write(&self, session_id: SessionId, data: Vec<u8>) -> AppResult<()> {
        if data.len() > 64 * 1024 {
            return Err(AppError::InvalidProfile);
        }
        let writer = {
            let sessions = self.sessions.read().await;
            Arc::clone(
                &sessions
                    .get(&session_id)
                    .ok_or(AppError::SessionNotFound)?
                    .writer,
            )
        };
        writer
            .data(&data[..])
            .await
            .map_err(|_| AppError::ConnectionLost)
    }

    pub async fn resize(&self, session_id: SessionId, cols: u32, rows: u32) -> AppResult<()> {
        if cols == 0 || rows == 0 || cols > 1000 || rows > 1000 {
            return Err(AppError::InvalidProfile);
        }
        let writer = {
            let sessions = self.sessions.read().await;
            Arc::clone(
                &sessions
                    .get(&session_id)
                    .ok_or(AppError::SessionNotFound)?
                    .writer,
            )
        };
        writer
            .window_change(cols, rows, 0, 0)
            .await
            .map_err(|_| AppError::ConnectionLost)
    }

    pub async fn disconnect(&self, session_id: SessionId) -> AppResult<()> {
        self.transfers.cancel_session(session_id).await;
        let session = self
            .sessions
            .write()
            .await
            .remove(&session_id)
            .ok_or(AppError::SessionNotFound)?;
        tracing::info!(session_id = %session_id, profile_id = %session.profile_id, "disconnecting SSH session");
        if let Some(sftp) = session.sftp.lock().await.take() {
            sftp.close().await;
        }
        let _ = session.writer.close().await;
        session
            .client
            .disconnect(Disconnect::ByApplication, "user disconnect", "en")
            .await
            .map_err(|_| AppError::ConnectionLost)?;
        if let Some(output) = session.output.as_ref() {
            let _ = output.send(TerminalEvent::State {
                state: SessionState::Disconnected,
            });
        }
        Ok(())
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
                if session.client.is_closed() {
                    return Err(AppError::ConnectionLost);
                }
                session
                    .client
                    .channel_open_session()
                    .await
                    .map_err(|_| AppError::SftpOperationFailed)?
            };
            channel
                .request_subsystem(true, "sftp")
                .await
                .map_err(|_| AppError::SftpOperationFailed)?;
            let session = russh_sftp::client::SftpSession::new(channel.into_stream())
                .await
                .map_err(|_| AppError::SftpOperationFailed)?;
            *sftp = Some(Arc::new(SftpChannel::new(session).await?));
            tracing::info!(session_id = %session_id, "opened SFTP channel on existing SSH session");
        }
        sftp.as_ref().ok_or(AppError::SftpNotOpen)?.refresh().await
    }

    pub async fn sftp_list_directory(
        &self,
        session_id: SessionId,
        path: String,
    ) -> AppResult<SftpDirectory> {
        let state = self.sftp_state(session_id).await?;
        let sftp = state.lock().await;
        sftp.as_ref()
            .ok_or(AppError::SftpNotOpen)?
            .list_directory(&path)
            .await
    }

    pub async fn sftp_stat(&self, session_id: SessionId, path: String) -> AppResult<SftpMetadata> {
        let state = self.sftp_state(session_id).await?;
        let sftp = state.lock().await;
        sftp.as_ref()
            .ok_or(AppError::SftpNotOpen)?
            .stat(&path)
            .await
    }

    pub async fn sftp_change_directory(
        &self,
        session_id: SessionId,
        path: String,
    ) -> AppResult<SftpDirectory> {
        let state = self.sftp_state(session_id).await?;
        let sftp = state.lock().await;
        sftp.as_ref()
            .ok_or(AppError::SftpNotOpen)?
            .change_directory(&path)
            .await
    }

    pub async fn sftp_refresh(&self, session_id: SessionId) -> AppResult<SftpDirectory> {
        let state = self.sftp_state(session_id).await?;
        let sftp = state.lock().await;
        sftp.as_ref().ok_or(AppError::SftpNotOpen)?.refresh().await
    }

    pub async fn sftp_read_text(
        &self,
        session_id: SessionId,
        path: String,
    ) -> AppResult<(String, String)> {
        self.sftp_open(session_id).await?;
        self.sftp_channel(session_id).await?.read_text(&path).await
    }

    pub async fn sftp_read_image_preview(
        &self,
        session_id: SessionId,
        path: String,
    ) -> AppResult<crate::domain::RemoteImagePreview> {
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
    ) -> AppResult<crate::domain::RemoteTextPreview> {
        self.sftp_open(session_id).await?;
        self.sftp_channel(session_id)
            .await?
            .read_text_preview(&path)
            .await
    }

    pub async fn sftp_write_text(
        &self,
        session_id: SessionId,
        path: String,
        content: String,
    ) -> AppResult<(String, u64)> {
        self.sftp_open(session_id).await?;
        self.sftp_channel(session_id)
            .await?
            .write_text(&path, &content)
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

    pub(crate) async fn sftp_channel(&self, session_id: SessionId) -> AppResult<Arc<SftpChannel>> {
        let state = self.sftp_state(session_id).await?;
        let sftp = state.lock().await;
        sftp.as_ref().cloned().ok_or(AppError::SftpNotOpen)
    }

    pub(crate) fn transfer_queue(&self) -> TransferQueue {
        self.transfers.clone()
    }

    pub(crate) async fn exec(
        &self,
        session_id: SessionId,
        command: RemoteCommand,
    ) -> AppResult<RemoteExecResult> {
        let client = {
            let sessions = self.sessions.read().await;
            let session = sessions.get(&session_id).ok_or(AppError::SessionNotFound)?;
            if session.client.is_closed() {
                return Err(AppError::ConnectionLost);
            }
            Arc::clone(&session.client)
        };
        ExecChannel::run(client, command).await
    }

    pub(crate) async fn profile_id(&self, session_id: SessionId) -> AppResult<uuid::Uuid> {
        self.sessions
            .read()
            .await
            .get(&session_id)
            .map(|session| session.profile_id)
            .ok_or(AppError::SessionNotFound)
    }

    pub(crate) async fn detect_os_distribution(
        &self,
        session_id: SessionId,
    ) -> AppResult<Option<OsDistribution>> {
        let os_release = self
            .exec(
                session_id,
                RemoteCommand::program("cat", vec!["/etc/os-release".into()])
                    .with_timeout(Duration::from_secs(4))
                    .with_output_limit(16 * 1024),
            )
            .await;
        if let Ok(result) = os_release {
            if result.exit_code == 0 {
                return Ok(parse_os_release(&result.stdout));
            }
        }

        let uname = self
            .exec(
                session_id,
                RemoteCommand::program("uname", vec!["-s".into()])
                    .with_timeout(Duration::from_secs(4))
                    .with_output_limit(1024),
            )
            .await?;
        Ok((uname.exit_code == 0)
            .then(|| parse_uname(&uname.stdout))
            .flatten())
    }

    pub(crate) async fn recent_terminal_output(&self, session_id: SessionId) -> AppResult<String> {
        let context = {
            let sessions = self.sessions.read().await;
            Arc::clone(
                &sessions
                    .get(&session_id)
                    .ok_or(AppError::SessionNotFound)?
                    .terminal_context,
            )
        };
        let text = context.lock().await.text();
        Ok(text)
    }

    async fn sftp_state(
        &self,
        session_id: SessionId,
    ) -> AppResult<Arc<Mutex<Option<Arc<SftpChannel>>>>> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(&session_id).ok_or(AppError::SessionNotFound)?;
        if session.client.is_closed() {
            return Err(AppError::ConnectionLost);
        }
        Ok(Arc::clone(&session.sftp))
    }
}

async fn stream_output(
    mut reader: russh::ChannelReadHalf,
    output: Channel<TerminalEvent>,
    sessions: Arc<RwLock<HashMap<SessionId, SessionHandle>>>,
    transfers: TransferQueue,
    terminal_context: Arc<Mutex<TerminalContextBuffer>>,
    session_id: SessionId,
) {
    while let Some(message) = reader.wait().await {
        match message {
            ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                terminal_context.lock().await.append(&data);
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
    // A shell channel can reach EOF while the authenticated transport and its SFTP channel remain usable.
    let transport_closed = sessions
        .read()
        .await
        .get(&session_id)
        .is_none_or(|session| session.client.is_closed());
    let reason = if transport_closed {
        transfers.cancel_session(session_id).await;
        sessions.write().await.remove(&session_id);
        "transport-closed"
    } else {
        "terminal-closed"
    };
    let _ = output.send(TerminalEvent::Closed {
        reason: Some(reason.to_owned()),
    });
}

#[cfg(test)]
mod terminal_context_tests {
    use super::*;

    #[test]
    fn terminal_context_keeps_only_the_bounded_tail() {
        let mut context = TerminalContextBuffer::default();
        context.append(&vec![b'a'; TERMINAL_CONTEXT_LIMIT]);
        context.append(b"tail");
        assert_eq!(context.bytes.len(), TERMINAL_CONTEXT_LIMIT);
        assert!(context.text().ends_with("tail"));

        context.append(&vec![b'b'; TERMINAL_CONTEXT_LIMIT + 10]);
        assert_eq!(context.bytes, vec![b'b'; TERMINAL_CONTEXT_LIMIT]);
    }
}
