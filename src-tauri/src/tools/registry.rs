use std::collections::BTreeMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};

use crate::domain::{AppError, AppResult, SessionId};
use crate::ssh::ServerSessionManager;

use super::{
    change, http, incident, native_descriptors, network, nginx, service, system, NativeToolName,
    ToolData, ToolDescriptor, ToolResult,
};

#[derive(Clone, Debug)]
pub(crate) enum NativeToolInvocation {
    SystemInfo,
    SystemDisk,
    ServiceStatus {
        service: String,
    },
    ServiceLogs {
        service: String,
        lines: u32,
    },
    NetworkPortCheck {
        host: String,
        port: u16,
    },
    HttpRequest {
        url: String,
    },
    NginxTest,
    DnsResolve {
        host: String,
    },
    TlsInspect {
        host: String,
        port: u16,
    },
    NetworkListeners,
    ProcessList,
    FileInspect {
        path: String,
    },
    SystemDirectoryUsage {
        path: String,
    },
    SystemLargeFiles {
        path: String,
        minimum_bytes: u64,
    },
    DockerList,
    DockerInspect {
        container: String,
    },
    DockerLogs {
        container: String,
        lines: u32,
    },
    FilePatch {
        path: String,
        expected: String,
        replacement: String,
    },
    ServiceRestart {
        service: String,
    },
    ServiceReload {
        service: String,
    },
    NginxReload,
    DockerRestart {
        container: String,
    },
}

impl NativeToolInvocation {
    pub(crate) const fn name(&self) -> NativeToolName {
        match self {
            Self::SystemInfo => NativeToolName::SystemInfo,
            Self::SystemDisk => NativeToolName::SystemDisk,
            Self::ServiceStatus { .. } => NativeToolName::ServiceStatus,
            Self::ServiceLogs { .. } => NativeToolName::ServiceLogs,
            Self::NetworkPortCheck { .. } => NativeToolName::NetworkPortCheck,
            Self::HttpRequest { .. } => NativeToolName::HttpRequest,
            Self::NginxTest => NativeToolName::NginxTest,
            Self::DnsResolve { .. } => NativeToolName::DnsResolve,
            Self::TlsInspect { .. } => NativeToolName::TlsInspect,
            Self::NetworkListeners => NativeToolName::NetworkListeners,
            Self::ProcessList => NativeToolName::ProcessList,
            Self::FileInspect { .. } => NativeToolName::FileInspect,
            Self::SystemDirectoryUsage { .. } => NativeToolName::SystemDirectoryUsage,
            Self::SystemLargeFiles { .. } => NativeToolName::SystemLargeFiles,
            Self::DockerList => NativeToolName::DockerList,
            Self::DockerInspect { .. } => NativeToolName::DockerInspect,
            Self::DockerLogs { .. } => NativeToolName::DockerLogs,
            Self::FilePatch { .. } => NativeToolName::FilePatch,
            Self::ServiceRestart { .. } => NativeToolName::ServiceRestart,
            Self::ServiceReload { .. } => NativeToolName::ServiceReload,
            Self::NginxReload => NativeToolName::NginxReload,
            Self::DockerRestart { .. } => NativeToolName::DockerRestart,
        }
    }

    /// Converts an LLM-proposed call into the closed native read-tool vocabulary.
    /// Write tools are deliberately absent: model output never grants authority.
    pub(crate) fn from_model_read_call(name: NativeToolName, arguments: Value) -> AppResult<Self> {
        let arguments = arguments
            .as_object()
            .ok_or(AppError::ModelResponseInvalid)?;
        match name {
            NativeToolName::SystemInfo => empty(arguments).map(|()| Self::SystemInfo),
            NativeToolName::SystemDisk => empty(arguments).map(|()| Self::SystemDisk),
            NativeToolName::ServiceStatus => Ok(Self::ServiceStatus {
                service: identifier(only_string(arguments, "service")?, 128)?,
            }),
            NativeToolName::ServiceLogs => Ok(Self::ServiceLogs {
                service: identifier(required_string(arguments, "service")?, 128)?,
                lines: bounded_u32(arguments, "lines", 20, 500)?,
            }),
            NativeToolName::NetworkPortCheck => Ok(Self::NetworkPortCheck {
                host: host(required_string(arguments, "host")?)?,
                port: port(arguments)?,
            }),
            NativeToolName::HttpRequest => Ok(Self::HttpRequest {
                url: bounded_string(only_string(arguments, "url")?, 8, 2_048)?,
            }),
            NativeToolName::NginxTest => empty(arguments).map(|()| Self::NginxTest),
            NativeToolName::DnsResolve => Ok(Self::DnsResolve {
                host: host(only_string(arguments, "host")?)?,
            }),
            NativeToolName::TlsInspect => Ok(Self::TlsInspect {
                host: host(required_string(arguments, "host")?)?,
                port: port(arguments)?,
            }),
            NativeToolName::NetworkListeners => empty(arguments).map(|()| Self::NetworkListeners),
            NativeToolName::ProcessList => empty(arguments).map(|()| Self::ProcessList),
            NativeToolName::FileInspect => Ok(Self::FileInspect {
                path: path(only_string(arguments, "path")?)?,
            }),
            NativeToolName::SystemDirectoryUsage => Ok(Self::SystemDirectoryUsage {
                path: path(only_string(arguments, "path")?)?,
            }),
            NativeToolName::SystemLargeFiles => Ok(Self::SystemLargeFiles {
                path: path(required_string(arguments, "path")?)?,
                minimum_bytes: bounded_u64(
                    arguments,
                    "minimumBytes",
                    1_048_576,
                    1_099_511_627_776,
                )?,
            }),
            NativeToolName::DockerList => empty(arguments).map(|()| Self::DockerList),
            NativeToolName::DockerInspect => Ok(Self::DockerInspect {
                container: identifier(only_string(arguments, "container")?, 256)?,
            }),
            NativeToolName::DockerLogs => Ok(Self::DockerLogs {
                container: identifier(required_string(arguments, "container")?, 256)?,
                lines: bounded_u32(arguments, "lines", 20, 500)?,
            }),
            NativeToolName::FilePatch
            | NativeToolName::ServiceRestart
            | NativeToolName::ServiceReload
            | NativeToolName::NginxReload
            | NativeToolName::DockerRestart => Err(AppError::ModelResponseInvalid),
        }
    }
}

fn empty(arguments: &Map<String, Value>) -> AppResult<()> {
    if arguments.is_empty() {
        Ok(())
    } else {
        Err(AppError::ModelResponseInvalid)
    }
}

fn only_string(arguments: &Map<String, Value>, key: &str) -> AppResult<String> {
    if arguments.len() != 1 {
        return Err(AppError::ModelResponseInvalid);
    }
    required_string(arguments, key)
}

fn required_string(arguments: &Map<String, Value>, key: &str) -> AppResult<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or(AppError::ModelResponseInvalid)
}

fn bounded_string(value: String, minimum: usize, maximum: usize) -> AppResult<String> {
    if (minimum..=maximum).contains(&value.len()) && !value.chars().any(char::is_control) {
        Ok(value)
    } else {
        Err(AppError::ModelResponseInvalid)
    }
}

fn identifier(value: String, maximum: usize) -> AppResult<String> {
    let value = bounded_string(value, 1, maximum)?;
    if value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'@' | b':' | b'-')
    }) {
        Ok(value)
    } else {
        Err(AppError::ModelResponseInvalid)
    }
}

fn host(value: String) -> AppResult<String> {
    let value = bounded_string(value, 1, 255)?;
    if !value.starts_with('-') && !value.chars().any(char::is_whitespace) {
        Ok(value)
    } else {
        Err(AppError::ModelResponseInvalid)
    }
}

fn path(value: String) -> AppResult<String> {
    let value = bounded_string(value, 1, 4_096)?;
    if value.starts_with('/') {
        Ok(value)
    } else {
        Err(AppError::ModelResponseInvalid)
    }
}

fn bounded_u32(
    arguments: &Map<String, Value>,
    key: &str,
    minimum: u32,
    maximum: u32,
) -> AppResult<u32> {
    if arguments.len() != 2 {
        return Err(AppError::ModelResponseInvalid);
    }
    let value = arguments
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(AppError::ModelResponseInvalid)?;
    if (minimum..=maximum).contains(&value) {
        Ok(value)
    } else {
        Err(AppError::ModelResponseInvalid)
    }
}

fn bounded_u64(
    arguments: &Map<String, Value>,
    key: &str,
    minimum: u64,
    maximum: u64,
) -> AppResult<u64> {
    if arguments.len() != 2 {
        return Err(AppError::ModelResponseInvalid);
    }
    arguments
        .get(key)
        .and_then(Value::as_u64)
        .filter(|value| (minimum..=maximum).contains(value))
        .ok_or(AppError::ModelResponseInvalid)
}

fn port(arguments: &Map<String, Value>) -> AppResult<u16> {
    if arguments.len() != 2 {
        return Err(AppError::ModelResponseInvalid);
    }
    arguments
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or(AppError::ModelResponseInvalid)
}

pub(crate) struct NativeToolRegistry {
    descriptors: BTreeMap<NativeToolName, ToolDescriptor>,
}

impl NativeToolRegistry {
    pub(super) fn new() -> Self {
        let descriptors = native_descriptors()
            .into_iter()
            .map(|descriptor| (descriptor.name, descriptor))
            .collect();
        Self { descriptors }
    }

    pub(super) fn descriptors(&self) -> Vec<&ToolDescriptor> {
        NativeToolName::ALL
            .iter()
            .filter_map(|name| self.descriptors.get(name))
            .collect()
    }

    pub(super) fn descriptor(&self, name: NativeToolName) -> Option<&ToolDescriptor> {
        self.descriptors.get(&name)
    }

    pub(super) async fn execute(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        invocation_id: uuid::Uuid,
        invocation: NativeToolInvocation,
    ) -> ToolResult {
        let started_at_epoch_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        let started = Instant::now();
        let tool_name = invocation.name();
        let outcome: AppResult<(ToolData, &'static str, bool)> = async {
            Ok(match invocation {
                NativeToolInvocation::SystemInfo => (
                    ToolData::SystemInfo(system::info(sessions, session_id).await?),
                    "system-info-collected",
                    false,
                ),
                NativeToolInvocation::SystemDisk => (
                    ToolData::SystemDisk(system::disk(sessions, session_id).await?),
                    "system-disk-collected",
                    false,
                ),
                NativeToolInvocation::ServiceStatus {
                    service: service_name,
                } => (
                    ToolData::ServiceStatus(
                        service::status(sessions, session_id, service_name).await?,
                    ),
                    "service-status-collected",
                    false,
                ),
                NativeToolInvocation::ServiceLogs {
                    service: service_name,
                    lines,
                } => {
                    let (logs, truncated) =
                        service::logs(sessions, session_id, service_name, lines).await?;
                    (
                        ToolData::ServiceLogs(logs),
                        "service-logs-collected",
                        truncated,
                    )
                }
                NativeToolInvocation::NetworkPortCheck { host, port } => (
                    ToolData::NetworkPortCheck(
                        network::port_check(sessions, session_id, host, port).await?,
                    ),
                    "network-port-checked",
                    false,
                ),
                NativeToolInvocation::HttpRequest { url } => {
                    let (response, truncated) = http::request(sessions, session_id, url).await?;
                    (
                        ToolData::HttpResponse(response),
                        "http-request-completed",
                        truncated,
                    )
                }
                NativeToolInvocation::NginxTest => (
                    ToolData::NginxTest(nginx::test(sessions, session_id).await?),
                    "nginx-configuration-tested",
                    false,
                ),
                NativeToolInvocation::DnsResolve { host } => diagnostic(
                    incident::dns_resolve(sessions, session_id, host).await?,
                    "dns-resolved",
                ),
                NativeToolInvocation::TlsInspect { host, port } => diagnostic(
                    incident::tls_inspect(sessions, session_id, host, port).await?,
                    "tls-inspected",
                ),
                NativeToolInvocation::NetworkListeners => diagnostic(
                    incident::network_listeners(sessions, session_id).await?,
                    "network-listeners-collected",
                ),
                NativeToolInvocation::ProcessList => diagnostic(
                    incident::process_list(sessions, session_id).await?,
                    "process-list-collected",
                ),
                NativeToolInvocation::FileInspect { path } => diagnostic(
                    incident::file_inspect(sessions, session_id, path).await?,
                    "file-inspected",
                ),
                NativeToolInvocation::SystemDirectoryUsage { path } => diagnostic(
                    incident::directory_usage(sessions, session_id, path).await?,
                    "directory-usage-collected",
                ),
                NativeToolInvocation::SystemLargeFiles {
                    path,
                    minimum_bytes,
                } => diagnostic(
                    incident::large_files(sessions, session_id, path, minimum_bytes).await?,
                    "large-files-collected",
                ),
                NativeToolInvocation::DockerList => diagnostic(
                    incident::docker_list(sessions, session_id).await?,
                    "docker-list-collected",
                ),
                NativeToolInvocation::DockerInspect { container } => diagnostic(
                    incident::docker_inspect(sessions, session_id, container).await?,
                    "docker-inspected",
                ),
                NativeToolInvocation::DockerLogs { container, lines } => diagnostic(
                    incident::docker_logs(sessions, session_id, container, lines).await?,
                    "docker-logs-collected",
                ),
                NativeToolInvocation::FilePatch {
                    path,
                    expected,
                    replacement,
                } => (
                    ToolData::FilePatch(
                        change::file_patch(sessions, session_id, path, expected, replacement)
                            .await?,
                    ),
                    "file-patch-applied",
                    false,
                ),
                NativeToolInvocation::ServiceRestart { service } => (
                    ToolData::ServiceChange(
                        change::service_change(sessions, session_id, service, "restart").await?,
                    ),
                    "service-restarted",
                    false,
                ),
                NativeToolInvocation::ServiceReload { service } => (
                    ToolData::ServiceChange(
                        change::service_change(sessions, session_id, service, "reload").await?,
                    ),
                    "service-reloaded",
                    false,
                ),
                NativeToolInvocation::NginxReload => (
                    ToolData::ServiceChange(change::nginx_reload(sessions, session_id).await?),
                    "nginx-reloaded",
                    false,
                ),
                NativeToolInvocation::DockerRestart { container } => (
                    ToolData::ServiceChange(
                        change::docker_restart(sessions, session_id, container).await?,
                    ),
                    "docker-restarted",
                    false,
                ),
            })
        }
        .await;
        let (success, summary, data, error_code, truncated) = match outcome {
            Ok((data, summary, truncated)) => (true, summary, Some(data), None, truncated),
            Err(error) => (
                false,
                "tool-execution-failed",
                None,
                Some(error.code()),
                false,
            ),
        };
        ToolResult {
            invocation_id,
            tool_name,
            success,
            summary,
            data,
            error_code,
            warnings: if truncated {
                vec!["remote-output-truncated"]
            } else {
                Vec::new()
            },
            started_at_epoch_ms,
            duration_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            truncated,
            cancelled: false,
            untrusted_remote_data: true,
        }
    }
}

fn diagnostic(
    data: super::DiagnosticData,
    summary: &'static str,
) -> (ToolData, &'static str, bool) {
    (ToolData::Diagnostic(data), summary, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lookup_is_closed_and_stable() {
        let registry = NativeToolRegistry::new();
        assert_eq!(registry.descriptors().len(), NativeToolName::ALL.len());
        for name in NativeToolName::ALL {
            assert_eq!(registry.descriptor(name).map(|item| item.name), Some(name));
        }
    }

    #[test]
    fn typed_invocations_map_to_fixed_tool_names() {
        let invocations = [
            NativeToolInvocation::SystemInfo,
            NativeToolInvocation::SystemDisk,
            NativeToolInvocation::ServiceStatus {
                service: "sshd".into(),
            },
            NativeToolInvocation::ServiceLogs {
                service: "sshd".into(),
                lines: 20,
            },
            NativeToolInvocation::NetworkPortCheck {
                host: "127.0.0.1".into(),
                port: 22,
            },
            NativeToolInvocation::HttpRequest {
                url: "http://127.0.0.1".into(),
            },
            NativeToolInvocation::NginxTest,
            NativeToolInvocation::DnsResolve {
                host: "example.com".into(),
            },
            NativeToolInvocation::TlsInspect {
                host: "example.com".into(),
                port: 443,
            },
            NativeToolInvocation::NetworkListeners,
            NativeToolInvocation::ProcessList,
            NativeToolInvocation::FileInspect {
                path: "/etc/nginx/nginx.conf".into(),
            },
            NativeToolInvocation::SystemDirectoryUsage {
                path: "/var/log".into(),
            },
            NativeToolInvocation::SystemLargeFiles {
                path: "/var/log".into(),
                minimum_bytes: 100 * 1024 * 1024,
            },
            NativeToolInvocation::DockerList,
            NativeToolInvocation::DockerInspect {
                container: "web".into(),
            },
            NativeToolInvocation::DockerLogs {
                container: "web".into(),
                lines: 20,
            },
        ];
        assert_eq!(
            invocations
                .iter()
                .map(NativeToolInvocation::name)
                .collect::<Vec<_>>(),
            NativeToolName::READ_ONLY
        );
    }

    #[test]
    fn model_calls_are_read_only_and_exactly_typed() {
        let invocation = NativeToolInvocation::from_model_read_call(
            NativeToolName::SystemLargeFiles,
            serde_json::json!({"path":"/var/log","minimumBytes":104857600}),
        )
        .expect("valid read tool");
        assert_eq!(invocation.name(), NativeToolName::SystemLargeFiles);
        assert!(NativeToolInvocation::from_model_read_call(
            NativeToolName::ServiceRestart,
            serde_json::json!({"service":"nginx"}),
        )
        .is_err());
        assert!(NativeToolInvocation::from_model_read_call(
            NativeToolName::SystemInfo,
            serde_json::json!({"unexpected":true}),
        )
        .is_err());
        assert!(NativeToolInvocation::from_model_read_call(
            NativeToolName::FileInspect,
            serde_json::json!({"path":"relative.txt"}),
        )
        .is_err());
    }

    #[tokio::test]
    async fn validation_failures_stay_inside_the_structured_result_boundary() {
        let result = NativeToolRegistry::new()
            .execute(
                &ServerSessionManager::default(),
                uuid::Uuid::new_v4(),
                uuid::Uuid::new_v4(),
                NativeToolInvocation::HttpRequest {
                    url: "file:///etc/passwd".into(),
                },
            )
            .await;
        assert!(!result.success);
        assert_eq!(result.error_code, Some("INVALID_OPERATION"));
        assert!(result.data.is_none());
    }
}
