use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::connection::bastion::errors::BastionError;

use super::signer::JumpServerSigner;

/// Auth material for a JumpServer HTTP call. MFA uses session cookies, not Bearer.
#[derive(Clone)]
pub enum JumpServerRequestAuth {
    Bearer(String),
    /// Raw Cookie header value, e.g. `jms_sessionid=abc`.
    Cookie(String),
    /// Access Key: signed per-request with HMAC-SHA256 (secret never logged).
    AccessKey {
        key_id: String,
        secret: String,
    },
    /// JumpServer Private Token (`Authorization: Token …` / `PrivateToken …`).
    PrivateToken(String),
}

impl std::fmt::Debug for JumpServerRequestAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bearer(_) => f.write_str("Bearer(**REDACTED**)"),
            Self::Cookie(_) => f.write_str("Cookie(**REDACTED**)"),
            Self::AccessKey { key_id, .. } => f
                .debug_struct("AccessKey")
                .field("key_id", key_id)
                .field("secret", &"**REDACTED**")
                .finish(),
            Self::PrivateToken(_) => f.write_str("PrivateToken(**REDACTED**)"),
        }
    }
}

/// Control-plane credentials used by JumpServerApiClient business methods.
#[derive(Clone)]
pub enum JumpServerApiAuth {
    Bearer {
        token: String,
        org_id: Option<String>,
    },
    AccessKey {
        key_id: String,
        secret: String,
        org_id: Option<String>,
    },
    PrivateToken {
        token: String,
        org_id: Option<String>,
    },
}

impl std::fmt::Debug for JumpServerApiAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bearer { org_id, .. } => f
                .debug_struct("Bearer")
                .field("token", &"**REDACTED**")
                .field("org_id", org_id)
                .finish(),
            Self::AccessKey { key_id, org_id, .. } => f
                .debug_struct("AccessKey")
                .field("key_id", key_id)
                .field("secret", &"**REDACTED**")
                .field("org_id", org_id)
                .finish(),
            Self::PrivateToken { org_id, .. } => f
                .debug_struct("PrivateToken")
                .field("token", &"**REDACTED**")
                .field("org_id", org_id)
                .finish(),
        }
    }
}

impl JumpServerApiAuth {
    pub fn org_id(&self) -> Option<&str> {
        match self {
            Self::Bearer { org_id, .. }
            | Self::AccessKey { org_id, .. }
            | Self::PrivateToken { org_id, .. } => org_id.as_deref(),
        }
    }

    pub fn to_request_auth(&self) -> JumpServerRequestAuth {
        match self {
            Self::Bearer { token, .. } => JumpServerRequestAuth::Bearer(token.clone()),
            Self::AccessKey { key_id, secret, .. } => JumpServerRequestAuth::AccessKey {
                key_id: key_id.clone(),
                secret: secret.clone(),
            },
            Self::PrivateToken { token, .. } => JumpServerRequestAuth::PrivateToken(token.clone()),
        }
    }
}

/// Minimal HTTP surface so unit tests can drive JumpServer without a live server.
#[async_trait::async_trait]
pub trait JumpServerHttp: Send + Sync {
    async fn request(
        &self,
        method: &str,
        path: &str,
        auth: Option<JumpServerRequestAuth>,
        org_id: Option<&str>,
        body: Option<Value>,
    ) -> Result<JumpServerHttpResponse, BastionError>;
}

#[derive(Clone, Debug)]
pub struct JumpServerHttpResponse {
    pub status: u16,
    pub body: Value,
    pub cookies: Vec<(String, String)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JumpServerToken {
    pub token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JumpServerConnectionToken {
    pub id: String,
    pub value: String,
}

#[derive(Clone, Debug)]
pub struct JumpServerApiClient<H: JumpServerHttp> {
    http: H,
    base_path: String,
}

impl<H: JumpServerHttp> JumpServerApiClient<H> {
    pub fn new(http: H) -> Self {
        Self {
            http,
            base_path: "/api/v1".into(),
        }
    }

    pub async fn probe_version(&self) -> Result<Option<String>, BastionError> {
        let response = self
            .http
            .request(
                "GET",
                &format!("{}/settings/public/", self.base_path),
                None,
                None,
                None,
            )
            .await?;
        if response.status >= 400 {
            return Err(BastionError::ProviderUnavailable);
        }
        Ok(response
            .body
            .get("XPACKAGE_VERSION")
            .or_else(|| response.body.get("version"))
            .and_then(Value::as_str)
            .map(str::to_string))
    }

    /// Validates Access Key / Bearer by fetching the current user profile.
    pub async fn get_current_user(
        &self,
        auth: &JumpServerApiAuth,
    ) -> Result<JumpServerUser, BastionError> {
        // Prefer profile (works for non-admin keys). Official docs also demo `/users/users/`.
        let mut saw_unauthorized = false;
        let mut last_status = 0u16;
        for path in [
            format!("{}/users/profile/", self.base_path),
            format!("{}/users/profile", self.base_path),
            format!("{}/users/users/?limit=1", self.base_path),
        ] {
            let response = self
                .http
                .request(
                    "GET",
                    &path,
                    Some(auth.to_request_auth()),
                    auth.org_id(),
                    None,
                )
                .await?;
            last_status = response.status;
            let body_kind = if response.body.is_null() {
                "null"
            } else if response.body.is_object() {
                "object"
            } else if response.body.is_array() {
                "array"
            } else {
                "other"
            };
            let keys = response
                .body
                .as_object()
                .map(|object| object.keys().take(8).cloned().collect::<Vec<_>>().join(","))
                .unwrap_or_default();
            let hint = safe_api_error_hint(&response.body).unwrap_or_default();
            eprintln!(
                "[runory jumpserver] current-user probe path={path} status={} body={body_kind} keys=[{keys}] hint={hint}",
                response.status
            );
            tracing::info!(
                path = %path,
                status = response.status,
                body_kind,
                "jumpserver current-user probe"
            );
            if response.status == 401 || response.status == 403 {
                saw_unauthorized = true;
                continue;
            }
            if response.status == 404 {
                continue;
            }
            if response.status >= 400 {
                return Err(BastionError::ProviderProtocolError);
            }
            // Empty/non-JSON body is not a successful profile payload (often an HTML login page).
            if response.body.is_null() {
                eprintln!(
                    "[runory jumpserver] current-user empty/non-json body path={path} status={}",
                    response.status
                );
                continue;
            }
            // HTTP 2xx + JSON means the Access Key / token signature was accepted.
            // Do not treat an unfamiliar profile JSON shape as authentication failure.
            if let Some(user) = parse_user(&response.body).or_else(|| {
                parse_results_array(&response.body)
                    .into_iter()
                    .find_map(|value| parse_user(&value))
            }) {
                return Ok(user);
            }
            let fallback =
                fallback_user_from_body(&response.body).unwrap_or_else(|| JumpServerUser {
                    id: None,
                    username: "jumpserver-user".into(),
                    name: None,
                });
            eprintln!(
                "[runory jumpserver] current-user parse fallback username={}",
                fallback.username
            );
            return Ok(fallback);
        }
        eprintln!(
            "[runory jumpserver] current-user exhausted unauthorized={saw_unauthorized} last_status={last_status}"
        );
        Err(if saw_unauthorized {
            BastionError::AuthenticationFailed
        } else {
            BastionError::ProviderProtocolError
        })
    }

    pub async fn login(
        &self,
        username: &str,
        password: &str,
    ) -> Result<JumpServerAuthOutcome, BastionError> {
        self.login_with_session(username, password, None).await
    }

    pub async fn login_with_session(
        &self,
        username: &str,
        password: &str,
        session_id: Option<&str>,
    ) -> Result<JumpServerAuthOutcome, BastionError> {
        let auth = session_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| JumpServerRequestAuth::Cookie(format!("jms_sessionid={value}")));
        let response = self
            .http
            .request(
                "POST",
                &format!("{}/authentication/auth/", self.base_path),
                auth,
                None,
                Some(serde_json::json!({
                    "username": username,
                    "password": password,
                })),
            )
            .await?;
        parse_auth_response(response)
    }

    /// Completes OTP using the MFA URL from the login challenge, then re-auths for a Bearer token.
    ///
    /// JumpServer keeps MFA state in `jms_sessionid`; Bearer is issued only after a follow-up
    /// `/authentication/auth/` call on the same session (see JumpServer gist LeeEirc/dfe40838).
    pub async fn complete_otp_login(
        &self,
        username: &str,
        password: &str,
        session_id: &str,
        mfa_path: &str,
        code: &str,
    ) -> Result<JumpServerToken, BastionError> {
        let session_id = session_id.trim();
        if session_id.is_empty() || code.trim().is_empty() {
            return Err(BastionError::AuthenticationFailed);
        }
        let path = normalize_api_path(mfa_path, &self.base_path);
        let cookie = JumpServerRequestAuth::Cookie(format!("jms_sessionid={session_id}"));
        let verify = self
            .http
            .request(
                "POST",
                &path,
                Some(cookie),
                None,
                Some(serde_json::json!({
                    "code": code.trim(),
                    "type": "otp",
                })),
            )
            .await?;
        if verify.status >= 400 || looks_like_auth_failure(&verify.body) {
            return Err(BastionError::AuthenticationFailed);
        }
        match self
            .login_with_session(username, password, Some(session_id))
            .await?
        {
            JumpServerAuthOutcome::Authenticated(token) => Ok(token),
            JumpServerAuthOutcome::MfaRequired { .. } => Err(BastionError::MfaRequired),
        }
    }

    pub async fn list_assets(
        &self,
        auth: &JumpServerApiAuth,
        search: Option<&str>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<JumpServerAssetDto>, bool), BastionError> {
        let mut path = format!(
            "{}/perms/users/self/assets/?limit={limit}&offset={offset}",
            self.base_path
        );
        if let Some(search) = search.filter(|value| !value.is_empty()) {
            path.push_str(&format!("&search={}", urlencoding_lite(search)));
        }
        let response = self
            .http
            .request(
                "GET",
                &path,
                Some(auth.to_request_auth()),
                auth.org_id(),
                None,
            )
            .await?;
        if response.status == 401 || response.status == 403 {
            return Err(BastionError::AuthenticationExpired);
        }
        if response.status >= 400 {
            return Err(BastionError::ProviderProtocolError);
        }
        let items = parse_results_array(&response.body)
            .into_iter()
            .filter_map(parse_asset)
            .collect::<Vec<_>>();
        let has_more = response.body.get("next").and_then(Value::as_str).is_some();
        Ok((items, has_more))
    }

    pub async fn list_accounts(
        &self,
        auth: &JumpServerApiAuth,
        asset_id: &str,
    ) -> Result<Vec<JumpServerAccountDto>, BastionError> {
        let path = format!(
            "{}/perms/users/self/assets/{asset_id}/accounts/",
            self.base_path
        );
        let response = self
            .http
            .request(
                "GET",
                &path,
                Some(auth.to_request_auth()),
                auth.org_id(),
                None,
            )
            .await?;
        if response.status == 401 {
            return Err(BastionError::AuthenticationExpired);
        }
        if response.status == 200 {
            let accounts = parse_results_array(&response.body)
                .into_iter()
                .filter_map(parse_account)
                .collect::<Vec<_>>();
            if !accounts.is_empty() {
                return Ok(accounts);
            }
        }
        // Newer JumpServer UIs expose accounts on the asset detail as `permed_accounts`.
        self.list_accounts_from_asset_detail(auth, asset_id).await
    }

    async fn list_accounts_from_asset_detail(
        &self,
        auth: &JumpServerApiAuth,
        asset_id: &str,
    ) -> Result<Vec<JumpServerAccountDto>, BastionError> {
        let path = format!("{}/perms/users/self/assets/{asset_id}/", self.base_path);
        let response = self
            .http
            .request(
                "GET",
                &path,
                Some(auth.to_request_auth()),
                auth.org_id(),
                None,
            )
            .await?;
        if response.status == 401 {
            return Err(BastionError::AuthenticationExpired);
        }
        if response.status == 403 {
            return Err(BastionError::PermissionDenied);
        }
        if response.status == 404 {
            return Err(BastionError::AssetNotFound);
        }
        if response.status >= 400 {
            return Err(BastionError::ProviderProtocolError);
        }
        let accounts = response
            .body
            .get("permed_accounts")
            .or_else(|| response.body.get("accounts"))
            .cloned()
            .map(|value| {
                if let Some(array) = value.as_array() {
                    array.clone()
                } else {
                    Vec::new()
                }
            })
            .unwrap_or_default()
            .into_iter()
            .filter_map(parse_account)
            .collect::<Vec<_>>();
        Ok(accounts)
    }

    pub async fn create_connection_token(
        &self,
        auth: &JumpServerApiAuth,
        asset_id: &str,
        account: &str,
        protocol: &str,
    ) -> Result<JumpServerConnectionToken, BastionError> {
        self.create_connection_token_for_accounts(auth, asset_id, &[account.to_string()], protocol)
            .await
    }

    /// Tries each account alias until JumpServer accepts one.
    ///
    /// JumpServer looks up `account` by `Account.alias` (name for normal accounts, `@…` for
    /// special accounts). Callers should prefer alias, then name, then username.
    pub async fn create_connection_token_for_accounts(
        &self,
        auth: &JumpServerApiAuth,
        asset_id: &str,
        accounts: &[String],
        protocol: &str,
    ) -> Result<JumpServerConnectionToken, BastionError> {
        // `ssh_guide` matches OpenSSH / third-party clients (temporary JMS- credentials).
        // `ssh_client` is the Luna "Client" launch method (jms://). Try both.
        let mut last_error = BastionError::SessionRejected;
        for account in accounts
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            for connect_method in ["ssh_guide", "ssh_client"] {
                match self
                    .create_connection_token_with_method(
                        auth,
                        asset_id,
                        account,
                        protocol,
                        connect_method,
                    )
                    .await
                {
                    Ok(token) => return Ok(token),
                    Err(BastionError::AuthenticationExpired) => {
                        return Err(BastionError::AuthenticationExpired);
                    }
                    Err(BastionError::PermissionDenied) => {
                        return Err(BastionError::PermissionDenied);
                    }
                    Err(error) => {
                        tracing::warn!(
                            connect_method,
                            asset_id = %asset_id,
                            account = %account,
                            "jumpserver connection-token method rejected"
                        );
                        last_error = error;
                    }
                }
            }
        }
        Err(last_error)
    }

    async fn create_connection_token_with_method(
        &self,
        auth: &JumpServerApiAuth,
        asset_id: &str,
        account: &str,
        protocol: &str,
        connect_method: &str,
    ) -> Result<JumpServerConnectionToken, BastionError> {
        let account = account.trim();
        if asset_id.trim().is_empty() || account.is_empty() {
            return Err(BastionError::SessionRejected);
        }
        // JumpServer expects account alias (`name` for normal accounts, `@USER` / …), not UUID.
        // `input_username` is only meaningful for `@INPUT` / `@USER` virtual accounts and must be
        // the real login name — callers that need it should pass a separate field later.
        let body = serde_json::json!({
            "asset": asset_id,
            "account": account,
            "protocol": protocol,
            "connect_method": connect_method,
        });
        let response = self
            .http
            .request(
                "POST",
                &format!("{}/authentication/connection-token/", self.base_path),
                Some(auth.to_request_auth()),
                auth.org_id(),
                Some(body),
            )
            .await?;
        if response.status == 401 {
            return Err(BastionError::AuthenticationExpired);
        }
        if response.status == 403 {
            return Err(BastionError::PermissionDenied);
        }
        if response.status >= 400 {
            return Err(BastionError::SessionRejected);
        }
        let id = response
            .body
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or(BastionError::ProviderProtocolError)?
            .to_string();
        let value = response
            .body
            .get("value")
            .or_else(|| response.body.get("token"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or(BastionError::ProviderProtocolError)?
            .to_string();
        Ok(JumpServerConnectionToken { id, value })
    }

    /// Resolves the smart KoKo endpoint JumpServer would hand to native clients.
    ///
    /// See `GET /authentication/connection-token/{id}/client-url/` and Components → Endpoint.
    pub async fn fetch_token_client_endpoint(
        &self,
        auth: &JumpServerApiAuth,
        token_id: &str,
    ) -> Result<Option<(String, u16)>, BastionError> {
        let token_id = token_id.trim();
        if token_id.is_empty() {
            return Ok(None);
        }
        let response = self
            .http
            .request(
                "GET",
                &format!(
                    "{}/authentication/connection-token/{token_id}/client-url/",
                    self.base_path
                ),
                Some(auth.to_request_auth()),
                auth.org_id(),
                None,
            )
            .await?;
        if response.status == 401 {
            return Err(BastionError::AuthenticationExpired);
        }
        if response.status >= 400 {
            tracing::warn!(
                status = response.status,
                token_id = %token_id,
                "jumpserver client-url unavailable"
            );
            return Ok(None);
        }
        Ok(parse_client_url_endpoint(&response.body))
    }

    /// JumpServer smart terminal endpoint (`host` + `ssh_port`).
    ///
    /// Matches jumpserver-client: `GET /api/v1/terminal/endpoints/smart/?protocol=ssh`.
    pub async fn fetch_smart_ssh_endpoint(
        &self,
        auth: &JumpServerApiAuth,
    ) -> Result<Option<(String, u16)>, BastionError> {
        let response = self
            .http
            .request(
                "GET",
                &format!("{}/terminal/endpoints/smart/?protocol=ssh", self.base_path),
                Some(auth.to_request_auth()),
                auth.org_id(),
                None,
            )
            .await?;
        if response.status == 401 {
            return Err(BastionError::AuthenticationExpired);
        }
        if response.status >= 400 {
            tracing::warn!(
                status = response.status,
                "jumpserver smart endpoint unavailable"
            );
            return Ok(None);
        }
        Ok(parse_smart_ssh_endpoint(&response.body))
    }
}

fn parse_smart_ssh_endpoint(body: &Value) -> Option<(String, u16)> {
    let host = body
        .get("host")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let port = body
        .get("ssh_port")
        .or_else(|| body.get("port"))
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .and_then(|value| u16::try_from(value).ok())
        .filter(|port| *port > 0)
        .unwrap_or(2222);
    Some((host, port))
}

fn parse_client_url_endpoint(body: &Value) -> Option<(String, u16)> {
    let url = body.get("url").and_then(Value::as_str)?;
    let encoded = url
        .strip_prefix("jms://")
        .or_else(|| url.strip_prefix("JMS://"))?
        .trim();
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(encoded))
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(encoded))
        .ok()?;
    let payload: Value = serde_json::from_slice(&bytes).ok()?;
    let endpoint = payload.get("endpoint")?;
    let host = endpoint
        .get("host")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let port = endpoint
        .get("port")
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .and_then(|value| u16::try_from(value).ok())
        .filter(|port| *port > 0)?;
    Some((host, port))
}

#[derive(Clone, Debug)]
pub enum JumpServerAuthOutcome {
    Authenticated(JumpServerToken),
    MfaRequired {
        session_id: String,
        mfa_path: String,
        message: String,
    },
}

#[derive(Clone, Debug)]
pub struct JumpServerUser {
    pub id: Option<String>,
    pub username: String,
    pub name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct JumpServerAssetDto {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub platform: Option<String>,
    pub protocols: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct JumpServerAccountDto {
    pub id: Option<String>,
    pub username: String,
    pub name: Option<String>,
    /// JumpServer `Account.alias`: `@…` usernames keep username; otherwise `name`.
    /// Connection-token `account` must match this key (`PermAssetDetailUtil.validate_permission`).
    pub alias: String,
    pub privileged: bool,
}

fn parse_auth_response(
    response: JumpServerHttpResponse,
) -> Result<JumpServerAuthOutcome, BastionError> {
    let session_id = extract_session_id(&response);
    let error = response
        .body
        .get("error")
        .or_else(|| response.body.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("");

    if error.eq_ignore_ascii_case("mfa_required") || looks_like_mfa(&response.body) {
        let session_id = session_id.ok_or(BastionError::AuthenticationFailed)?;
        let mfa_path = response
            .body
            .get("data")
            .and_then(|data| data.get("url"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "/api/v1/authentication/mfa/verify/".into());
        return Ok(JumpServerAuthOutcome::MfaRequired {
            session_id,
            mfa_path,
            message: response
                .body
                .get("msg")
                .and_then(Value::as_str)
                .unwrap_or("Enter the one-time password")
                .to_string(),
        });
    }

    if response.status >= 400 {
        return Err(BastionError::AuthenticationFailed);
    }

    let token = response
        .body
        .get("token")
        .or_else(|| response.body.get("access_token"))
        .and_then(Value::as_str)
        .ok_or(BastionError::AuthenticationFailed)?
        .to_string();
    Ok(JumpServerAuthOutcome::Authenticated(JumpServerToken {
        token,
    }))
}

fn extract_session_id(response: &JumpServerHttpResponse) -> Option<String> {
    for (name, value) in &response.cookies {
        if name.eq_ignore_ascii_case("jms_sessionid") || name.eq_ignore_ascii_case("sessionid") {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    response
        .body
        .get("token")
        .or_else(|| response.body.get("data").and_then(|data| data.get("token")))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn looks_like_mfa(body: &Value) -> bool {
    body.get("error")
        .and_then(Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case("mfa_required"))
        || body.get("code").and_then(Value::as_str) == Some("mfa_required")
}

fn looks_like_auth_failure(body: &Value) -> bool {
    let error = body
        .get("error")
        .or_else(|| body.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    error.contains("auth")
        || error.contains("mfa")
        || error.contains("otp")
        || error == "not_authenticated"
        || body
            .get("detail")
            .and_then(Value::as_str)
            .is_some_and(|detail| {
                detail.to_lowercase().contains("credential")
                    || detail.to_lowercase().contains("authenticat")
            })
}

fn normalize_api_path(path: &str, base_path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        // Absolute URL is unexpected for in-app calls; fall back to verify path.
        return format!("{base_path}/authentication/mfa/verify/");
    }
    if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn parse_results_array(body: &Value) -> Vec<Value> {
    if let Some(results) = body.get("results").and_then(Value::as_array) {
        return results.clone();
    }
    body.as_array().cloned().unwrap_or_default()
}

fn parse_user(value: &Value) -> Option<JumpServerUser> {
    let source = value
        .get("user")
        .or_else(|| value.get("data"))
        .filter(|nested| nested.is_object())
        .unwrap_or(value);
    let username = source
        .get("username")
        .or_else(|| source.get("name"))
        .or_else(|| source.get("email"))
        .and_then(|field| {
            field
                .as_str()
                .map(str::to_string)
                .or_else(|| field.as_i64().map(|n| n.to_string()))
        })
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    Some(JumpServerUser {
        id: source.get("id").and_then(|id| {
            id.as_str()
                .map(str::to_string)
                .or_else(|| id.as_i64().map(|n| n.to_string()))
        }),
        username: username.clone(),
        name: source
            .get("name")
            .or_else(|| source.get("display_name"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|value| value != &username),
    })
}

fn fallback_user_from_body(value: &Value) -> Option<JumpServerUser> {
    let source = value
        .get("user")
        .or_else(|| value.get("data"))
        .filter(|nested| nested.is_object())
        .unwrap_or(value);
    let id = source.get("id").and_then(|id| {
        id.as_str()
            .map(str::to_string)
            .or_else(|| id.as_i64().map(|n| n.to_string()))
    })?;
    Some(JumpServerUser {
        id: Some(id.clone()),
        username: id,
        name: source
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn parse_asset(value: Value) -> Option<JumpServerAssetDto> {
    let id = value.get("id")?.as_str()?.to_string();
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(id.as_str())
        .to_string();
    let protocols = value
        .get("protocols")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.as_str()
                        .map(str::to_string)
                        .or_else(|| item.get("name").and_then(Value::as_str).map(str::to_string))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(JumpServerAssetDto {
        id,
        name,
        address: value
            .get("address")
            .or_else(|| value.get("ip"))
            .and_then(Value::as_str)
            .map(str::to_string),
        platform: value.get("platform").and_then(|platform| {
            platform.as_str().map(str::to_string).or_else(|| {
                platform
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
        }),
        protocols,
    })
}

fn parse_account(value: Value) -> Option<JumpServerAccountDto> {
    let username = value
        .get("username")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let name = value
        .get("name")
        .or_else(|| value.get("label"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let alias_from_api = value
        .get("alias")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    // Prefer real OS username when present; otherwise fall back to name/alias for virtual accounts.
    let username = username
        .or_else(|| name.clone())
        .or_else(|| alias_from_api.clone())?;

    // Mirror JumpServer Account.alias: special @accounts use username; others use name.
    let alias = alias_from_api.unwrap_or_else(|| {
        if username.starts_with('@') {
            username.clone()
        } else {
            name.clone().unwrap_or_else(|| username.clone())
        }
    });

    Some(JumpServerAccountDto {
        id: value
            .get("id")
            .and_then(|id| {
                id.as_str()
                    .map(str::to_string)
                    .or_else(|| id.as_i64().map(|n| n.to_string()))
            })
            .or_else(|| {
                value
                    .get("account")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
        username: username.clone(),
        name: name.or(Some(username)),
        alias,
        privileged: value
            .get("privileged")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn urlencoding_lite(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn parse_set_cookie_pairs(header_value: &str) -> Option<(String, String)> {
    let first = header_value.split(';').next()?.trim();
    let (name, value) = first.split_once('=')?;
    let name = name.trim();
    let value = value.trim();
    if name.is_empty() || value.is_empty() {
        return None;
    }
    Some((name.to_string(), value.to_string()))
}

/// Production reqwest-backed HTTP client for JumpServer Core API.
pub struct ReqwestJumpServerHttp {
    client: reqwest::Client,
    base_url: String,
}

impl ReqwestJumpServerHttp {
    pub fn new(base_url: impl Into<String>) -> Result<Self, BastionError> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err(BastionError::ProviderUnavailable);
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            // Signed Access Key requests must not follow redirects: a hop changes the
            // path/host and JumpServer signature verification uses get_full_path().
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| BastionError::Network)?;
        Ok(Self { client, base_url })
    }
}

fn safe_api_error_hint(body: &Value) -> Option<String> {
    body.get("detail")
        .or_else(|| body.get("msg"))
        .or_else(|| body.get("error"))
        .or_else(|| body.get("message"))
        .and_then(|value| {
            value.as_str().map(str::to_string).or_else(|| {
                if value.is_object() || value.is_array() {
                    Some(value.to_string())
                } else {
                    None
                }
            })
        })
        .map(|text| {
            let trimmed = text.trim();
            if trimmed.len() > 180 {
                format!("{}…", &trimmed[..180])
            } else {
                trimmed.to_string()
            }
        })
        .filter(|text| {
            let lower = text.to_ascii_lowercase();
            !lower.contains("signature=")
                && !lower.contains("authorization")
                && !lower.contains("secret")
                && !lower.contains("password")
                && !lower.contains("token")
        })
}

#[async_trait::async_trait]
impl JumpServerHttp for ReqwestJumpServerHttp {
    async fn request(
        &self,
        method: &str,
        path: &str,
        auth: Option<JumpServerRequestAuth>,
        org_id: Option<&str>,
        body: Option<Value>,
    ) -> Result<JumpServerHttpResponse, BastionError> {
        let url = format!("{}{}", self.base_url, path);
        let mut builder = match method {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            "PATCH" => self.client.patch(&url),
            "DELETE" => self.client.delete(&url),
            _ => return Err(BastionError::Internal),
        };
        builder = builder.header("Accept", "application/json");
        // JumpServer Access Key / org-scoped APIs expect X-JMS-ORG (Default org when unset).
        let effective_org = org_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or(Some(super::auth::DEFAULT_JMS_ORG_ID));
        if let Some(org_id) = effective_org {
            builder = builder.header("X-JMS-ORG", org_id);
        }
        let auth_kind = match &auth {
            Some(JumpServerRequestAuth::Bearer(_)) => "bearer",
            Some(JumpServerRequestAuth::Cookie(_)) => "cookie",
            Some(JumpServerRequestAuth::AccessKey { .. }) => "access_key",
            Some(JumpServerRequestAuth::PrivateToken(_)) => "private_token",
            None => "none",
        };
        match auth {
            Some(JumpServerRequestAuth::Bearer(token)) => {
                builder = builder.bearer_auth(token);
            }
            Some(JumpServerRequestAuth::Cookie(cookie)) => {
                builder = builder.header(reqwest::header::COOKIE, cookie);
            }
            Some(JumpServerRequestAuth::AccessKey { key_id, secret }) => {
                let signer = JumpServerSigner::new(key_id.trim(), secret.trim());
                let signed = signer.sign(method, path, None);
                builder = builder
                    .header("Accept", signed.accept)
                    .header("Date", signed.date)
                    .header(reqwest::header::AUTHORIZATION, signed.authorization);
            }
            Some(JumpServerRequestAuth::PrivateToken(token)) => {
                // JumpServer docs use PrivateToken; DRF TokenAuthentication uses Token.
                builder = builder.header(
                    reqwest::header::AUTHORIZATION,
                    format!("Token {}", token.trim()),
                );
            }
            None => {}
        }
        if body.is_some() {
            builder = builder.header("Content-Type", "application/json");
        }
        if let Some(body) = body {
            builder = builder.json(&body);
        }
        let response = builder.send().await.map_err(|error| {
            eprintln!(
                "[runory jumpserver] http FAILED network method={method} path={path} auth={auth_kind} err={error}"
            );
            BastionError::Network
        })?;
        let status = response.status().as_u16();
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");
            eprintln!(
                "[runory jumpserver] http redirect blocked status={status} method={method} path={path} location={location}"
            );
            return Err(BastionError::ProviderProtocolError);
        }
        let mut cookies = Vec::new();
        for value in response.headers().get_all(reqwest::header::SET_COOKIE) {
            if let Ok(raw) = value.to_str() {
                if let Some(pair) = parse_set_cookie_pairs(raw) {
                    cookies.push(pair);
                }
            }
        }
        let body = response
            .json::<Value>()
            .await
            .unwrap_or_else(|_| Value::Null);
        if status >= 400 {
            let hint = safe_api_error_hint(&body).unwrap_or_default();
            eprintln!(
                "[runory jumpserver] http status={status} method={method} path={path} auth={auth_kind} hint={hint}"
            );
        }
        Ok(JumpServerHttpResponse {
            status,
            body,
            cookies,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct ScriptedHttp {
        calls: Mutex<Vec<(String, String, Option<String>)>>,
        responses: Mutex<Vec<JumpServerHttpResponse>>,
    }

    #[async_trait::async_trait]
    impl JumpServerHttp for ScriptedHttp {
        async fn request(
            &self,
            method: &str,
            path: &str,
            auth: Option<JumpServerRequestAuth>,
            _org_id: Option<&str>,
            _body: Option<Value>,
        ) -> Result<JumpServerHttpResponse, BastionError> {
            let auth_label = match auth {
                Some(JumpServerRequestAuth::Bearer(_)) => Some("bearer".into()),
                Some(JumpServerRequestAuth::Cookie(cookie)) => Some(cookie),
                Some(JumpServerRequestAuth::AccessKey { key_id, .. }) => {
                    Some(format!("access-key:{key_id}"))
                }
                Some(JumpServerRequestAuth::PrivateToken(_)) => Some("private-token".into()),
                None => None,
            };
            self.calls
                .lock()
                .expect("lock")
                .push((method.into(), path.into(), auth_label));
            self.responses
                .lock()
                .expect("lock")
                .pop()
                .ok_or(BastionError::ProviderUnavailable)
        }
    }

    #[tokio::test]
    async fn access_key_auth_accepts_profile_without_username_field() {
        let http = ScriptedHttp {
            calls: Mutex::new(Vec::new()),
            responses: Mutex::new(vec![JumpServerHttpResponse {
                status: 200,
                body: serde_json::json!({
                    "id": "u-42",
                    "email": "ops@example.com"
                }),
                cookies: vec![],
            }]),
        };
        let client = JumpServerApiClient::new(http);
        let auth = JumpServerApiAuth::AccessKey {
            key_id: "AKTEST".into(),
            secret: "secret".into(),
            org_id: None,
        };
        let user = client.get_current_user(&auth).await.expect("user");
        assert_eq!(user.username, "ops@example.com");
    }

    #[tokio::test]
    async fn login_maps_mfa_challenge_from_session_cookie() {
        let http = ScriptedHttp {
            calls: Mutex::new(Vec::new()),
            responses: Mutex::new(vec![JumpServerHttpResponse {
                status: 200,
                body: serde_json::json!({
                    "error": "mfa_required",
                    "msg": "MFA required",
                    "data": {
                        "choices": ["otp"],
                        "url": "/api/v1/authentication/mfa/challenge/"
                    }
                }),
                cookies: vec![("jms_sessionid".into(), "sess-abc".into())],
            }]),
        };
        let client = JumpServerApiClient::new(http);
        let outcome = client.login("alice", "secret").await.expect("login");
        match outcome {
            JumpServerAuthOutcome::MfaRequired {
                session_id,
                mfa_path,
                ..
            } => {
                assert_eq!(session_id, "sess-abc");
                assert_eq!(mfa_path, "/api/v1/authentication/mfa/challenge/");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn complete_otp_uses_cookie_then_reauths_for_bearer() {
        let http = ScriptedHttp {
            calls: Mutex::new(Vec::new()),
            // pop() = LIFO: last pushed is first returned
            responses: Mutex::new(vec![
                JumpServerHttpResponse {
                    status: 200,
                    body: serde_json::json!({ "token": "bearer-xyz" }),
                    cookies: vec![],
                },
                JumpServerHttpResponse {
                    status: 200,
                    body: serde_json::json!({ "msg": "ok" }),
                    cookies: vec![],
                },
            ]),
        };
        let client = JumpServerApiClient::new(http);
        let token = client
            .complete_otp_login(
                "alice",
                "secret",
                "sess-abc",
                "/api/v1/authentication/mfa/challenge/",
                "123456",
            )
            .await
            .expect("otp");
        assert_eq!(token.token, "bearer-xyz");
        let calls = client.http.calls.lock().expect("lock");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].1, "/api/v1/authentication/mfa/challenge/");
        assert_eq!(calls[0].2.as_deref(), Some("jms_sessionid=sess-abc"));
        assert_eq!(calls[1].1, "/api/v1/authentication/auth/");
        assert_eq!(calls[1].2.as_deref(), Some("jms_sessionid=sess-abc"));
    }

    #[test]
    fn parses_jms_client_url_endpoint() {
        use base64::Engine;
        let payload = serde_json::json!({
            "endpoint": { "host": "koko.example.com", "port": 2222 },
            "token": { "id": "tok-1", "value": "secret" }
        });
        let encoded = base64::engine::general_purpose::STANDARD
            .encode(serde_json::to_vec(&payload).expect("json"));
        let body = serde_json::json!({ "url": format!("jms://{encoded}") });
        assert_eq!(
            parse_client_url_endpoint(&body),
            Some(("koko.example.com".into(), 2222))
        );
    }

    #[test]
    fn parses_smart_ssh_endpoint() {
        assert_eq!(
            parse_smart_ssh_endpoint(&serde_json::json!({
                "host": "jms.zzliaoyuan.com",
                "ssh_port": 2222
            })),
            Some(("jms.zzliaoyuan.com".into(), 2222))
        );
        assert_eq!(
            parse_smart_ssh_endpoint(&serde_json::json!({
                "host": "koko.internal",
                "port": 3022
            })),
            Some(("koko.internal".into(), 3022))
        );
    }

    #[test]
    fn parse_account_prefers_jumpserver_alias_over_username() {
        let account = parse_account(serde_json::json!({
            "id": "acc-1",
            "alias": "prod-yadz",
            "name": "prod-yadz",
            "username": "yadz",
            "privileged": false
        }))
        .expect("account");
        assert_eq!(account.username, "yadz");
        assert_eq!(account.name.as_deref(), Some("prod-yadz"));
        assert_eq!(account.alias, "prod-yadz");
    }

    #[test]
    fn parse_account_derives_alias_from_name_when_api_omits_it() {
        let account = parse_account(serde_json::json!({
            "id": "acc-2",
            "name": "deploy-key",
            "username": "deploy"
        }))
        .expect("account");
        assert_eq!(account.alias, "deploy-key");
        assert_eq!(account.username, "deploy");
    }

    #[test]
    fn parse_account_keeps_special_alias_username() {
        let account = parse_account(serde_json::json!({
            "alias": "@USER",
            "username": "@USER",
            "name": "Dynamic user"
        }))
        .expect("account");
        assert_eq!(account.alias, "@USER");
        assert_eq!(account.username, "@USER");
    }
}
