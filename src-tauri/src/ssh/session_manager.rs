use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use russh::client;
use russh::{ChannelMsg, ChannelWriteHalf, Disconnect};
use tauri::ipc::Channel;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{sleep, Instant};

use super::os_detection::{parse_os_release, parse_uname};
use crate::domain::{
    AppError, AppResult, ConnectRequest, OsDistribution, SessionId, SessionState, SftpDirectory,
    SftpMetadata, TerminalEvent,
};
use crate::ssh::{
    BackgroundConnection, ExecChannel, RemoteCommand, RemoteExecResult, SftpChannel, SshService,
};
use crate::transfers::TransferQueue;

const TERMINAL_CONTEXT_LIMIT: usize = 32 * 1024;
const AGENT_TERMINAL_COMMAND_TIMEOUT: Duration = Duration::from_secs(60);
const AGENT_TERMINAL_POLL_INTERVAL: Duration = Duration::from_millis(75);

#[derive(Default)]
struct TerminalContextBuffer {
    bytes: Vec<u8>,
    total_bytes: u64,
}

impl TerminalContextBuffer {
    fn append(&mut self, data: &[u8]) {
        self.total_bytes = self
            .total_bytes
            .saturating_add(u64::try_from(data.len()).unwrap_or(u64::MAX));
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

    fn mark(&self) -> u64 {
        self.total_bytes
    }

    fn text_since(&self, mark: u64) -> String {
        let retained_start = self
            .total_bytes
            .saturating_sub(u64::try_from(self.bytes.len()).unwrap_or(u64::MAX));
        let offset = mark.saturating_sub(retained_start);
        let offset = usize::try_from(offset)
            .unwrap_or(self.bytes.len())
            .min(self.bytes.len());
        String::from_utf8_lossy(&self.bytes[offset..]).into_owned()
    }
}

/// Output captured from a command entered into the visible interactive PTY.
/// The same bytes continue to flow to xterm through `TerminalEvent::Output`;
/// this copy stays inside Rust and is used only as untrusted model evidence.
pub(crate) struct TerminalCommandResult {
    pub output: String,
    pub timed_out: bool,
}

struct SessionHandle {
    profile_id: uuid::Uuid,
    client: Arc<client::Handle<super::service::HostKeyHandler>>,
    writer: Arc<ChannelWriteHalf<client::Msg>>,
    sftp: Arc<Mutex<Option<Arc<SftpChannel>>>>,
    terminal_context: Arc<Mutex<TerminalContextBuffer>>,
    agent_command_lock: Arc<Mutex<()>>,
    output: Option<Channel<TerminalEvent>>,
    // Keeps every outer SSH transport alive for the lifetime of a jump-host session.
    _route_owner: Option<Arc<BackgroundConnection>>,
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
    pub(crate) async fn has_active_sessions(&self) -> bool {
        !self.sessions.read().await.is_empty()
    }

    pub(crate) async fn disconnect_all(&self) -> AppResult<()> {
        let session_ids = self
            .sessions
            .read()
            .await
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for session_id in session_ids {
            match self.disconnect(session_id).await {
                Ok(()) | Err(AppError::SessionNotFound) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn forward_transport(
        &self,
        session_id: SessionId,
        profile_id: uuid::Uuid,
    ) -> AppResult<super::ForwardTransport> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(&session_id).ok_or(AppError::SessionNotFound)?;
        if session.profile_id != profile_id {
            return Err(AppError::TunnelSessionMismatch);
        }
        if session.client.is_closed() {
            return Err(AppError::ConnectionLost);
        }
        Ok(super::ForwardTransport::new(&session.client))
    }

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
                agent_command_lock: Arc::new(Mutex::new(())),
                output: Some(output.clone()),
                _route_owner: None,
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

    pub(crate) async fn connect_via(
        &self,
        profile_id: uuid::Uuid,
        route_owner: Arc<BackgroundConnection>,
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
        let opened =
            match SshService::connect_via(&route_owner.transport(), request, expected_fingerprint)
                .await
            {
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
                agent_command_lock: Arc::new(Mutex::new(())),
                output: Some(output.clone()),
                _route_owner: Some(route_owner),
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

    /// Enters an approved command into the existing interactive terminal and
    /// sends Enter. Completion is detected from the returning shell prompt.
    /// An idle period is not completion: silent installers can echo the
    /// command, pause while downloading, and only then print an error. No
    /// frontend shell API or secondary SSH exec channel is involved.
    pub(crate) async fn execute_terminal_command(
        &self,
        session_id: SessionId,
        command: &str,
    ) -> AppResult<TerminalCommandResult> {
        let (writer, context, command_lock) = {
            let sessions = self.sessions.read().await;
            let session = sessions.get(&session_id).ok_or(AppError::SessionNotFound)?;
            if session.client.is_closed() {
                return Err(AppError::ConnectionLost);
            }
            (
                Arc::clone(&session.writer),
                Arc::clone(&session.terminal_context),
                Arc::clone(&session.agent_command_lock),
            )
        };
        let _exclusive = command_lock.lock().await;
        let (mark, prompt_hint) = {
            let current = context.lock().await;
            (current.mark(), last_terminal_line(&current.text()))
        };

        let input = terminal_command_input(command);
        writer
            .data(&input[..])
            .await
            .map_err(|_| AppError::ConnectionLost)?;

        let started = Instant::now();
        loop {
            sleep(AGENT_TERMINAL_POLL_INTERVAL).await;
            let captured = context.lock().await.text_since(mark);
            if !captured.is_empty() && terminal_prompt_returned(&captured, prompt_hint.as_deref()) {
                return Ok(TerminalCommandResult {
                    output: clean_terminal_capture(&captured, command, prompt_hint.as_deref()),
                    timed_out: false,
                });
            }
            if started.elapsed() >= AGENT_TERMINAL_COMMAND_TIMEOUT {
                return Ok(TerminalCommandResult {
                    output: clean_terminal_capture(&captured, command, prompt_hint.as_deref()),
                    timed_out: true,
                });
            }
        }
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

    /// Validates both immutable profile ownership and live transport state for
    /// an operation that is already bound to an exact session.
    pub(crate) async fn validate_profile_session(
        &self,
        session_id: SessionId,
        profile_id: uuid::Uuid,
    ) -> AppResult<()> {
        let client = {
            let sessions = self.sessions.read().await;
            let session = sessions.get(&session_id).ok_or(AppError::SessionNotFound)?;
            if session.profile_id != profile_id {
                return Err(AppError::AgentFleetTargetMismatch);
            }
            Arc::clone(&session.client)
        };
        if client.is_closed() {
            return Err(AppError::ConnectionLost);
        }

        // `Handle::is_closed` can remain false until the idle TCP connection is
        // used. Approval revalidation must fail closed before scheduling a
        // Fleet child, so probe the transport with an otherwise unused SSH
        // session channel and bound the round trip.
        let channel = tokio::time::timeout(Duration::from_secs(5), client.channel_open_session())
            .await
            .map_err(|_| AppError::ConnectionLost)?
            .map_err(|_| AppError::ConnectionLost)?;
        channel
            .close()
            .await
            .map_err(|_| AppError::ConnectionLost)?;
        Ok(())
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

    /// Extracts only the working-directory metadata from the current shell
    /// prompt. Raw terminal content never crosses this boundary.
    pub(crate) async fn current_terminal_directory(
        &self,
        session_id: SessionId,
        username: &str,
    ) -> AppResult<Option<String>> {
        let recent = self.recent_terminal_output(session_id).await?;
        Ok(last_terminal_line(&recent)
            .as_deref()
            .and_then(|line| prompt_directory(line, username)))
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

fn last_terminal_line(value: &str) -> Option<String> {
    let plain = strip_terminal_control(value);
    plain
        .split(['\r', '\n'])
        .rev()
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

fn terminal_command_input(command: &str) -> Vec<u8> {
    let mut input = command.as_bytes().to_vec();
    input.push(b'\r');
    input
}

fn prompt_directory(prompt: &str, username: &str) -> Option<String> {
    let after_host = prompt.rsplit_once(':')?.1;
    let directory = after_host
        .trim_end_matches(|character: char| {
            character.is_whitespace() || matches!(character, '$' | '#' | '%')
        })
        .trim();
    if directory == "~" {
        return Some(if username == "root" {
            "/root".into()
        } else {
            format!("/home/{username}")
        });
    }
    if let Some(relative) = directory.strip_prefix("~/") {
        let home = if username == "root" {
            "/root".to_owned()
        } else {
            format!("/home/{username}")
        };
        return Some(format!("{home}/{relative}"));
    }
    directory.starts_with('/').then(|| directory.to_owned())
}

fn terminal_prompt_returned(captured: &str, prompt_hint: Option<&str>) -> bool {
    let plain = strip_terminal_control(captured);
    let tail = plain
        .split(['\r', '\n'])
        .rev()
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if let Some(prompt) = prompt_hint.filter(|prompt| prompt.len() >= 2) {
        if tail.ends_with(prompt) {
            return true;
        }
    }
    let trimmed = tail.trim_end();
    (tail.ends_with("$ ") || tail.ends_with("# ") || tail.ends_with("% "))
        || (trimmed.ends_with('$') || trimmed.ends_with('#') || trimmed.ends_with('%'))
            && (captured.contains('\r') || captured.contains('\n'))
}

fn clean_terminal_capture(captured: &str, command: &str, prompt_hint: Option<&str>) -> String {
    let plain = strip_terminal_control(captured).replace('\r', "");
    let mut lines = plain.lines().collect::<Vec<_>>();
    if lines
        .first()
        .is_some_and(|line| line.trim() == command.trim())
    {
        lines.remove(0);
    }
    if let Some(prompt) = prompt_hint {
        if lines.last().is_some_and(|line| line.ends_with(prompt)) {
            lines.pop();
        }
    }
    lines.join("\n").trim().to_owned()
}

fn strip_terminal_control(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\u{1b}' {
            output.push(character);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') => {
                let mut previous_escape = false;
                for next in chars.by_ref() {
                    if next == '\u{7}' || (previous_escape && next == '\\') {
                        break;
                    }
                    previous_escape = next == '\u{1b}';
                }
            }
            Some(_) | None => {}
        }
    }
    output
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

    #[test]
    fn terminal_capture_removes_echo_and_prompt_but_keeps_output() {
        let captured = "df -h\r\n/dev/sda2 100G 94G 6G 94% /\r\nzzly@host:~$ ";
        assert!(terminal_prompt_returned(captured, Some("zzly@host:~$ ")));
        assert_eq!(
            clean_terminal_capture(captured, "df -h", Some("zzly@host:~$ ")),
            "/dev/sda2 100G 94G 6G 94% /"
        );
    }

    #[test]
    fn command_echo_without_a_returned_prompt_is_not_complete() {
        let command = "curl -sL https://npmjs.org/install.sh | sh && npm install -g pm2";
        let captured = format!("{command}\r\n");
        assert!(!terminal_prompt_returned(
            &captured,
            Some("[root@localhost ~]# ")
        ));
        assert!(
            clean_terminal_capture(&captured, command, Some("[root@localhost ~]# ")).is_empty()
        );
    }

    #[test]
    fn approved_agent_command_is_sent_to_terminal_with_enter() {
        assert_eq!(terminal_command_input("df -h"), b"df -h\r");
    }

    #[test]
    fn prompt_metadata_resolves_the_remote_working_directory() {
        assert_eq!(
            prompt_directory("zzly@server:~$ ", "zzly").as_deref(),
            Some("/home/zzly")
        );
        assert_eq!(
            prompt_directory("root@server:~/logs# ", "root").as_deref(),
            Some("/root/logs")
        );
    }
}
