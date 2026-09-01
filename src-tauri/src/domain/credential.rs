use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum CredentialInput {
    SessionOnly { secret: String },
    RememberSecurely { secret: String },
    Stored,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum CredentialKind {
    Password,
    KeyPassphrase,
    McpToken,
    LlmApiKey,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultUnlockRequest {
    pub master_password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatusRequest {
    pub profile_id: Option<Uuid>,
    pub kind: Option<CredentialKind>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    pub vault_initialized: bool,
    pub vault_unlocked: bool,
    pub has_credential: bool,
    pub platform_unlock_supported: bool,
    pub platform_unlock_available: bool,
    pub platform_unlock_configured: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgetCredentialRequest {
    pub profile_id: Uuid,
    pub kind: CredentialKind,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateKeyRequest {
    pub key_id: Uuid,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateKeyImport {
    pub key_id: Uuid,
    pub name: String,
}
