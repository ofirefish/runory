//! Browser OAuth helpers for ChatGPT (Codex-compatible PKCE) and OpenRouter.
//! Secrets never leave this module via logs; callers persist tokens through ModelGateway.

use std::time::Duration;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ring::digest::{digest, SHA256};
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::time::timeout;

use crate::domain::{AppError, AppResult};

pub(crate) const CHATGPT_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub(crate) const CHATGPT_ISSUER: &str = "https://auth.openai.com";
pub(crate) const CHATGPT_RESPONSES_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
const CHATGPT_CALLBACK_PORTS: &[u16] = &[1455, 1457, 1456];
const OPENROUTER_AUTH_BASE: &str = "https://openrouter.ai/auth";
const OPENROUTER_KEY_EXCHANGE: &str = "https://openrouter.ai/api/v1/auth/keys";
const OAUTH_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum OauthProvider {
    ChatGpt,
    OpenRouter,
}

#[derive(Debug)]
pub(crate) struct PkceCodes {
    pub verifier: String,
    pub challenge: String,
}

#[derive(Debug)]
pub(crate) struct ChatGptTokenSet {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at_epoch_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterKeyResponse {
    key: String,
}

#[derive(Debug, Deserialize)]
struct ChatGptTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    id_token: Option<String>,
}

pub(crate) fn generate_pkce() -> AppResult<PkceCodes> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| AppError::ModelUnavailable)?;
    let verifier = URL_SAFE_NO_PAD.encode(bytes);
    let challenge = URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes()).as_ref());
    Ok(PkceCodes {
        verifier,
        challenge,
    })
}

pub(crate) fn generate_state() -> AppResult<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| AppError::ModelUnavailable)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub(crate) fn chatgpt_authorize_url(redirect_uri: &str, pkce: &PkceCodes, state: &str) -> String {
    format!(
        "{}/oauth/authorize?response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&id_token_add_organizations=true&codex_cli_simplified_flow=true&state={}&originator=runory",
        CHATGPT_ISSUER,
        urlencoding_encode(CHATGPT_CLIENT_ID),
        urlencoding_encode(redirect_uri),
        urlencoding_encode("openid profile email offline_access"),
        urlencoding_encode(&pkce.challenge),
        urlencoding_encode(state),
    )
}

pub(crate) fn openrouter_authorize_url(callback_url: &str, pkce: &PkceCodes) -> String {
    format!(
        "{}?callback_url={}&code_challenge={}&code_challenge_method=S256",
        OPENROUTER_AUTH_BASE,
        urlencoding_encode(callback_url),
        urlencoding_encode(&pkce.challenge),
    )
}

pub(crate) fn open_system_browser(url: &str) -> AppResult<()> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = url;
        return Err(AppError::ModelOauthUnsupported);
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        open::that(url).map_err(|_| AppError::ModelUnavailable)
    }
}

pub(crate) async fn bind_loopback_listener(preferred: &[u16]) -> AppResult<(TcpListener, u16)> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = preferred;
        return Err(AppError::ModelOauthUnsupported);
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        for port in preferred {
            if let Ok(listener) = TcpListener::bind(("127.0.0.1", *port)).await {
                return Ok((listener, *port));
            }
        }
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|_| AppError::ModelUnavailable)?;
        let port = listener
            .local_addr()
            .map_err(|_| AppError::ModelUnavailable)?
            .port();
        Ok((listener, port))
    }
}

pub(crate) fn chatgpt_preferred_ports() -> &'static [u16] {
    CHATGPT_CALLBACK_PORTS
}

/// Wait for a single OAuth redirect on the bound listener. Validates `state` when present.
pub(crate) async fn await_authorization_code(
    listener: TcpListener,
    expected_state: Option<&str>,
    cancel: watch::Receiver<bool>,
) -> AppResult<String> {
    let accept = async {
        loop {
            if *cancel.borrow() {
                return Err(AppError::ModelOauthCancelled);
            }
            let (mut stream, _) = listener
                .accept()
                .await
                .map_err(|_| AppError::ModelUnavailable)?;
            let mut buffer = vec![0u8; 8192];
            let read = stream
                .read(&mut buffer)
                .await
                .map_err(|_| AppError::ModelUnavailable)?;
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let query = path.split_once('?').map(|(_, query)| query).unwrap_or("");
            let params = parse_query(query);
            if let Some(expected) = expected_state {
                if params.get("state").map(String::as_str) != Some(expected) {
                    let _ = write_callback_response(
                        &mut stream,
                        "Invalid OAuth state. You can close this window.",
                    )
                    .await;
                    continue;
                }
            }
            if let Some(error) = params.get("error") {
                let _ = write_callback_response(
                    &mut stream,
                    &format!("Authorization failed ({error}). You can close this window."),
                )
                .await;
                return Err(AppError::ModelAuthFailed);
            }
            let Some(code) = params
                .get("code")
                .cloned()
                .filter(|value| !value.is_empty())
            else {
                let _ = write_callback_response(
                    &mut stream,
                    "Missing authorization code. You can close this window.",
                )
                .await;
                continue;
            };
            let _ = write_callback_response(
                &mut stream,
                "Runory is connected. You can close this window and return to the app.",
            )
            .await;
            return Ok(code);
        }
    };

    match timeout(OAUTH_TIMEOUT, accept).await {
        Ok(result) => result,
        Err(_) => Err(AppError::ModelOauthCancelled),
    }
}

pub(crate) async fn exchange_openrouter_code(
    client: &reqwest::Client,
    code: &str,
    verifier: &str,
) -> AppResult<String> {
    let response = client
        .post(OPENROUTER_KEY_EXCHANGE)
        .json(&serde_json::json!({
            "code": code,
            "code_verifier": verifier,
            "code_challenge_method": "S256"
        }))
        .send()
        .await
        .map_err(|_| AppError::ModelUnavailable)?;
    if !response.status().is_success() {
        return Err(AppError::ModelAuthFailed);
    }
    let body: OpenRouterKeyResponse = response
        .json()
        .await
        .map_err(|_| AppError::ModelResponseInvalid)?;
    if body.key.len() < 8 {
        return Err(AppError::ModelAuthFailed);
    }
    Ok(body.key)
}

pub(crate) async fn exchange_chatgpt_code(
    client: &reqwest::Client,
    redirect_uri: &str,
    code: &str,
    verifier: &str,
) -> AppResult<ChatGptTokenSet> {
    let body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
        urlencoding_encode(code),
        urlencoding_encode(redirect_uri),
        urlencoding_encode(CHATGPT_CLIENT_ID),
        urlencoding_encode(verifier),
    );
    let response = client
        .post(format!("{CHATGPT_ISSUER}/oauth/token"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|_| AppError::ModelUnavailable)?;
    if !response.status().is_success() {
        return Err(AppError::ModelAuthFailed);
    }
    let token: ChatGptTokenResponse = response
        .json()
        .await
        .map_err(|_| AppError::ModelResponseInvalid)?;
    let refresh = token
        .refresh_token
        .filter(|value| !value.is_empty())
        .ok_or(AppError::ModelAuthFailed)?;
    let expires_at_epoch_ms = token.expires_in.map(|seconds| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_millis() as i64)
            .unwrap_or(0);
        now + seconds * 1000
    });
    let _ = token.id_token;
    Ok(ChatGptTokenSet {
        access_token: token.access_token,
        refresh_token: refresh,
        expires_at_epoch_ms,
    })
}

pub(crate) async fn refresh_chatgpt_token(
    client: &reqwest::Client,
    refresh_token: &str,
) -> AppResult<ChatGptTokenSet> {
    let body = format!(
        "grant_type=refresh_token&refresh_token={}&client_id={}",
        urlencoding_encode(refresh_token),
        urlencoding_encode(CHATGPT_CLIENT_ID),
    );
    let response = client
        .post(format!("{CHATGPT_ISSUER}/oauth/token"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|_| AppError::ModelUnavailable)?;
    if !response.status().is_success() {
        return Err(AppError::ModelAuthFailed);
    }
    let token: ChatGptTokenResponse = response
        .json()
        .await
        .map_err(|_| AppError::ModelResponseInvalid)?;
    let refresh = token
        .refresh_token
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| refresh_token.to_owned());
    let expires_at_epoch_ms = token.expires_in.map(|seconds| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_millis() as i64)
            .unwrap_or(0);
        now + seconds * 1000
    });
    Ok(ChatGptTokenSet {
        access_token: token.access_token,
        refresh_token: refresh,
        expires_at_epoch_ms,
    })
}

fn parse_query(query: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let Some(key) = parts.next().filter(|value| !value.is_empty()) else {
            continue;
        };
        let value = parts.next().unwrap_or("");
        map.insert(urlencoding_decode(key), urlencoding_decode(value));
    }
    map
}

async fn write_callback_response(
    stream: &mut tokio::net::TcpStream,
    message: &str,
) -> AppResult<()> {
    let body = format!(
        "<!DOCTYPE html><html><body style=\"font-family:sans-serif;padding:2rem\"><p>{message}</p></body></html>"
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|_| AppError::ModelUnavailable)?;
    let _ = stream.shutdown().await;
    Ok(())
}

fn urlencoding_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn urlencoding_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = &value[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    index += 3;
                } else {
                    out.push(b'%');
                    index += 1;
                }
            }
            other => {
                out.push(other);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_s256_base64url() {
        let pkce = generate_pkce().expect("pkce");
        assert!(pkce.verifier.len() >= 43);
        assert!(!pkce.challenge.contains('+'));
        assert!(!pkce.challenge.contains('/'));
        assert!(!pkce.challenge.contains('='));
    }

    #[test]
    fn query_parser_round_trips_code_and_state() {
        let params = parse_query("code=abc%2Fdef&state=xyz");
        assert_eq!(params.get("code").map(String::as_str), Some("abc/def"));
        assert_eq!(params.get("state").map(String::as_str), Some("xyz"));
    }

    #[test]
    fn chatgpt_authorize_url_includes_pkce_and_client() {
        let pkce = PkceCodes {
            verifier: "verifier".into(),
            challenge: "challenge".into(),
        };
        let url = chatgpt_authorize_url("http://localhost:1455/auth/callback", &pkce, "state1");
        assert!(url.contains(CHATGPT_CLIENT_ID));
        assert!(url.contains("code_challenge=challenge"));
        assert!(url.contains("state=state1"));
    }
}
