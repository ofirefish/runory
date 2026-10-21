use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AuthMethod, ConnectionRoute};

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
    #[serde(default)]
    pub connection_route: ConnectionRoute,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudDeploymentApp {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub name: String,
    pub repository_path: String,
    pub remote_url: String,
    pub branch: String,
    pub build: crate::domain::BuildPreset,
    pub restart: crate::domain::RestartTarget,
    pub project_kind: crate::domain::ProjectKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub python: Option<crate::domain::PythonProjectConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub php: Option<crate::domain::PhpProjectConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java: Option<crate::domain::JavaProjectConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<crate::domain::NodeProjectConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub go: Option<crate::domain::GoProjectConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<crate::domain::HtmlProjectConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<crate::domain::ReverseProxyConfig>,
    pub setup_step: u8,
    pub setup_status: crate::domain::DeploymentSetupStatus,
    pub source_kind: crate::domain::DeploymentSourceKind,
    #[serde(default)]
    pub venv_path: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudKeyEnvelope {
    pub salt: Vec<u8>,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudEncryptedPayload {
    pub version: u8,
    pub salt: Vec<u8>,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_envelope: Option<CloudKeyEnvelope>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudExportRequest {
    pub organization_id: Uuid,
    #[serde(default)]
    pub recovery_passphrase: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudImportRequest {
    pub organization_id: Uuid,
    #[serde(default)]
    pub recovery_passphrase: Option<String>,
    pub payload: CloudEncryptedPayload,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncKeyRequest {
    pub organization_id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudRecoveryPassphraseRotateRequest {
    pub organization_id: Uuid,
    pub new_recovery_passphrase: String,
    pub payload: CloudEncryptedPayload,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncKeyStatus {
    pub configured: bool,
    pub persisted_on_device: bool,
    pub secure_storage_available: bool,
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
    DeploymentApp,
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
    #[serde(default)]
    pub deployment_app_additions: usize,
    #[serde(default)]
    pub deployment_app_updates: usize,
    #[serde(default)]
    pub deployment_app_deletions: usize,
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
    #[serde(default)]
    pub deployment_apps_applied: usize,
    #[serde(default)]
    pub deployment_apps_deleted: usize,
    pub skipped: usize,
}
