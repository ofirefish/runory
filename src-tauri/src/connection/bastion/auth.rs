use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

/// Opaque reference into Runory's secret store. Never holds plaintext.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretRef {
    pub id: String,
}

/// Identity used to authenticate *to the bastion*, never the target host.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BastionCredential {
    Password {
        username: String,
        #[serde(rename = "passwordRef")]
        password_ref: SecretRef,
        /// One-shot secret for the current auth attempt. Never serialized / logged.
        #[serde(skip)]
        transient_password: Option<Zeroizing<String>>,
    },
    SshKey {
        username: String,
        #[serde(rename = "keyRef")]
        key_ref: SecretRef,
        #[serde(rename = "passphraseRef", skip_serializing_if = "Option::is_none")]
        passphrase_ref: Option<SecretRef>,
        #[serde(skip)]
        transient_passphrase: Option<Zeroizing<String>>,
    },
    Token {
        #[serde(rename = "tokenRef")]
        token_ref: SecretRef,
        #[serde(skip)]
        transient_token: Option<Zeroizing<String>>,
        /// Target SSH username hint (Boundary/Teleport). Not the bastion control-plane identity.
        #[serde(skip)]
        username_hint: Option<String>,
    },
    /// JumpServer Access Key (control-plane). Secret never serializes.
    AccessKey {
        #[serde(rename = "keyId")]
        key_id: String,
        #[serde(rename = "secretRef")]
        secret_ref: SecretRef,
        #[serde(skip)]
        transient_secret: Option<Zeroizing<String>>,
    },
    BrowserSso {
        #[serde(rename = "usernameHint", skip_serializing_if = "Option::is_none")]
        username_hint: Option<String>,
    },
    ExternalAgent {
        username: String,
    },
}

impl BastionCredential {
    pub fn password(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self::Password {
            username: username.into(),
            password_ref: SecretRef {
                id: "transient:session".into(),
            },
            transient_password: Some(Zeroizing::new(password.into())),
        }
    }

    pub fn access_key(key_id: impl Into<String>, secret: impl Into<String>) -> Self {
        Self::AccessKey {
            key_id: key_id.into(),
            secret_ref: SecretRef {
                id: "transient:session".into(),
            },
            transient_secret: Some(Zeroizing::new(secret.into())),
        }
    }

    pub fn browser_sso(username_hint: Option<String>) -> Self {
        Self::BrowserSso { username_hint }
    }

    pub fn token(token: impl Into<String>) -> Self {
        Self::token_with_username(token, None)
    }

    pub fn token_with_username(
        token: impl Into<String>,
        username_hint: Option<String>,
    ) -> Self {
        Self::Token {
            token_ref: SecretRef {
                id: "transient:session".into(),
            },
            transient_token: Some(Zeroizing::new(token.into())),
            username_hint,
        }
    }
}

/// Authenticated principal on the bastion (no secrets).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BastionPrincipal {
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// Provider-private session state. Contents are never logged or serialized to UI.
#[derive(Clone, Default)]
pub struct ProtectedProviderState {
    bytes: Vec<u8>,
}

impl ProtectedProviderState {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl std::fmt::Debug for ProtectedProviderState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProtectedProviderState")
            .field("len", &self.bytes.len())
            .finish()
    }
}

/// Optional absolute path to vendor CLI from `providerConfig.cliPath`.
pub fn provider_cli_path(config: &serde_json::Value) -> Option<String> {
    config
        .get("cliPath")
        .or_else(|| config.get("cli_path"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[derive(Clone, Debug)]
pub struct AuthSession {
    pub id: Uuid,
    pub bastion_id: Uuid,
    pub provider: String,
    pub principal: BastionPrincipal,
    pub provider_state: ProtectedProviderState,
    /// Unix millis; None means no known expiry.
    pub expires_at: Option<i64>,
    /// Optional absolute path to vendor CLI (`tsh` / `boundary`) from profile config.
    pub helper_cli_override: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthChoice {
    pub id: String,
    pub label: String,
}

/// Interactive challenge returned to Runtime / UI. Providers never draw UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AuthChallenge {
    Password {
        id: String,
        message: String,
    },
    Totp {
        id: String,
        message: String,
    },
    SmsCode {
        id: String,
        message: String,
        #[serde(rename = "maskedTarget", skip_serializing_if = "Option::is_none")]
        masked_target: Option<String>,
    },
    Confirm {
        id: String,
        message: String,
    },
    Choice {
        id: String,
        message: String,
        choices: Vec<AuthChoice>,
    },
    Text {
        id: String,
        message: String,
        secret: bool,
    },
}

impl AuthChallenge {
    pub fn id(&self) -> &str {
        match self {
            Self::Password { id, .. }
            | Self::Totp { id, .. }
            | Self::SmsCode { id, .. }
            | Self::Confirm { id, .. }
            | Self::Choice { id, .. }
            | Self::Text { id, .. } => id,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ExternalAuthAction {
    OpenBrowser {
        url: String,
        #[serde(rename = "callbackUri", skip_serializing_if = "Option::is_none")]
        callback_uri: Option<String>,
    },
    DeviceCode {
        #[serde(rename = "verificationUri")]
        verification_uri: String,
        #[serde(rename = "userCode")]
        user_code: String,
        #[serde(rename = "expiresInSecs")]
        expires_in_secs: u64,
    },
    QrCode {
        payload: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AuthChallengeResponse {
    Password { id: String, password: String },
    Totp { id: String, code: String },
    SmsCode { id: String, code: String },
    Confirm { id: String, accepted: bool },
    Choice { id: String, #[serde(rename = "choiceId")] choice_id: String },
    Text { id: String, value: String },
    ExternalCompleted { id: String },
    Cancel { id: String },
}

impl AuthChallengeResponse {
    pub fn challenge_id(&self) -> &str {
        match self {
            Self::Password { id, .. }
            | Self::Totp { id, .. }
            | Self::SmsCode { id, .. }
            | Self::Confirm { id, .. }
            | Self::Choice { id, .. }
            | Self::Text { id, .. }
            | Self::ExternalCompleted { id }
            | Self::Cancel { id } => id,
        }
    }
}

#[derive(Debug)]
pub enum AuthStepResult {
    Authenticated(AuthSession),
    /// Pending auth session plus the challenge the UI/Runtime must present.
    Challenge {
        pending: AuthSession,
        challenge: AuthChallenge,
    },
    ExternalAction {
        pending: AuthSession,
        action: ExternalAuthAction,
    },
}

/// High-level bastion auth / discovery / connect lifecycle for Runtime.
#[derive(Clone, Debug, PartialEq)]
pub enum BastionSessionState {
    Idle,
    Probing,
    Authenticating,
    AwaitingUser { challenge: AuthChallenge },
    AwaitingExternalAuth { action: ExternalAuthAction },
    Authenticated,
    DiscoveringAssets,
    SelectingAsset,
    DiscoveringAccounts,
    SelectingAccount,
    Connecting,
    Connected,
    Disconnected,
    Failed { code: &'static str },
}
