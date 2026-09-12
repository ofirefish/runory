//! Boundary authentication state (token / env).

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroize;

use crate::connection::bastion::auth::{
    provider_cli_path, AuthChallengeResponse, AuthSession, AuthStepResult, BastionCredential,
    BastionPrincipal, ProtectedProviderState,
};
use crate::connection::bastion::errors::BastionError;
use crate::connection::bastion::session::{BastionContext, BastionEndpoint};
use crate::helper::{ExternalHelperManager, HelperError, VersionConstraint};

/// Opaque Boundary session payload kept only in `ProtectedProviderState`.
#[derive(Clone, Serialize, Deserialize)]
pub struct BoundarySessionState {
    pub addr: String,
    pub token: String,
}

impl Drop for BoundarySessionState {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}

impl BoundarySessionState {
    pub fn encode(&self) -> Result<ProtectedProviderState, BastionError> {
        let bytes = serde_json::to_vec(self).map_err(|_| BastionError::Internal)?;
        Ok(ProtectedProviderState::new(bytes))
    }

    pub fn decode(state: &ProtectedProviderState) -> Option<Self> {
        serde_json::from_slice(state.as_bytes()).ok()
    }
}

/// Resolve Controller URL for Boundary CLI (`BOUNDARY_ADDR` / `-addr`).
pub fn resolve_controller_addr(endpoint: &BastionEndpoint) -> String {
    if let Some(base) = endpoint
        .provider_config
        .get("apiBaseUrl")
        .or_else(|| endpoint.provider_config.get("api_base_url"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return base.trim_end_matches('/').to_string();
    }
    let host = endpoint.host.trim();
    if host.is_empty() {
        return "http://127.0.0.1:9200".into();
    }
    if host.contains("://") {
        return host.trim_end_matches('/').to_string();
    }
    let port = endpoint
        .ports
        .api
        .or(endpoint.ports.web)
        .or(endpoint.ports.ssh)
        .unwrap_or(9200);
    // Boundary CLI defaults to HTTPS; many lab Controllers speak plain HTTP.
    let scheme = if port == 443 { "https" } else { "http" };
    format!("{scheme}://{host}:{port}")
}

pub async fn start_auth(
    helpers: &dyn ExternalHelperManager,
    ctx: &BastionContext,
    credential: &BastionCredential,
) -> Result<AuthStepResult, BastionError> {
    let cli_override = provider_cli_path(&ctx.endpoint.provider_config);
    let binary = helpers
        .locate_binary_with_override(cli_override.as_deref(), &["boundary", "boundary.exe"])
        .map_err(map_helper)?;
    let _ = helpers
        .check_version(&binary, &VersionConstraint::at_least(0, 15))
        .map_err(map_helper)?;

    let addr = resolve_controller_addr(&ctx.endpoint);
    let username = match credential {
        BastionCredential::Token {
            username_hint: Some(hint),
            ..
        } if !hint.trim().is_empty() => hint.trim().to_string(),
        BastionCredential::Token { .. } => "boundary".to_string(),
        BastionCredential::Password { username, .. } => username.clone(),
        BastionCredential::BrowserSso { username_hint } => username_hint
            .clone()
            .unwrap_or_else(|| "boundary".into()),
        BastionCredential::ExternalAgent { username } => username.clone(),
        _ => "boundary".into(),
    };

    match credential {
        BastionCredential::Token {
            transient_token: Some(token),
            ..
        } => {
            let token = token.trim();
            if token.is_empty() {
                return Err(BastionError::AuthenticationFailed);
            }
            let state = BoundarySessionState {
                addr,
                token: token.to_string(),
            };
            Ok(AuthStepResult::Authenticated(AuthSession {
                id: Uuid::new_v4(),
                bastion_id: ctx.endpoint.id,
                provider: "boundary".into(),
                principal: BastionPrincipal {
                    username,
                    display_name: None,
                },
                provider_state: state.encode()?,
                expires_at: None,
                helper_cli_override: cli_override,
            }))
        }
        BastionCredential::Password {
            transient_password: Some(_),
            ..
        } => {
            // Password-to-token exchange via CLI is follow-up; require Token for now.
            Err(BastionError::CapabilityUnavailable)
        }
        BastionCredential::Token { .. } | BastionCredential::Password { .. } => {
            Err(BastionError::AuthenticationFailed)
        }
        BastionCredential::BrowserSso { .. } => Err(BastionError::CapabilityUnavailable),
        _ => Err(BastionError::CapabilityUnavailable),
    }
}

pub async fn continue_auth(
    _helpers: &dyn ExternalHelperManager,
    session: &AuthSession,
    response: AuthChallengeResponse,
) -> Result<AuthStepResult, BastionError> {
    if session.provider != "boundary" {
        return Err(BastionError::ProviderUnavailable);
    }
    match response {
        AuthChallengeResponse::Cancel { .. } => Err(BastionError::Cancelled),
        _ => Err(BastionError::ProviderProtocolError),
    }
}

pub fn map_helper(error: HelperError) -> BastionError {
    match error {
        HelperError::Missing => BastionError::HelperMissing,
        HelperError::VersionMismatch => BastionError::HelperVersionMismatch,
        HelperError::Cancelled => BastionError::Cancelled,
        _ => BastionError::HelperProxyFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::bastion::session::BastionPorts;
    use uuid::Uuid;

    #[test]
    fn resolve_prefers_api_base_url_and_defaults_to_http() {
        let endpoint = BastionEndpoint {
            id: Uuid::new_v4(),
            provider: "boundary".into(),
            name: "lab".into(),
            host: "192.168.133.231".into(),
            ports: BastionPorts {
                api: Some(22),
                ssh: Some(22),
                web: Some(22),
            },
            tls: None,
            provider_config: serde_json::json!({
                "apiBaseUrl": "http://192.168.133.231:9200"
            }),
        };
        assert_eq!(
            resolve_controller_addr(&endpoint),
            "http://192.168.133.231:9200"
        );
    }
}
