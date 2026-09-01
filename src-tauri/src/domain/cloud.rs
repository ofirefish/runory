use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AuthMethod;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudGroup {
    pub id: Uuid,
    pub name: String,
    pub sort_order: i32,
    pub collapsed: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudProfile {
    pub id: Uuid,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub group_id: Option<Uuid>,
    pub auth_method: AuthMethod,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudEncryptedPayload {
    pub version: u8,
    pub salt: Vec<u8>,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudExportRequest {
    pub organization_id: Uuid,
    pub passphrase: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudImportRequest {
    pub organization_id: Uuid,
    pub passphrase: String,
    pub payload: CloudEncryptedPayload,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudApplyRequest {
    pub import_id: Uuid,
    #[serde(default)]
    pub decisions: Vec<CloudConflictDecision>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloudObjectKind {
    Group,
    Profile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloudConflictResolution {
    KeepLocal,
    UseRemote,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudConflictDecision {
    pub kind: CloudObjectKind,
    pub id: Uuid,
    pub expected_local_updated_at: String,
    pub resolution: CloudConflictResolution,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudConflictItem {
    pub kind: CloudObjectKind,
    pub id: Uuid,
    pub label: String,
    pub local_updated_at: String,
    pub remote_updated_at: String,
    pub remote_deleted: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudDiscardRequest {
    pub import_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudImportPreview {
    pub import_id: Uuid,
    pub group_additions: usize,
    pub group_updates: usize,
    pub profile_additions: usize,
    pub profile_updates: usize,
    pub group_deletions: usize,
    pub profile_deletions: usize,
    pub local_newer: usize,
    pub conflicts: usize,
    pub conflict_items: Vec<CloudConflictItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudApplyResult {
    pub groups_applied: usize,
    pub profiles_applied: usize,
    pub groups_deleted: usize,
    pub profiles_deleted: usize,
    pub skipped: usize,
}
