use crate::dashboard::DashboardService;
use crate::domain::{
    AiTerminalPreset, AiToolInput, AiToolOutput, AppError, AppResult, NginxAction, ResourceAction,
    SessionId,
};
use crate::operations::OperationsService;
use crate::ssh::{RemoteCommand, ServerSessionManager};

pub struct AiToolExecutor;

impl AiToolExecutor {
    pub async fn execute(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        tool: AiToolInput,
    ) -> AppResult<AiToolOutput> {
        match tool {
            AiToolInput::TerminalExec { preset } => {
                let command = match preset {
                    AiTerminalPreset::DiskUsage => RemoteCommand::program("df", vec!["-h".into()]),
                    AiTerminalPreset::MemoryUsage => RemoteCommand::program("free", vec!["-h".into()]),
                    AiTerminalPreset::ListeningPorts => RemoteCommand::program("ss", vec!["-lntup".into()]),
                    AiTerminalPreset::RecentErrors => RemoteCommand::script(
                        "command -v journalctl >/dev/null 2>&1 || exit 90; journalctl -p err -n 100 --no-pager",
                        Vec::new(),
                    ),
                };
                let result = sessions.exec(session_id, command).await?;
                if result.exit_code == 90 {
                    return Err(AppError::UnsupportedRemote);
                }
                if result.exit_code != 0 {
                    return Err(AppError::ExecFailed);
                }
                Ok(AiToolOutput::Text {
                    value: combine(result.stdout, result.stderr),
                })
            }
            AiToolInput::FileRead { path } => {
                let (path, content) = sessions.sftp_read_text(session_id, path).await?;
                Ok(AiToolOutput::File { path, content })
            }
            AiToolInput::FileWrite { path, content } => {
                let (path, bytes) = sessions.sftp_write_text(session_id, path, content).await?;
                Ok(AiToolOutput::FileWritten { path, bytes })
            }
            AiToolInput::SystemMetrics => {
                let value = DashboardService::overview(sessions, session_id).await?;
                Ok(AiToolOutput::Metrics {
                    cpu_usage_percent: value.cpu_usage_percent,
                    memory_used_bytes: value.memory_used_bytes,
                    memory_total_bytes: value.memory_total_bytes,
                    uptime_seconds: value.uptime_seconds,
                    network_received_bytes: value.network_received_bytes,
                    network_transmitted_bytes: value.network_transmitted_bytes,
                    disks: value.disks,
                })
            }
            AiToolInput::ProcessList => Ok(AiToolOutput::Processes {
                processes: DashboardService::processes(sessions, session_id).await?,
            }),
            AiToolInput::DockerList => Ok(AiToolOutput::Containers {
                containers: OperationsService::docker_list(sessions, session_id).await?,
            }),
            AiToolInput::DockerRestart { container } => {
                let value = OperationsService::docker_action(
                    sessions,
                    session_id,
                    container,
                    ResourceAction::Restart,
                )
                .await?;
                if !value.success {
                    return Err(AppError::ExecFailed);
                }
                Ok(AiToolOutput::Text {
                    value: value.output,
                })
            }
            AiToolInput::NginxTest => operation(
                OperationsService::nginx_action(sessions, session_id, NginxAction::Test).await?,
            ),
            AiToolInput::NginxReload => operation(
                OperationsService::nginx_action(sessions, session_id, NginxAction::Reload).await?,
            ),
        }
    }
}

fn operation(value: crate::domain::OperationResult) -> AppResult<AiToolOutput> {
    if value.success {
        Ok(AiToolOutput::Text {
            value: value.output,
        })
    } else {
        Err(AppError::ExecFailed)
    }
}

fn combine(stdout: String, stderr: String) -> String {
    match (stdout.trim(), stderr.trim()) {
        ("", "") => String::new(),
        (stdout, "") => stdout.to_owned(),
        ("", stderr) => stderr.to_owned(),
        (stdout, stderr) => format!("{stdout}\n{stderr}"),
    }
}
