use serde::Serialize;
use uuid::Uuid;

use crate::domain::{DiskUsage, ServiceHealth};

use super::NativeToolName;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolResult {
    pub invocation_id: Uuid,
    pub tool_name: NativeToolName,
    pub success: bool,
    pub summary: &'static str,
    pub data: Option<ToolData>,
    pub error_code: Option<&'static str>,
    pub warnings: Vec<&'static str>,
    pub started_at_epoch_ms: u64,
    pub duration_ms: u64,
    pub truncated: bool,
    pub cancelled: bool,
    pub untrusted_remote_data: bool,
}

impl ToolResult {
    pub(super) fn failure(
        invocation_id: Uuid,
        tool_name: NativeToolName,
        error_code: &'static str,
        started_at_epoch_ms: u64,
        duration_ms: u64,
    ) -> Self {
        Self {
            invocation_id,
            tool_name,
            success: false,
            summary: "tool-execution-failed",
            data: None,
            error_code: Some(error_code),
            warnings: Vec::new(),
            started_at_epoch_ms,
            duration_ms,
            truncated: false,
            cancelled: false,
            untrusted_remote_data: false,
        }
    }

    pub(super) fn cancelled(
        invocation_id: Uuid,
        tool_name: NativeToolName,
        started_at_epoch_ms: u64,
        duration_ms: u64,
    ) -> Self {
        Self {
            invocation_id,
            tool_name,
            success: false,
            summary: "tool-execution-cancelled",
            data: None,
            error_code: Some("TOOL_CANCELLED"),
            warnings: Vec::new(),
            started_at_epoch_ms,
            duration_ms,
            truncated: false,
            cancelled: true,
            untrusted_remote_data: false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub(crate) enum ToolData {
    SystemInfo(SystemInfoData),
    SystemDisk(SystemDiskData),
    ServiceStatus(ServiceStatusData),
    ServiceLogs(ServiceLogsData),
    NetworkPortCheck(NetworkPortCheckData),
    HttpResponse(HttpResponseData),
    NginxTest(NginxTestData),
    FilePatch(FilePatchData),
    ServiceChange(ServiceChangeData),
    Diagnostic(DiagnosticData),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticData {
    pub category: &'static str,
    pub fields: serde_json::Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SystemInfoData {
    pub hostname: String,
    pub operating_system: String,
    pub kernel_release: String,
    pub architecture: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SystemDiskData {
    pub disks: Vec<DiskUsage>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServiceStatusData {
    pub service: ServiceHealth,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServiceLogsData {
    pub service: String,
    pub entries: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NetworkPortCheckData {
    pub host: String,
    pub port: u16,
    pub reachable: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HttpResponseData {
    pub status_code: u16,
    pub content_type: Option<String>,
    pub body_preview: String,
    pub body_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NginxTestData {
    pub valid: bool,
    pub config_file: Option<String>,
    pub error_file: Option<String>,
    pub error_line: Option<u32>,
    pub error_message: Option<String>,
    pub raw_summary: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FilePatchData {
    pub path: String,
    pub bytes: u64,
    pub verified: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServiceChangeData {
    pub service: String,
    pub action: &'static str,
    pub verified: bool,
}
