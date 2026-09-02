use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub(crate) enum NativeToolName {
    #[serde(rename = "system.info")]
    SystemInfo,
    #[serde(rename = "system.disk_usage", alias = "system.disk")]
    SystemDisk,
    #[serde(rename = "service.status")]
    ServiceStatus,
    #[serde(rename = "service.logs")]
    ServiceLogs,
    #[serde(rename = "network.port_check")]
    NetworkPortCheck,
    #[serde(rename = "http.request")]
    HttpRequest,
    #[serde(rename = "nginx.test")]
    NginxTest,
    #[serde(rename = "dns.resolve")]
    DnsResolve,
    #[serde(rename = "tls.inspect")]
    TlsInspect,
    #[serde(rename = "network.listeners")]
    NetworkListeners,
    #[serde(rename = "process.list")]
    ProcessList,
    #[serde(rename = "file.inspect")]
    FileInspect,
    #[serde(rename = "system.directory_usage")]
    SystemDirectoryUsage,
    #[serde(rename = "system.large_files")]
    SystemLargeFiles,
    #[serde(rename = "docker.list")]
    DockerList,
    #[serde(rename = "docker.inspect")]
    DockerInspect,
    #[serde(rename = "docker.logs")]
    DockerLogs,
    #[serde(rename = "file.patch")]
    FilePatch,
    #[serde(rename = "service.restart")]
    ServiceRestart,
    #[serde(rename = "service.reload")]
    ServiceReload,
    #[serde(rename = "nginx.reload")]
    NginxReload,
    #[serde(rename = "docker.restart")]
    DockerRestart,
    #[serde(rename = "filesystem.inode_usage")]
    FilesystemInodeUsage,
    #[serde(rename = "block_devices.list")]
    BlockDevicesList,
    #[serde(rename = "terminal.exec_readonly")]
    TerminalExecReadonly,
}

impl NativeToolName {
    pub(crate) const READ_ONLY: [Self; 20] = [
        Self::SystemInfo,
        Self::SystemDisk,
        Self::ServiceStatus,
        Self::ServiceLogs,
        Self::NetworkPortCheck,
        Self::HttpRequest,
        Self::NginxTest,
        Self::DnsResolve,
        Self::TlsInspect,
        Self::NetworkListeners,
        Self::ProcessList,
        Self::FileInspect,
        Self::SystemDirectoryUsage,
        Self::SystemLargeFiles,
        Self::DockerList,
        Self::DockerInspect,
        Self::DockerLogs,
        Self::FilesystemInodeUsage,
        Self::BlockDevicesList,
        Self::TerminalExecReadonly,
    ];

    pub(crate) const ALL: [Self; 25] = [
        Self::SystemInfo,
        Self::SystemDisk,
        Self::ServiceStatus,
        Self::ServiceLogs,
        Self::NetworkPortCheck,
        Self::HttpRequest,
        Self::NginxTest,
        Self::DnsResolve,
        Self::TlsInspect,
        Self::NetworkListeners,
        Self::ProcessList,
        Self::FileInspect,
        Self::SystemDirectoryUsage,
        Self::SystemLargeFiles,
        Self::DockerList,
        Self::DockerInspect,
        Self::DockerLogs,
        Self::FilesystemInodeUsage,
        Self::BlockDevicesList,
        Self::TerminalExecReadonly,
        Self::FilePatch,
        Self::ServiceRestart,
        Self::ServiceReload,
        Self::NginxReload,
        Self::DockerRestart,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::SystemInfo => "system.info",
            Self::SystemDisk => "system.disk_usage",
            Self::ServiceStatus => "service.status",
            Self::ServiceLogs => "service.logs",
            Self::NetworkPortCheck => "network.port_check",
            Self::HttpRequest => "http.request",
            Self::NginxTest => "nginx.test",
            Self::DnsResolve => "dns.resolve",
            Self::TlsInspect => "tls.inspect",
            Self::NetworkListeners => "network.listeners",
            Self::ProcessList => "process.list",
            Self::FileInspect => "file.inspect",
            Self::SystemDirectoryUsage => "system.directory_usage",
            Self::SystemLargeFiles => "system.large_files",
            Self::DockerList => "docker.list",
            Self::DockerInspect => "docker.inspect",
            Self::DockerLogs => "docker.logs",
            Self::FilePatch => "file.patch",
            Self::ServiceRestart => "service.restart",
            Self::ServiceReload => "service.reload",
            Self::NginxReload => "nginx.reload",
            Self::DockerRestart => "docker.restart",
            Self::FilesystemInodeUsage => "filesystem.inode_usage",
            Self::BlockDevicesList => "block_devices.list",
            Self::TerminalExecReadonly => "terminal.exec_readonly",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
// The full policy vocabulary is defined now even though this read-only phase uses R0/R1 only.
#[allow(dead_code)]
pub(crate) enum RiskLevel {
    R0,
    R1,
    R2,
    R3,
    R4,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
// Write is reserved for a later, explicitly authorized ChangeSet phase.
#[allow(dead_code)]
pub(crate) enum Mutability {
    Read,
    Write,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ResourceImpact {
    Low,
    Medium,
    HighIo,
    HighCpu,
    LongRunning,
    ExternalCost,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[allow(dead_code)]
pub(crate) enum ToolScope {
    Server,
    Session,
    Workspace,
    External,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolDescriptor {
    pub name: NativeToolName,
    pub description: &'static str,
    pub input_schema: Value,
    pub output_schema: Value,
    pub risk_level: RiskLevel,
    pub mutability: Mutability,
    pub requires_approval: bool,
    pub supports_rollback: bool,
    pub scope: ToolScope,
    pub resource_impact: ResourceImpact,
    pub timeout_ms: u64,
}

pub(crate) fn resource_impact_for(name: NativeToolName) -> ResourceImpact {
    native_descriptors()
        .into_iter()
        .find(|descriptor| descriptor.name == name)
        .map(|descriptor| descriptor.resource_impact)
        .unwrap_or(ResourceImpact::Medium)
}

pub(crate) fn native_descriptors() -> Vec<ToolDescriptor> {
    let mut descriptors = vec![
        descriptor(
            NativeToolName::SystemInfo,
            "Collect basic operating system and host information.",
            empty_input(),
            object_output(&[
                ("hostname", json!({ "type": "string" })),
                ("operatingSystem", json!({ "type": "string" })),
                ("kernelRelease", json!({ "type": "string" })),
                ("architecture", json!({ "type": "string" })),
            ]),
            RiskLevel::R0,
            ResourceImpact::Low,
        ),
        descriptor(
            NativeToolName::SystemDisk,
            "Collect mounted filesystem capacity and utilization.",
            empty_input(),
            object_output(&[(
                "disks",
                json!({
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "mount": { "type": "string" },
                            "usedBytes": { "type": "integer", "minimum": 0 },
                            "totalBytes": { "type": "integer", "minimum": 0 },
                            "usagePercent": { "type": "number", "minimum": 0 }
                        },
                        "required": ["mount", "usedBytes", "totalBytes", "usagePercent"],
                        "additionalProperties": false
                    }
                }),
            )]),
            RiskLevel::R0,
            ResourceImpact::Low,
        ),
        descriptor(
            NativeToolName::ServiceStatus,
            "Read the current status of one system service.",
            object_input(&[(
                "service",
                json!({ "type": "string", "minLength": 1, "maxLength": 128 }),
            )]),
            object_output(&[(
                "service",
                json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "status": { "enum": ["active", "inactive", "failed", "unknown"] }
                    },
                    "required": ["name", "status"],
                    "additionalProperties": false
                }),
            )]),
            RiskLevel::R0,
            ResourceImpact::Low,
        ),
        descriptor(
            NativeToolName::ServiceLogs,
            "Read a bounded tail of logs for one system service.",
            object_input(&[
                (
                    "service",
                    json!({ "type": "string", "minLength": 1, "maxLength": 128 }),
                ),
                (
                    "lines",
                    json!({ "type": "integer", "minimum": 20, "maximum": 500 }),
                ),
            ]),
            object_output(&[
                ("service", json!({ "type": "string" })),
                (
                    "entries",
                    json!({ "type": "array", "items": { "type": "string" } }),
                ),
            ]),
            RiskLevel::R1,
            ResourceImpact::LongRunning,
        ),
        descriptor(
            NativeToolName::NetworkPortCheck,
            "Check TCP reachability from the selected server session.",
            object_input(&[
                (
                    "host",
                    json!({ "type": "string", "minLength": 1, "maxLength": 255 }),
                ),
                (
                    "port",
                    json!({ "type": "integer", "minimum": 1, "maximum": 65535 }),
                ),
            ]),
            object_output(&[
                ("host", json!({ "type": "string" })),
                (
                    "port",
                    json!({ "type": "integer", "minimum": 1, "maximum": 65535 }),
                ),
                ("reachable", json!({ "type": "boolean" })),
            ]),
            RiskLevel::R1,
            ResourceImpact::Medium,
        ),
        descriptor(
            NativeToolName::HttpRequest,
            "Perform one bounded HTTP GET request from the selected server session.",
            object_input(&[(
                "url",
                json!({ "type": "string", "minLength": 8, "maxLength": 2048 }),
            )]),
            object_output(&[
                (
                    "statusCode",
                    json!({ "type": "integer", "minimum": 100, "maximum": 599 }),
                ),
                ("contentType", json!({ "type": ["string", "null"] })),
                ("bodyPreview", json!({ "type": "string" })),
                ("bodyBytes", json!({ "type": "integer", "minimum": 0 })),
            ]),
            RiskLevel::R1,
            ResourceImpact::ExternalCost,
        ),
        descriptor(
            NativeToolName::NginxTest,
            "Validate the active Nginx configuration without reloading it.",
            empty_input(),
            object_output(&[
                ("valid", json!({ "type": "boolean" })),
                ("configFile", json!({ "type": ["string", "null"] })),
                ("errorFile", json!({ "type": ["string", "null"] })),
                (
                    "errorLine",
                    json!({ "type": ["integer", "null"], "minimum": 1 }),
                ),
                ("errorMessage", json!({ "type": ["string", "null"] })),
                (
                    "rawSummary",
                    json!({ "type": "string", "maxLength": 16384 }),
                ),
            ]),
            RiskLevel::R1,
            ResourceImpact::Low,
        ),
        descriptor(
            NativeToolName::DnsResolve,
            "Resolve one DNS name from the target.",
            object_input(&[(
                "host",
                json!({"type":"string","minLength":1,"maxLength":255}),
            )]),
            generic_object_output(),
            RiskLevel::R1,
            ResourceImpact::Medium,
        ),
        descriptor(
            NativeToolName::TlsInspect,
            "Inspect one TLS endpoint without sending application data.",
            object_input(&[
                (
                    "host",
                    json!({"type":"string","minLength":1,"maxLength":255}),
                ),
                (
                    "port",
                    json!({"type":"integer","minimum":1,"maximum":65535}),
                ),
            ]),
            generic_object_output(),
            RiskLevel::R1,
            ResourceImpact::ExternalCost,
        ),
        descriptor(
            NativeToolName::NetworkListeners,
            "List bounded TCP listeners.",
            empty_input(),
            generic_object_output(),
            RiskLevel::R1,
            ResourceImpact::Medium,
        ),
        descriptor(
            NativeToolName::ProcessList,
            "List bounded high-CPU processes.",
            empty_input(),
            generic_object_output(),
            RiskLevel::R1,
            ResourceImpact::HighCpu,
        ),
        descriptor(
            NativeToolName::FileInspect,
            "Inspect bounded metadata and redacted text for one explicit remote file.",
            object_input(&[(
                "path",
                json!({"type":"string","minLength":1,"maxLength":4096}),
            )]),
            generic_object_output(),
            RiskLevel::R1,
            ResourceImpact::Medium,
        ),
        descriptor(
            NativeToolName::SystemDirectoryUsage,
            "Measure immediate children of one explicit directory.",
            object_input(&[(
                "path",
                json!({"type":"string","minLength":1,"maxLength":4096}),
            )]),
            generic_object_output(),
            RiskLevel::R1,
            ResourceImpact::HighIo,
        ),
        descriptor(
            NativeToolName::SystemLargeFiles,
            "List bounded large regular files below one explicit directory.",
            object_input(&[
                (
                    "path",
                    json!({"type":"string","minLength":1,"maxLength":4096}),
                ),
                (
                    "minimumBytes",
                    json!({"type":"integer","minimum":1048576,"maximum":1099511627776_u64}),
                ),
            ]),
            generic_object_output(),
            RiskLevel::R1,
            ResourceImpact::HighIo,
        ),
        descriptor(
            NativeToolName::DockerList,
            "List bounded Docker container state.",
            empty_input(),
            generic_object_output(),
            RiskLevel::R1,
            ResourceImpact::Medium,
        ),
        descriptor(
            NativeToolName::DockerInspect,
            "Inspect one validated Docker container.",
            object_input(&[(
                "container",
                json!({"type":"string","minLength":1,"maxLength":256}),
            )]),
            generic_object_output(),
            RiskLevel::R1,
            ResourceImpact::Medium,
        ),
        descriptor(
            NativeToolName::DockerLogs,
            "Read a bounded Docker log tail.",
            object_input(&[
                (
                    "container",
                    json!({"type":"string","minLength":1,"maxLength":256}),
                ),
                (
                    "lines",
                    json!({"type":"integer","minimum":20,"maximum":500}),
                ),
            ]),
            generic_object_output(),
            RiskLevel::R1,
            ResourceImpact::LongRunning,
        ),
        descriptor(
            NativeToolName::FilesystemInodeUsage,
            "Collect inode utilization for mounted filesystems (df -i).",
            empty_input(),
            generic_object_output(),
            RiskLevel::R0,
            ResourceImpact::Low,
        ),
        descriptor(
            NativeToolName::BlockDevicesList,
            "List block devices and mount relationships (lsblk).",
            empty_input(),
            generic_object_output(),
            RiskLevel::R0,
            ResourceImpact::Low,
        ),
        descriptor(
            NativeToolName::TerminalExecReadonly,
            "Execute one allowlisted read-only command when no typed tool exists.",
            object_input(&[(
                "command",
                json!({"type":"string","minLength":1,"maxLength":256}),
            )]),
            generic_object_output(),
            RiskLevel::R1,
            ResourceImpact::Medium,
        ),
    ];
    descriptors.extend([
        write_descriptor(
            NativeToolName::FilePatch,
            "Apply one exact bounded text replacement.",
            RiskLevel::R3,
            true,
        ),
        write_descriptor(
            NativeToolName::ServiceRestart,
            "Restart one validated system service.",
            RiskLevel::R3,
            false,
        ),
        write_descriptor(
            NativeToolName::ServiceReload,
            "Reload one validated system service.",
            RiskLevel::R2,
            false,
        ),
        write_descriptor(
            NativeToolName::NginxReload,
            "Reload Nginx after configuration validation.",
            RiskLevel::R2,
            false,
        ),
        write_descriptor(
            NativeToolName::DockerRestart,
            "Restart one validated Docker container.",
            RiskLevel::R3,
            false,
        ),
    ]);
    descriptors
}

fn write_descriptor(
    name: NativeToolName,
    description: &'static str,
    risk_level: RiskLevel,
    supports_rollback: bool,
) -> ToolDescriptor {
    ToolDescriptor {
        name,
        description,
        input_schema: write_input_schema(name),
        output_schema: write_output_schema(name),
        risk_level,
        mutability: Mutability::Write,
        requires_approval: true,
        supports_rollback,
        scope: ToolScope::Session,
        resource_impact: ResourceImpact::Medium,
        timeout_ms: 30_000,
    }
}

fn write_input_schema(name: NativeToolName) -> Value {
    match name {
        NativeToolName::FilePatch => object_input(&[
            (
                "path",
                json!({"type":"string","minLength":1,"maxLength":4096}),
            ),
            (
                "expected",
                json!({"type":"string","minLength":1,"maxLength":524288}),
            ),
            ("replacement", json!({"type":"string","maxLength":524288})),
        ]),
        NativeToolName::ServiceRestart | NativeToolName::ServiceReload => object_input(&[(
            "service",
            json!({"type":"string","minLength":1,"maxLength":128}),
        )]),
        NativeToolName::NginxReload => empty_input(),
        NativeToolName::DockerRestart => object_input(&[(
            "container",
            json!({"type":"string","minLength":1,"maxLength":256}),
        )]),
        _ => empty_input(),
    }
}

fn write_output_schema(name: NativeToolName) -> Value {
    match name {
        NativeToolName::FilePatch => object_output(&[
            ("path", json!({"type":"string"})),
            ("bytes", json!({"type":"integer","minimum":0})),
            ("verified", json!({"type":"boolean"})),
        ]),
        _ => object_output(&[
            ("service", json!({"type":"string"})),
            ("action", json!({"type":"string"})),
            ("verified", json!({"type":"boolean"})),
        ]),
    }
}

fn descriptor(
    name: NativeToolName,
    description: &'static str,
    input_schema: Value,
    output_schema: Value,
    risk_level: RiskLevel,
    resource_impact: ResourceImpact,
) -> ToolDescriptor {
    ToolDescriptor {
        name,
        description,
        input_schema,
        output_schema,
        risk_level,
        mutability: Mutability::Read,
        requires_approval: false,
        supports_rollback: false,
        scope: ToolScope::Session,
        resource_impact,
        timeout_ms: 30_000,
    }
}

fn empty_input() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn object_input(properties: &[(&str, Value)]) -> Value {
    let properties = properties
        .iter()
        .map(|(name, schema)| ((*name).to_owned(), schema.clone()))
        .collect::<serde_json::Map<_, _>>();
    let required = properties.keys().cloned().collect::<Vec<_>>();
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

fn object_output(properties: &[(&str, Value)]) -> Value {
    let properties = properties
        .iter()
        .map(|(name, schema)| ((*name).to_owned(), schema.clone()))
        .collect::<serde_json::Map<_, _>>();
    let required = properties.keys().cloned().collect::<Vec<_>>();
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

fn generic_object_output() -> Value {
    json!({"type":"object","properties":{},"required":[]})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_marks_all_diagnostic_tools_as_read_only() {
        let descriptors = native_descriptors();
        assert_eq!(descriptors.len(), NativeToolName::ALL.len());
        assert_eq!(
            descriptors.iter().map(|item| item.name).collect::<Vec<_>>(),
            NativeToolName::ALL
        );
        assert!(descriptors
            .iter()
            .take(NativeToolName::READ_ONLY.len())
            .all(|item| {
                item.mutability == Mutability::Read
                    && !item.requires_approval
                    && !item.supports_rollback
                    && item.scope == ToolScope::Session
            }));
        assert!(descriptors
            .iter()
            .skip(NativeToolName::READ_ONLY.len())
            .all(|item| item.mutability == Mutability::Write && item.requires_approval));
    }

    #[test]
    fn descriptors_reject_unspecified_input_properties() {
        for descriptor in native_descriptors() {
            assert_eq!(
                descriptor.input_schema["additionalProperties"],
                Value::Bool(false),
                "{} input must remain closed",
                descriptor.name.as_str()
            );
        }
    }

    #[test]
    fn output_schemas_define_every_required_property() {
        for descriptor in native_descriptors() {
            let properties = descriptor.output_schema["properties"]
                .as_object()
                .expect("output properties");
            let required = descriptor.output_schema["required"]
                .as_array()
                .expect("required output properties");
            assert!(required.iter().all(|name| {
                name.as_str()
                    .is_some_and(|name| properties.contains_key(name))
            }));
        }
    }

    #[test]
    fn high_io_tools_carry_high_io_resource_impact() {
        let descriptors = native_descriptors();
        let directory = descriptors
            .iter()
            .find(|item| item.name == NativeToolName::SystemDirectoryUsage)
            .expect("directory usage descriptor");
        let disk = descriptors
            .iter()
            .find(|item| item.name == NativeToolName::SystemDisk)
            .expect("disk descriptor");
        assert_eq!(directory.resource_impact, ResourceImpact::HighIo);
        assert_eq!(disk.resource_impact, ResourceImpact::Low);
    }
}
