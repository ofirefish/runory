use std::collections::HashSet;

use crate::domain::{AiPlanRequest, AiRisk, AiToolInput, AiToolSummary, AppError, AppResult};

const MAX_GOAL_BYTES: usize = 4096;
const MAX_SESSIONS: usize = 10;
const MAX_TOOLS: usize = 12;
const MAX_FILE_BYTES: usize = 512 * 1024;

pub fn validate_plan(request: &AiPlanRequest) -> AppResult<()> {
    if request.goal.trim().is_empty()
        || request.goal.len() > MAX_GOAL_BYTES
        || request.session_ids.is_empty()
        || request.session_ids.len() > MAX_SESSIONS
        || request.tools.is_empty()
        || request.tools.len() > MAX_TOOLS
    {
        return Err(AppError::InvalidOperation);
    }
    let unique = request.session_ids.iter().copied().collect::<HashSet<_>>();
    if unique.len() != request.session_ids.len() {
        return Err(AppError::InvalidOperation);
    }
    request.tools.iter().try_for_each(validate_tool)
}

pub const fn risk(tool: &AiToolInput) -> AiRisk {
    match tool {
        AiToolInput::SystemMetrics
        | AiToolInput::ProcessList
        | AiToolInput::DockerList
        | AiToolInput::NginxTest
        | AiToolInput::FileRead { .. }
        | AiToolInput::TerminalExec { .. } => AiRisk::Low,
        AiToolInput::DockerRestart { .. } | AiToolInput::NginxReload => AiRisk::High,
        AiToolInput::FileWrite { .. } => AiRisk::Critical,
    }
}

pub fn audit_target(tool: &AiToolInput) -> Option<String> {
    match tool {
        AiToolInput::FileRead { path } | AiToolInput::FileWrite { path, .. } => Some(path.clone()),
        AiToolInput::DockerRestart { container } => Some(container.clone()),
        AiToolInput::TerminalExec { preset } => Some(format!("{preset:?}")),
        _ => None,
    }
}

pub fn summary(tool: &AiToolInput) -> AiToolSummary {
    match tool {
        AiToolInput::TerminalExec { preset } => AiToolSummary::TerminalExec { preset: *preset },
        AiToolInput::FileRead { path } => AiToolSummary::FileRead { path: path.clone() },
        AiToolInput::FileWrite { path, content } => AiToolSummary::FileWrite {
            path: path.clone(),
            bytes: content.len() as u64,
        },
        AiToolInput::SystemMetrics => AiToolSummary::SystemMetrics,
        AiToolInput::ProcessList => AiToolSummary::ProcessList,
        AiToolInput::DockerList => AiToolSummary::DockerList,
        AiToolInput::DockerRestart { container } => AiToolSummary::DockerRestart {
            container: container.clone(),
        },
        AiToolInput::NginxTest => AiToolSummary::NginxTest,
        AiToolInput::NginxReload => AiToolSummary::NginxReload,
    }
}

fn validate_tool(tool: &AiToolInput) -> AppResult<()> {
    match tool {
        AiToolInput::FileRead { path } => validate_path(path),
        AiToolInput::FileWrite { path, content } => {
            validate_path(path)?;
            if content.len() > MAX_FILE_BYTES || content.contains('\0') {
                return Err(AppError::InvalidOperation);
            }
            Ok(())
        }
        AiToolInput::DockerRestart { container } => validate_identifier(container),
        _ => Ok(()),
    }
}

fn validate_path(path: &str) -> AppResult<()> {
    if path.is_empty() || path.len() > 4096 || path.contains('\0') {
        Err(AppError::InvalidOperation)
    } else {
        Ok(())
    }
}

fn validate_identifier(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 256
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'@' | b':' | b'/')
        })
    {
        Err(AppError::InvalidOperation)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::AiTerminalPreset;
    use uuid::Uuid;

    #[test]
    fn policy_assigns_mutations_high_or_critical_risk() {
        assert_eq!(
            risk(&AiToolInput::DockerRestart {
                container: "web".into()
            }),
            AiRisk::High
        );
        assert_eq!(
            risk(&AiToolInput::FileWrite {
                path: "/tmp/a".into(),
                content: "x".into()
            }),
            AiRisk::Critical
        );
        assert_eq!(
            risk(&AiToolInput::TerminalExec {
                preset: AiTerminalPreset::DiskUsage
            }),
            AiRisk::Low
        );
    }

    #[test]
    fn plan_rejects_duplicate_sessions_and_unsafe_identifiers() {
        let session = Uuid::new_v4();
        let request = AiPlanRequest {
            session_ids: vec![session, session],
            goal: "restart web".into(),
            tools: vec![AiToolInput::DockerRestart {
                container: "web; reboot".into(),
            }],
        };
        assert!(matches!(
            validate_plan(&request),
            Err(AppError::InvalidOperation)
        ));
    }

    #[test]
    fn file_write_summary_never_contains_content() {
        let value = serde_json::to_string(&summary(&AiToolInput::FileWrite {
            path: "/tmp/config".into(),
            content: "secret-value".into(),
        }))
        .expect("serialize summary");
        assert!(value.contains("/tmp/config"));
        assert!(!value.contains("secret-value"));
    }
}
