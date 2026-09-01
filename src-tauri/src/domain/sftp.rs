use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::SessionId;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SftpEntryKind {
    Directory,
    File,
    Symlink,
    Other,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpEntry {
    pub name: String,
    pub path: String,
    pub kind: SftpEntryKind,
    pub size: Option<u64>,
    pub modified: Option<u32>,
    pub permissions: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpDirectory {
    pub path: String,
    pub entries: Vec<SftpEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpMetadata {
    pub path: String,
    pub kind: SftpEntryKind,
    pub size: Option<u64>,
    pub modified: Option<u32>,
    pub permissions: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteImagePreview {
    pub path: String,
    pub name: String,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
    pub size: u64,
    pub data_base64: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTextPreview {
    pub path: String,
    pub name: String,
    pub encoding: String,
    pub language: String,
    pub size: u64,
    pub line_count: usize,
    pub content: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpPathRequest {
    pub session_id: SessionId,
    pub path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpCreateDirectoryRequest {
    pub session_id: SessionId,
    pub parent: String,
    pub name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpRenameRequest {
    pub session_id: SessionId,
    pub path: String,
    pub new_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpDeleteRequest {
    pub session_id: SessionId,
    pub path: String,
    pub recursive: bool,
}

pub type TransferJobId = Uuid;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransferDirection {
    Upload,
    Download,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransferState {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferJob {
    pub id: TransferJobId,
    pub session_id: SessionId,
    pub direction: TransferDirection,
    pub name: String,
    pub remote_path: String,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub state: TransferState,
    pub error_code: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum TransferEvent {
    Updated { job: TransferJob },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalFileSelection {
    pub grant_id: Uuid,
    pub name: String,
    pub size: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectUploadFilesRequest {
    pub session_id: SessionId,
    pub remote_directory: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectDownloadTargetRequest {
    pub suggested_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartUploadRequest {
    pub session_id: SessionId,
    pub grant_id: Uuid,
    pub remote_directory: String,
    pub overwrite: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartDownloadRequest {
    pub session_id: SessionId,
    pub grant_id: Uuid,
    pub remote_path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferJobRequest {
    pub session_id: SessionId,
    pub job_id: TransferJobId,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryTransferRequest {
    pub session_id: SessionId,
    pub job_id: TransferJobId,
    #[serde(default)]
    pub overwrite: bool,
}
