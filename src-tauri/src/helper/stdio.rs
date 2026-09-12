//! Stdio proxy: helper stdin/stdout become the SSH transport byte stream.

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf};
use tokio::process::{ChildStdin, ChildStdout};

use super::process::{spawn, ProcessSpec};
use super::redaction::redact_helper_output;
use super::{ExternalHelperManager, HelperError, HelperProcess};

/// Handle owning a stdio-proxy helper and its piped stdin/stdout.
pub struct StdioProxyHandle {
    pub process: HelperProcess,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
}

impl StdioProxyHandle {
    /// Combine stdin/stdout into a single AsyncRead + AsyncWrite for russh.
    pub fn into_transport(self) -> (HelperProcess, StdioTransport) {
        (
            self.process,
            StdioTransport {
                stdin: self.stdin,
                stdout: self.stdout,
            },
        )
    }
}

/// Bidirectional stream over helper stdout (read) and stdin (write).
pub struct StdioTransport {
    stdin: ChildStdin,
    stdout: ChildStdout,
}

impl AsyncRead for StdioTransport {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stdout).poll_read(cx, buf)
    }
}

impl AsyncWrite for StdioTransport {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut self.stdin).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.stdin).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.stdin).poll_shutdown(cx)
    }
}

pub async fn spawn_stdio_proxy(
    _manager: &dyn ExternalHelperManager,
    mut spec: ProcessSpec,
) -> Result<StdioProxyHandle, HelperError> {
    spec.stdin = true;
    spec.stdout = true;
    spec.stderr = true;
    let process = spawn(spec).await?;
    let mut child = process
        .take_child()
        .await
        .ok_or(HelperError::SpawnFailed)?;
    let stdin = child.stdin.take().ok_or(HelperError::SpawnFailed)?;
    let stdout = child.stdout.take().ok_or(HelperError::SpawnFailed)?;
    // Drain stderr so Teleport diagnostics cannot block the SSH byte stream on a full pipe.
    if let Some(mut stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            let mut line = Vec::new();
            loop {
                match stderr.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        for byte in &buf[..n] {
                            if *byte == b'\n' {
                                let text = String::from_utf8_lossy(&line);
                                let safe = redact_helper_output(text.trim());
                                if !safe.is_empty() {
                                    tracing::debug!(
                                        target: "TeleportProxy",
                                        stream = "stderr",
                                        line = %safe,
                                        "helper diagnostic"
                                    );
                                }
                                line.clear();
                            } else {
                                line.push(*byte);
                                if line.len() > 8 * 1024 {
                                    line.clear();
                                }
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }
    process.restore_child(child).await;
    Ok(StdioProxyHandle {
        process,
        stdin,
        stdout,
    })
}
