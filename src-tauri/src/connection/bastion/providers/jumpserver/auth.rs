use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::connection::bastion::auth::{AuthSession, BastionPrincipal, ProtectedProviderState};
use crate::connection::bastion::session::BastionEndpoint;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "camelCase")]
pub enum JumpServerAuthState {
    PendingMfa {
        base_url: String,
        koko_host: String,
        koko_port: u16,
        /// JumpServer `jms_sessionid` cookie value (not a Bearer token).
        session_id: String,
        /// Relative MFA verify/challenge path from `data.url`.
        mfa_path: String,
        username: String,
        /// Transient password kept only for the post-OTP re-auth call. In-memory AuthSession only.
        password: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        org_id: Option<String>,
    },
    /// Password / MFA login issued a Bearer for control-plane API calls.
    Authenticated {
        base_url: String,
        koko_host: String,
        koko_port: u16,
        bearer: String,
        username: String,
        /// JumpServer login password kept only in-memory for KoKo direct-login fallback
        /// (`user@account@asset`). Cleared when the AuthSession ends; never logged.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        password: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        org_id: Option<String>,
    },
    /// Access Key ID + Secret: each API call is HMAC-signed (no Bearer).
    AuthenticatedAccessKey {
        base_url: String,
        koko_host: String,
        koko_port: u16,
        key_id: String,
        /// In-memory only; never logged. Used to sign REST requests.
        secret: String,
        username: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        org_id: Option<String>,
    },
}

impl JumpServerAuthState {
    pub fn from_endpoint(
        endpoint: &BastionEndpoint,
        stage: JumpServerStage,
    ) -> Result<Self, ()> {
        let base_url = endpoint_base_url(endpoint)?;
        let koko_host = normalize_koko_host(&endpoint.host).ok_or(())?;
        let koko_port = normalize_koko_ssh_port(endpoint.ports.ssh.unwrap_or(2222));
        let org_id = resolve_org_id(endpoint);
        Ok(match stage {
            JumpServerStage::PendingMfa {
                session_id,
                mfa_path,
                username,
                password,
            } => Self::PendingMfa {
                base_url,
                koko_host,
                koko_port,
                session_id,
                mfa_path,
                username,
                password,
                org_id,
            },
            JumpServerStage::Authenticated {
                bearer,
                username,
                password,
            } => Self::Authenticated {
                base_url,
                koko_host,
                koko_port,
                bearer,
                username,
                password,
                org_id,
            },
            JumpServerStage::AuthenticatedAccessKey {
                key_id,
                secret,
                username,
            } => Self::AuthenticatedAccessKey {
                base_url,
                koko_host,
                koko_port,
                key_id,
                secret,
                username,
                org_id,
            },
        })
    }

    pub fn encode(&self) -> Result<ProtectedProviderState, ()> {
        let bytes = serde_json::to_vec(self).map_err(|_| ())?;
        Ok(ProtectedProviderState::new(bytes))
    }

    pub fn decode(state: &ProtectedProviderState) -> Option<Self> {
        serde_json::from_slice(state.as_bytes()).ok()
    }

    pub fn base_url(&self) -> &str {
        match self {
            Self::PendingMfa { base_url, .. }
            | Self::Authenticated { base_url, .. }
            | Self::AuthenticatedAccessKey { base_url, .. } => base_url,
        }
    }

    pub fn koko_endpoint(&self) -> (&str, u16) {
        match self {
            Self::PendingMfa {
                koko_host,
                koko_port,
                ..
            }
            | Self::Authenticated {
                koko_host,
                koko_port,
                ..
            }
            | Self::AuthenticatedAccessKey {
                koko_host,
                koko_port,
                ..
            } => (koko_host.as_str(), *koko_port),
        }
    }

    pub fn org_id(&self) -> Option<&str> {
        match self {
            Self::PendingMfa { org_id, .. }
            | Self::Authenticated { org_id, .. }
            | Self::AuthenticatedAccessKey { org_id, .. } => org_id.as_deref(),
        }
    }

    /// Control-plane auth material for subsequent JumpServer API calls.
    pub fn api_auth(&self) -> Option<super::api::JumpServerApiAuth> {
        match self {
            Self::Authenticated { bearer, org_id, .. } => Some(super::api::JumpServerApiAuth::Bearer {
                token: bearer.clone(),
                org_id: org_id.clone(),
            }),
            Self::AuthenticatedAccessKey {
                key_id,
                secret,
                org_id,
                ..
            } => Some(super::api::JumpServerApiAuth::AccessKey {
                key_id: key_id.clone(),
                secret: secret.clone(),
                org_id: org_id.clone(),
            }),
            Self::PendingMfa { .. } => None,
        }
    }

    pub fn password(&self) -> Option<&str> {
        match self {
            Self::Authenticated { password, .. } if !password.is_empty() => Some(password.as_str()),
            _ => None,
        }
    }

    pub fn username(&self) -> &str {
        match self {
            Self::PendingMfa { username, .. }
            | Self::Authenticated { username, .. }
            | Self::AuthenticatedAccessKey { username, .. } => username,
        }
    }
}

pub enum JumpServerStage {
    PendingMfa {
        session_id: String,
        mfa_path: String,
        username: String,
        password: String,
    },
    Authenticated {
        bearer: String,
        username: String,
        password: String,
    },
    AuthenticatedAccessKey {
        key_id: String,
        secret: String,
        username: String,
    },
}

/// JumpServer Default organization. Official Access Key examples always send this header.
pub const DEFAULT_JMS_ORG_ID: &str = "00000000-0000-0000-0000-000000000002";

/// Optional JumpServer organization id from profile provider_config (`orgId`).
/// Falls back to the Default org so Access Key signatures match official clients.
pub fn resolve_org_id(endpoint: &BastionEndpoint) -> Option<String> {
    endpoint
        .provider_config
        .get("orgId")
        .or_else(|| endpoint.provider_config.get("org_id"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| Some(DEFAULT_JMS_ORG_ID.to_string()))
}

pub fn auth_session(
    bastion_id: Uuid,
    state: JumpServerAuthState,
    expires_at: Option<i64>,
) -> Result<AuthSession, ()> {
    let username = state.username().to_string();
    Ok(AuthSession {
        id: Uuid::new_v4(),
        bastion_id,
        provider: "jumpserver".into(),
        principal: BastionPrincipal {
            username,
            display_name: None,
        },
        provider_state: state.encode()?,
        expires_at,
        helper_cli_override: None,
    })
}

fn endpoint_base_url(endpoint: &BastionEndpoint) -> Result<String, ()> {
    resolve_api_base_url(endpoint).ok_or(())
}

/// Parsed JumpServer Web/API base (no trailing slash).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedApiBaseUrl {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub base_url: String,
}

/// Parse `http://host:61080` or `https://host` into scheme/host/port/base.
pub fn parse_api_base_url(raw: &str) -> Option<ParsedApiBaseUrl> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let (scheme, rest) = if let Some(rest) = trimmed.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        ("http", rest)
    } else {
        return None;
    };
    let authority = rest.split('/').next().unwrap_or("").trim();
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let (host, port) = if let Some(host) = authority.strip_prefix('[') {
        let end = host.find(']')?;
        let host_name = &host[..end];
        let after = &host[end + 1..];
        let port = if let Some(port_text) = after.strip_prefix(':') {
            port_text.parse().ok()?
        } else if after.is_empty() {
            if scheme == "http" {
                80
            } else {
                443
            }
        } else {
            return None;
        };
        (host_name.to_string(), port)
    } else if let Some((host, port_text)) = authority.rsplit_once(':') {
        if host.contains(':') {
            // Ambiguous IPv6 without brackets — reject.
            return None;
        }
        (host.to_string(), port_text.parse().ok()?)
    } else {
        (
            authority.to_string(),
            if scheme == "http" { 80 } else { 443 },
        )
    };
    if host.is_empty() || port == 0 {
        return None;
    }
    Some(ParsedApiBaseUrl {
        scheme: scheme.to_string(),
        host,
        port,
        base_url: format!("{scheme}://{authority}"),
    })
}

pub fn resolve_api_base_url(endpoint: &BastionEndpoint) -> Option<String> {
    if let Some(url) = endpoint
        .provider_config
        .get("apiBaseUrl")
        .and_then(|value| value.as_str())
        .and_then(parse_api_base_url)
    {
        return Some(url.base_url);
    }
    let host = normalize_koko_host(&endpoint.host)?;
    let port = endpoint.ports.api.or(endpoint.ports.web).unwrap_or(443);
    let scheme = endpoint
        .provider_config
        .get("apiScheme")
        .and_then(|value| value.as_str())
        .unwrap_or(if port == 80 { "http" } else { "https" });
    if scheme != "http" && scheme != "https" {
        return None;
    }
    Some(format!("{scheme}://{host}:{port}"))
}

/// Strip accidental `http(s)://` / path from a KoKo SSH host field.
pub fn normalize_koko_host(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    if let Some(parsed) = parse_api_base_url(trimmed) {
        return Some(parsed.host);
    }
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .or_else(|| trimmed.strip_prefix("HTTPS://"))
        .or_else(|| trimmed.strip_prefix("HTTP://"))
        .unwrap_or(trimmed);
    let authority = without_scheme.split('/').next().unwrap_or("").trim();
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let host = if let Some(host) = authority.strip_prefix('[') {
        let end = host.find(']')?;
        host[..end].to_string()
    } else if let Some((host, port_text)) = authority.rsplit_once(':') {
        if host.contains(':') {
            return None;
        }
        if port_text.parse::<u16>().is_ok() {
            host.to_string()
        } else {
            authority.to_string()
        }
    } else {
        authority.to_string()
    };
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

/// Web ports are never KoKo SSH; remap common misconfig (API URL pasted into SSH fields).
pub fn normalize_koko_ssh_port(port: u16) -> u16 {
    if port == 0 || port == 80 || port == 443 {
        2222
    } else {
        port
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::bastion::session::{BastionEndpoint, BastionPorts};
    use uuid::Uuid;

    #[test]
    fn parses_http_api_url_with_custom_port() {
        let parsed = parse_api_base_url("http://122.114.1.1:61080/")
            .expect("parse");
        assert_eq!(parsed.scheme, "http");
        assert_eq!(parsed.host, "122.114.1.1");
        assert_eq!(parsed.port, 61080);
        assert_eq!(parsed.base_url, "http://122.114.1.1:61080");
    }

    #[test]
    fn normalize_koko_host_strips_url_scheme() {
        assert_eq!(
            normalize_koko_host("http://jms.zzliaoyuan.com").as_deref(),
            Some("jms.zzliaoyuan.com")
        );
        assert_eq!(
            normalize_koko_host("https://jms.example.com:443/path").as_deref(),
            Some("jms.example.com")
        );
        assert_eq!(
            normalize_koko_host("jms.zzliaoyuan.com").as_deref(),
            Some("jms.zzliaoyuan.com")
        );
    }

    #[test]
    fn normalize_koko_ssh_port_rejects_web_ports() {
        assert_eq!(normalize_koko_ssh_port(80), 2222);
        assert_eq!(normalize_koko_ssh_port(443), 2222);
        assert_eq!(normalize_koko_ssh_port(2222), 2222);
        assert_eq!(normalize_koko_ssh_port(3022), 3022);
    }

    #[test]
    fn resolve_prefers_provider_config_api_base_url() {
        let endpoint = BastionEndpoint {
            id: Uuid::new_v4(),
            provider: "jumpserver".into(),
            name: "js".into(),
            host: "122.114.1.1".into(),
            ports: BastionPorts {
                api: Some(443),
                ssh: Some(2222),
                web: Some(443),
            },
            tls: None,
            provider_config: serde_json::json!({
                "apiBaseUrl": "http://122.114.1.1:61080"
            }),
        };
        assert_eq!(
            resolve_api_base_url(&endpoint).as_deref(),
            Some("http://122.114.1.1:61080")
        );
    }
}
