use super::state::Evidence;
use crate::tools::ToolData;
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum ModelEvidence {
    NginxValid {
        valid: bool,
    },
    ServiceActive {
        service: String,
        active: bool,
    },
    ToolFailed {
        error_code: Option<&'static str>,
    },
    SystemInfo {
        operating_system: String,
        kernel_release: String,
        architecture: String,
    },
    DiskUsage {
        highest_usage_percent: f64,
        filesystems: Vec<ModelFilesystemUsage>,
    },
    NetworkPort {
        port: u16,
        reachable: bool,
    },
    HttpResponse {
        status_code: u16,
        body_bytes: u64,
    },
    SafeMetadata {
        tool: &'static str,
    },
    Diagnostic {
        category: &'static str,
        fields: Value,
    },
    FileMetadata {
        path: String,
        size_bytes: u64,
        redacted: bool,
        truncated: bool,
    },
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelFilesystemUsage {
    mount: String,
    used_bytes: u64,
    total_bytes: u64,
    usage_percent: f64,
}

pub(crate) fn model_evidence(evidence: &[Evidence]) -> Vec<ModelEvidence> {
    evidence
        .iter()
        .map(|item| {
            if !item.result.success {
                return ModelEvidence::ToolFailed {
                    error_code: item.result.error_code,
                };
            }
            match item.result.data.as_ref() {
                Some(ToolData::NginxTest(data)) => ModelEvidence::NginxValid { valid: data.valid },
                Some(ToolData::ServiceStatus(data)) => ModelEvidence::ServiceActive {
                    service: data.service.name.clone(),
                    active: matches!(data.service.status, crate::domain::ServiceStatus::Active),
                },
                Some(ToolData::SystemInfo(data)) => ModelEvidence::SystemInfo {
                    operating_system: data.operating_system.clone(),
                    kernel_release: data.kernel_release.clone(),
                    architecture: data.architecture.clone(),
                },
                Some(ToolData::SystemDisk(data)) => {
                    let highest_usage_percent = data
                        .disks
                        .iter()
                        .map(|disk| disk.usage_percent)
                        .filter(|value| value.is_finite())
                        .fold(0.0, f64::max);
                    let mut disks = data
                        .disks
                        .iter()
                        .filter(|disk| disk.usage_percent.is_finite())
                        .collect::<Vec<_>>();
                    disks.sort_by(|left, right| right.usage_percent.total_cmp(&left.usage_percent));
                    ModelEvidence::DiskUsage {
                        highest_usage_percent,
                        filesystems: disks
                            .into_iter()
                            .take(12)
                            .map(|disk| ModelFilesystemUsage {
                                mount: disk.mount.chars().take(256).collect(),
                                used_bytes: disk.used_bytes,
                                total_bytes: disk.total_bytes,
                                usage_percent: disk.usage_percent,
                            })
                            .collect(),
                    }
                }
                Some(ToolData::NetworkPortCheck(data)) => ModelEvidence::NetworkPort {
                    port: data.port,
                    reachable: data.reachable,
                },
                Some(ToolData::HttpResponse(data)) => ModelEvidence::HttpResponse {
                    status_code: data.status_code,
                    body_bytes: data.body_bytes,
                },
                Some(ToolData::Diagnostic(data)) if data.category == "file" => {
                    ModelEvidence::FileMetadata {
                        path: data
                            .fields
                            .get("path")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        size_bytes: data
                            .fields
                            .get("sizeBytes")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                        redacted: data
                            .fields
                            .get("redacted")
                            .and_then(Value::as_bool)
                            .unwrap_or(true),
                        truncated: data
                            .fields
                            .get("truncated")
                            .and_then(Value::as_bool)
                            .unwrap_or(true),
                    }
                }
                Some(ToolData::Diagnostic(data))
                    if matches!(
                        data.category,
                        "dns"
                            | "tls"
                            | "network-listeners"
                            | "directory-usage"
                            | "large-files"
                            | "docker-list"
                            | "docker-inspect"
                    ) =>
                {
                    ModelEvidence::Diagnostic {
                        category: data.category,
                        fields: bounded_diagnostic_fields(&data.fields),
                    }
                }
                _ => ModelEvidence::SafeMetadata {
                    tool: item.result.tool_name.as_str(),
                },
            }
        })
        .collect()
}

fn bounded_diagnostic_fields(fields: &Value) -> Value {
    const MAX_BYTES: usize = 16 * 1024;
    if serde_json::to_vec(fields).map_or(true, |value| value.len() > MAX_BYTES) {
        serde_json::json!({"truncated": true})
    } else {
        fields.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentic::context::ContextTrust;
    use crate::domain::DiskUsage;
    use crate::tools::{NativeToolName, SystemDiskData, ToolResult};
    use uuid::Uuid;

    #[test]
    fn disk_evidence_contains_bounded_sorted_filesystem_details() {
        let invocation_id = Uuid::new_v4();
        let evidence = Evidence {
            id: Uuid::new_v4(),
            source: "tool.system.disk_usage".into(),
            invocation_id,
            trust: ContextTrust::UntrustedRemoteData,
            summary: "system-disk-collected",
            result: ToolResult {
                invocation_id,
                tool_name: NativeToolName::SystemDisk,
                success: true,
                summary: "system-disk-collected",
                data: Some(ToolData::SystemDisk(SystemDiskData {
                    disks: vec![
                        DiskUsage {
                            mount: "/".into(),
                            used_bytes: 40,
                            total_bytes: 100,
                            usage_percent: 40.0,
                        },
                        DiskUsage {
                            mount: "/data".into(),
                            used_bytes: 90,
                            total_bytes: 100,
                            usage_percent: 90.0,
                        },
                    ],
                })),
                error_code: None,
                warnings: Vec::new(),
                started_at_epoch_ms: 1,
                duration_ms: 1,
                truncated: false,
                cancelled: false,
                untrusted_remote_data: true,
            },
        };

        let value = serde_json::to_value(model_evidence(&[evidence])).expect("serialize");
        assert_eq!(value[0]["kind"], "disk-usage");
        assert_eq!(value[0]["highest_usage_percent"], 90.0);
        assert_eq!(value[0]["filesystems"][0]["mount"], "/data");
        assert_eq!(value[0]["filesystems"][0]["usagePercent"], 90.0);
    }
}
