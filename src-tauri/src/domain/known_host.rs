use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownHost {
    #[serde(default = "direct_route_scope")]
    pub route_scope: String,
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostVerificationStatus {
    Unknown,
    Trusted,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostVerification {
    pub attempt_id: Uuid,
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
    pub status: HostVerificationStatus,
    pub route_scope: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareHostVerificationRequest {
    pub profile_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustHostRequest {
    pub attempt_id: Uuid,
    pub remember: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelHostVerificationRequest {
    pub attempt_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveKnownHostRequest {
    #[serde(default = "direct_route_scope")]
    pub route_scope: String,
    pub host: String,
    pub port: u16,
}

fn direct_route_scope() -> String {
    "direct".to_owned()
}
