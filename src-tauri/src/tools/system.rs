use crate::dashboard::DashboardService;
use crate::domain::{AppError, AppResult, SessionId};
use crate::ssh::{RemoteCommand, ServerSessionManager};

use super::{SystemDiskData, SystemInfoData};

const SYSTEM_INFO_SCRIPT: &str = "printf 'HOSTNAME\\t%s\\n' \"$(uname -n)\"; printf 'OS\\t%s\\n' \"$(uname -s)\"; printf 'KERNEL\\t%s\\n' \"$(uname -r)\"; printf 'ARCH\\t%s\\n' \"$(uname -m)\"";

pub(super) async fn info(
    sessions: &ServerSessionManager,
    session_id: SessionId,
) -> AppResult<SystemInfoData> {
    let result = sessions
        .exec(
            session_id,
            RemoteCommand::script(SYSTEM_INFO_SCRIPT, Vec::new()).with_output_limit(16 * 1024),
        )
        .await?;
    if result.exit_code != 0 {
        return Err(AppError::ExecFailed);
    }
    parse_system_info(&result.stdout)
}

pub(super) async fn disk(
    sessions: &ServerSessionManager,
    session_id: SessionId,
) -> AppResult<SystemDiskData> {
    let dashboard = DashboardService::overview(sessions, session_id).await?;
    Ok(SystemDiskData {
        disks: dashboard.disks,
    })
}

fn parse_system_info(output: &str) -> AppResult<SystemInfoData> {
    let field = |name: &str| {
        output
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{name}\t")))
            .filter(|value| !value.trim().is_empty() && value.len() <= 4096)
            .map(str::to_owned)
            .ok_or(AppError::ExecFailed)
    };
    Ok(SystemInfoData {
        hostname: field("HOSTNAME")?,
        operating_system: field("OS")?,
        kernel_release: field("KERNEL")?,
        architecture: field("ARCH")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_structured_system_information() {
        let parsed =
            parse_system_info("HOSTNAME\trunory-test\nOS\tLinux\nKERNEL\t6.12.0\nARCH\tx86_64\n")
                .expect("parse system info");
        assert_eq!(parsed.hostname, "runory-test");
        assert_eq!(parsed.operating_system, "Linux");
        assert_eq!(parsed.kernel_release, "6.12.0");
        assert_eq!(parsed.architecture, "x86_64");
    }

    #[test]
    fn rejects_incomplete_system_information() {
        assert!(matches!(
            parse_system_info("HOSTNAME\trunory-test\nOS\tLinux\n"),
            Err(AppError::ExecFailed)
        ));
    }
}
