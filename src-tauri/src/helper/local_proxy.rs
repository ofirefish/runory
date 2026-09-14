//! Local TCP proxy: helper listens on 127.0.0.1; SSH Core dials that port.

use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::time::timeout;

use crate::connection::transport::LocalEndpointStrategy;

use super::process::{spawn, ProcessSpec};
use super::redaction::redact_helper_output;
use super::{ExternalHelperManager, HelperError, HelperProcess};

#[derive(Clone, Debug)]
pub struct LocalProxySpec {
    pub process: ProcessSpec,
    pub endpoint: LocalEndpointStrategy,
    pub ready_timeout: Duration,
}

/// Running local proxy with a discovered loopback endpoint.
pub struct LocalProxyHandle {
    pub process: HelperProcess,
    pub host: String,
    pub port: u16,
    /// First JSON / ready line from the helper (Boundary connect credentials). Never log.
    pub ready_payload: Option<zeroize::Zeroizing<String>>,
}

pub async fn spawn_local_proxy(
    _manager: &dyn ExternalHelperManager,
    mut spec: LocalProxySpec,
) -> Result<LocalProxyHandle, HelperError> {
    spec.process.stdout = true;
    spec.process.stderr = true;
    let process = spawn(spec.process).await?;

    match &spec.endpoint {
        LocalEndpointStrategy::Fixed { host, port } => {
            if host.trim().is_empty() || *port == 0 {
                let _ = super::process::kill_process_tree(&process).await;
                return Err(HelperError::EndpointDiscoveryFailed);
            }
            // Drain helper pipes so Boundary cannot block on a full stdout/stderr buffer.
            let captured = spawn_pipe_drainers(&process).await;
            // Wait until the helper binds the listen port (do not dial — that can
            // consume Boundary session connection slots with limit=1).
            wait_for_listener(host, *port, spec.ready_timeout, &process).await?;
            let ready_payload = wait_for_ready_payload(&captured, Duration::from_secs(5)).await;
            Ok(LocalProxyHandle {
                process,
                host: host.clone(),
                port: *port,
                ready_payload,
            })
        }
        LocalEndpointStrategy::Stdout | LocalEndpointStrategy::Stderr => {
            discover_endpoint(process, spec.ready_timeout).await
        }
    }
}

type CapturedPayload = std::sync::Arc<tokio::sync::Mutex<Option<zeroize::Zeroizing<String>>>>;

async fn spawn_pipe_drainers(process: &HelperProcess) -> CapturedPayload {
    let captured: CapturedPayload =
        std::sync::Arc::new(tokio::sync::Mutex::new(None::<zeroize::Zeroizing<String>>));
    let mut child = match process.take_child().await {
        Some(child) => child,
        None => return captured,
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    process.restore_child(child).await;
    if let Some(stdout) = stdout {
        let captured = std::sync::Arc::clone(&captured);
        tokio::spawn(async move {
            capture_json_from_stream(stdout, captured, "stdout").await;
        });
    }
    if let Some(stderr) = stderr {
        let captured = std::sync::Arc::clone(&captured);
        tokio::spawn(async move {
            capture_json_from_stream(stderr, captured, "stderr").await;
        });
    }
    captured
}

async fn capture_json_from_stream<R: tokio::io::AsyncRead + Unpin>(
    stream: R,
    captured: CapturedPayload,
    stream_name: &'static str,
) {
    let mut lines = BufReader::new(stream).lines();
    let mut buf = String::new();
    while let Ok(Some(line)) = lines.next_line().await {
        let safe = redact_helper_output(&line);
        tracing::debug!(stream = stream_name, line = %safe, "helper local-proxy output");
        buf.push_str(&line);
        buf.push('\n');
        if parse_boundary_json(&buf).is_some() || looks_like_json_object(&line) {
            let mut slot = captured.lock().await;
            if slot.is_none() {
                *slot = Some(zeroize::Zeroizing::new(buf.clone()));
            }
        }
    }
}

fn looks_like_json_object(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('{') && trimmed.contains("\"port\"")
}

async fn wait_for_ready_payload(
    captured: &CapturedPayload,
    timeout: Duration,
) -> Option<zeroize::Zeroizing<String>> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(payload) = captured.lock().await.clone() {
            return Some(payload);
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_listener(
    host: &str,
    port: u16,
    ready_timeout: Duration,
    process: &HelperProcess,
) -> Result<(), HelperError> {
    let deadline = tokio::time::Instant::now() + ready_timeout;
    let bind_host = if host == "localhost" {
        "127.0.0.1"
    } else {
        host
    };
    loop {
        if tokio::time::Instant::now() >= deadline {
            let _ = super::process::kill_process_tree(process).await;
            return Err(HelperError::ReadyTimeout);
        }
        match std::net::TcpListener::bind((bind_host, port)) {
            Err(error)
                if error.kind() == std::io::ErrorKind::AddrInUse
                    || error.raw_os_error() == Some(10048) /* WSAEADDRINUSE */ =>
            {
                return Ok(());
            }
            Ok(listener) => {
                drop(listener);
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
        }
    }
}

async fn discover_endpoint(
    process: HelperProcess,
    ready_timeout: Duration,
) -> Result<LocalProxyHandle, HelperError> {
    let mut child = process.take_child().await.ok_or(HelperError::SpawnFailed)?;
    let stdout = child.stdout.take().ok_or(HelperError::SpawnFailed)?;
    let stderr = child.stderr.take().ok_or(HelperError::SpawnFailed)?;
    process.restore_child(child).await;

    let discovery = read_endpoint_from_pipes(stdout, stderr);
    match timeout(ready_timeout, discovery).await {
        Ok(Ok((host, port, payload))) => Ok(LocalProxyHandle {
            process,
            host,
            port,
            ready_payload: payload,
        }),
        Ok(Err(error)) => {
            let _ = super::process::kill_process_tree(&process).await;
            Err(error)
        }
        Err(_) => {
            tracing::warn!("helper local-proxy endpoint discovery timed out");
            let _ = super::process::kill_process_tree(&process).await;
            Err(HelperError::ReadyTimeout)
        }
    }
}

async fn read_endpoint_from_pipes(
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
) -> Result<(String, u16, Option<zeroize::Zeroizing<String>>), HelperError> {
    let mut out_lines = BufReader::new(stdout).lines();
    let mut err_lines = BufReader::new(stderr).lines();
    let mut out_buf = String::new();
    let mut err_buf = String::new();
    let mut out_done = false;
    let mut err_done = false;

    loop {
        if out_done && err_done {
            return Err(HelperError::EndpointDiscoveryFailed);
        }
        tokio::select! {
            line = out_lines.next_line(), if !out_done => {
                match line {
                    Ok(Some(line)) => {
                        let safe = redact_helper_output(&line);
                        tracing::debug!(stream = "stdout", line = %safe, "helper endpoint probe");
                        out_buf.push_str(&line);
                        out_buf.push('\n');
                        if let Some(endpoint) = parse_endpoint_from_text(&line)
                            .or_else(|| parse_endpoint_from_text(&out_buf))
                        {
                            let payload = parse_boundary_json(&out_buf)
                                .map(|_| zeroize::Zeroizing::new(out_buf.clone()));
                            return Ok((endpoint.0, endpoint.1, payload));
                        }
                    }
                    Ok(None) => out_done = true,
                    Err(_) => return Err(HelperError::Crashed),
                }
            }
            line = err_lines.next_line(), if !err_done => {
                match line {
                    Ok(Some(line)) => {
                        let safe = redact_helper_output(&line);
                        tracing::debug!(stream = "stderr", line = %safe, "helper endpoint probe");
                        err_buf.push_str(&line);
                        err_buf.push('\n');
                        if let Some(endpoint) = parse_endpoint_from_text(&line)
                            .or_else(|| parse_endpoint_from_text(&err_buf))
                        {
                            let payload = parse_boundary_json(&err_buf)
                                .map(|_| zeroize::Zeroizing::new(err_buf.clone()));
                            return Ok((endpoint.0, endpoint.1, payload));
                        }
                    }
                    Ok(None) => err_done = true,
                    Err(_) => return Err(HelperError::Crashed),
                }
            }
        }
    }
}

fn parse_endpoint_from_text(text: &str) -> Option<(String, u16)> {
    parse_local_endpoint(text).or_else(|| parse_boundary_json(text))
}

/// Parse Boundary `connect -format=json` payloads (`address` + `port` fields).
pub fn parse_boundary_json(text: &str) -> Option<(String, u16)> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(endpoint) = extract_json_endpoint(&value) {
            return Some(endpoint);
        }
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end <= start {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(&trimmed[start..=end]).ok()?;
    extract_json_endpoint(&value)
}

fn extract_json_endpoint(value: &serde_json::Value) -> Option<(String, u16)> {
    let address = value
        .get("address")
        .or_else(|| value.pointer("/connection/address"))
        .or_else(|| value.pointer("/proxy/address"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let port = value
        .get("port")
        .or_else(|| value.pointer("/connection/port"))
        .or_else(|| value.pointer("/proxy/port"))
        .and_then(|v| v.as_u64())
        .and_then(|v| u16::try_from(v).ok())
        .filter(|port| *port > 0)?;
    let host = match address {
        "localhost" | "::1" | "[::1]" => "127.0.0.1".to_string(),
        other => other.to_string(),
    };
    Some((host, port))
}

/// Parse common helper listen announcements:
/// `127.0.0.1:12345`, `listening on 127.0.0.1:12345`, `Proxy listening at: 127.0.0.1:12345`
pub fn parse_local_endpoint(line: &str) -> Option<(String, u16)> {
    for token in line.split_whitespace() {
        let cleaned = token.trim_matches(|c: char| c == ',' || c == ';' || c == '"' || c == '\'');
        if let Some((host, port_str)) = cleaned.rsplit_once(':') {
            if host == "127.0.0.1" || host == "localhost" || host == "[::1]" || host == "::1" {
                if let Ok(port) = port_str.parse::<u16>() {
                    if port > 0 {
                        let host = if host == "::1" || host == "[::1]" {
                            "127.0.0.1".to_string()
                        } else if host == "localhost" {
                            "127.0.0.1".to_string()
                        } else {
                            host.to_string()
                        };
                        return Some((host, port));
                    }
                }
            }
        }
    }
    // Fallback: scan for host:port substring anywhere.
    for (idx, _) in line.match_indices("127.0.0.1:") {
        let rest = &line[idx + "127.0.0.1:".len()..];
        let port_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(port) = port_str.parse::<u16>() {
            if port > 0 {
                return Some(("127.0.0.1".into(), port));
            }
        }
    }
    None
}

/// Reserve an ephemeral loopback port for helpers that accept `-listen-port`.
pub fn reserve_loopback_port() -> Result<u16, HelperError> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|_| HelperError::EndpointDiscoveryFailed)?;
    let port = listener
        .local_addr()
        .map_err(|_| HelperError::EndpointDiscoveryFailed)?
        .port();
    drop(listener);
    if port == 0 {
        Err(HelperError::EndpointDiscoveryFailed)
    } else {
        Ok(port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_loopback() {
        assert_eq!(
            parse_local_endpoint("127.0.0.1:47281"),
            Some(("127.0.0.1".into(), 47281))
        );
    }

    #[test]
    fn parses_listening_on_line() {
        assert_eq!(
            parse_local_endpoint("Proxy listening at: 127.0.0.1:52001"),
            Some(("127.0.0.1".into(), 52001))
        );
    }

    #[test]
    fn parses_boundary_json_address_port() {
        let json = r#"{
          "credentials": [],
          "address": "127.0.0.1",
          "port": 53921,
          "session_id": "s_abc"
        }"#;
        assert_eq!(parse_boundary_json(json), Some(("127.0.0.1".into(), 53921)));
    }

    #[test]
    fn parses_boundary_json_embedded_in_logs() {
        let text = "starting proxy\n{\"address\":\"127.0.0.1\",\"port\":40001}\nready";
        assert_eq!(parse_boundary_json(text), Some(("127.0.0.1".into(), 40001)));
    }
}
