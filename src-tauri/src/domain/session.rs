use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::{CredentialInput, HostVerification};

pub type SessionId = Uuid;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionState {
    Connecting,
    VerifyingHost,
    Authenticating,
    OpeningShell,
    Connected,
    Disconnected,
    Error,
}

#[derive(Clone, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum TerminalEvent {
    Output { bytes: Vec<u8> },
    State { state: SessionState },
    Closed { reason: Option<String> },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyInfo {
    pub key_type: String,
    pub fingerprint: String,
}

pub struct ConnectRequest {
    pub connection: SshConnectionRequest,
    pub cols: u32,
    pub rows: u32,
}

pub struct SshConnectionRequest {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub authentication: SshAuthentication,
}

pub enum SshAuthentication {
    Password {
        password: Zeroizing<String>,
    },
    PrivateKeyFile {
        path: String,
        passphrase: Option<Zeroizing<String>>,
    },
    PrivateKeyData {
        content: Zeroizing<Vec<u8>>,
        passphrase: Option<Zeroizing<String>>,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectProfileRequest {
    pub verification_attempt_id: Uuid,
    pub profile_id: Uuid,
    pub credential: CredentialInput,
    #[serde(default)]
    pub jump_preparation_id: Option<Uuid>,
    pub cols: u32,
    pub rows: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionProfileRequest {
    pub verification_attempt_id: Uuid,
    pub profile_id: Uuid,
    pub credential: CredentialInput,
    #[serde(default)]
    pub jump_preparation_id: Option<Uuid>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareJumpConnectionRequest {
    pub target_profile_id: Uuid,
    pub jump_verification_attempt_id: Uuid,
    pub jump_credential: CredentialInput,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareJumpConnectionResponse {
    pub preparation_id: Uuid,
    pub target_verification: HostVerification,
    pub jump_credential_saved: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelJumpConnectionRequest {
    pub preparation_id: Uuid,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectResponse {
    pub session_id: SessionId,
    pub credential_saved: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionResponse {
    pub credential_saved: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteRequest {
    pub session_id: SessionId,
    pub data: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResizeRequest {
    pub session_id: SessionId,
    pub cols: u32,
    pub rows: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRequest {
    pub session_id: SessionId,
}
