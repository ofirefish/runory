use std::sync::Arc;

use crate::domain::{
    AiAssistantResponse, AiCommandProposal, AiDiagnosis, AiPurpose, AiRisk, AiSignal, AiTask,
    AppError, AppResult, SessionId,
};
use crate::ssh::ServerSessionManager;

const MAX_COMMAND_BYTES: usize = 16 * 1024;
const MAX_ANALYSIS_BYTES: usize = 128 * 1024;

pub trait AiAssistantProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn explain(&self, command: &str) -> AppResult<AiAssistantResponse>;
    fn generate(&self, intent: &str) -> AppResult<AiAssistantResponse>;
    fn diagnose(&self, output: &str, propose_fixes: bool) -> AppResult<AiAssistantResponse>;
}

pub struct LocalAssistantProvider;

impl AiAssistantProvider for LocalAssistantProvider {
    fn name(&self) -> &'static str {
        "local-rules-v1"
    }

    fn explain(&self, command: &str) -> AppResult<AiAssistantResponse> {
        validate_text(command, MAX_COMMAND_BYTES)?;
        let normalized = command.trim();
        let purpose = command_purpose(normalized);
        let (risk, signals) = classify_command(normalized, purpose);
        Ok(response(
            self.name(),
            AiTask::ExplainCommand,
            risk,
            purpose,
            signals,
            None,
            vec![proposal(normalized, risk, purpose)],
        ))
    }

    fn generate(&self, intent: &str) -> AppResult<AiAssistantResponse> {
        validate_text(intent, MAX_COMMAND_BYTES)?;
        let lower = intent.to_lowercase();
        let generated = if contains_any(&lower, &["disk", "storage", "磁盘", "空间"]) {
            Some(("df -h", AiPurpose::DiskUsage))
        } else if contains_any(&lower, &["memory", "ram", "内存"]) {
            Some(("free -h", AiPurpose::MemoryUsage))
        } else if contains_any(&lower, &["cpu", "process", "进程", "负载"]) {
            Some(("ps aux --sort=-%cpu | head -n 20", AiPurpose::ProcessList))
        } else if contains_any(&lower, &["port", "socket", "listen", "端口", "监听"]) {
            Some(("ss -lntup", AiPurpose::NetworkSockets))
        } else if contains_any(&lower, &["docker", "container", "容器"]) {
            Some(("docker ps -a", AiPurpose::ContainerList))
        } else if contains_any(&lower, &["log", "journal", "日志"]) {
            Some(("journalctl -n 100 --no-pager", AiPurpose::ReadLogs))
        } else if contains_any(&lower, &["service", "systemd", "服务"]) {
            Some(("systemctl --failed --no-pager", AiPurpose::ServiceStatus))
        } else {
            None
        };
        let proposals = generated
            .map(|(command, purpose)| proposal(command, AiRisk::Low, purpose))
            .into_iter()
            .collect();
        let purpose = generated.map_or(AiPurpose::Unknown, |(_, purpose)| purpose);
        Ok(response(
            self.name(),
            AiTask::GenerateCommand,
            AiRisk::Low,
            purpose,
            if generated.is_some() {
                vec![AiSignal::ReadOnly]
            } else {
                Vec::new()
            },
            None,
            proposals,
        ))
    }

    fn diagnose(&self, output: &str, propose_fixes: bool) -> AppResult<AiAssistantResponse> {
        if output.len() > MAX_ANALYSIS_BYTES {
            return Err(AppError::InvalidOperation);
        }
        let lower = output.to_lowercase();
        let diagnosis = if contains_any(&lower, &["permission denied", "operation not permitted"]) {
            AiDiagnosis::PermissionDenied
        } else if contains_any(
            &lower,
            &["command not found", "not recognized as an internal"],
        ) {
            AiDiagnosis::CommandNotFound
        } else if contains_any(&lower, &["no space left on device", "disk quota exceeded"]) {
            AiDiagnosis::DiskFull
        } else if contains_any(
            &lower,
            &["address already in use", "port is already allocated"],
        ) {
            AiDiagnosis::PortInUse
        } else if contains_any(&lower, &["connection refused", "failed to connect"]) {
            AiDiagnosis::ConnectionRefused
        } else if contains_any(
            &lower,
            &["out of memory", "oom-kill", "cannot allocate memory"],
        ) {
            AiDiagnosis::OutOfMemory
        } else if contains_any(&lower, &["no such file or directory", "not found"]) {
            AiDiagnosis::ResourceNotFound
        } else if contains_any(
            &lower,
            &["authentication failed", "access denied", "unauthorized"],
        ) {
            AiDiagnosis::AuthenticationFailed
        } else if contains_any(&lower, &["timed out", "timeout"]) {
            AiDiagnosis::TimedOut
        } else {
            AiDiagnosis::Unknown
        };
        let proposals = if propose_fixes {
            fix_proposals(diagnosis)
        } else {
            Vec::new()
        };
        let purpose = proposals
            .first()
            .map_or(AiPurpose::Unknown, |value| value.purpose);
        let risk = proposals
            .iter()
            .map(|value| value.risk)
            .max_by_key(|risk| risk_rank(*risk))
            .unwrap_or(AiRisk::Low);
        Ok(response(
            self.name(),
            if propose_fixes {
                AiTask::ProposeFix
            } else {
                AiTask::DiagnoseOutput
            },
            risk,
            purpose,
            Vec::new(),
            Some(diagnosis),
            proposals,
        ))
    }
}

#[derive(Clone)]
pub struct AiService {
    provider: Arc<dyn AiAssistantProvider>,
}

impl AiService {
    pub fn new(provider: Arc<dyn AiAssistantProvider>) -> Self {
        Self { provider }
    }

    pub fn explain(&self, command: String) -> AppResult<AiAssistantResponse> {
        self.provider.explain(&command)
    }

    pub fn generate(&self, intent: String) -> AppResult<AiAssistantResponse> {
        self.provider.generate(&intent)
    }

    pub async fn diagnose(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        output: Option<String>,
        propose_fixes: bool,
    ) -> AppResult<AiAssistantResponse> {
        let (input, context_used) = match output.filter(|value| !value.trim().is_empty()) {
            Some(value) => (value, false),
            None => (sessions.recent_terminal_output(session_id).await?, true),
        };
        let mut result = self.provider.diagnose(&input, propose_fixes)?;
        result.context_used = context_used && !input.is_empty();
        Ok(result)
    }
}

fn response(
    provider: &'static str,
    task: AiTask,
    risk: AiRisk,
    purpose: AiPurpose,
    signals: Vec<AiSignal>,
    diagnosis: Option<AiDiagnosis>,
    proposals: Vec<AiCommandProposal>,
) -> AiAssistantResponse {
    AiAssistantResponse {
        task,
        provider,
        risk,
        purpose,
        signals,
        diagnosis,
        context_used: false,
        proposals,
    }
}

fn proposal(command: &str, risk: AiRisk, purpose: AiPurpose) -> AiCommandProposal {
    AiCommandProposal {
        command: command.to_owned(),
        risk,
        purpose,
        requires_confirmation: true,
    }
}

fn command_purpose(command: &str) -> AiPurpose {
    let lower = command.to_lowercase();
    let program = lower
        .split_whitespace()
        .find(|value| *value != "sudo")
        .unwrap_or_default();
    match program {
        "df" | "du" => AiPurpose::DiskUsage,
        "free" | "vmstat" => AiPurpose::MemoryUsage,
        "ps" | "top" | "htop" => AiPurpose::ProcessList,
        "ss" | "netstat" | "lsof" => AiPurpose::NetworkSockets,
        "journalctl" | "tail" | "less" | "cat" => AiPurpose::ReadLogs,
        "ls" | "pwd" | "find" | "stat" => AiPurpose::FileInspection,
        "docker" | "podman" => AiPurpose::ContainerList,
        "systemctl" | "service" | "nginx" => AiPurpose::ServiceStatus,
        "apt" | "apt-get" | "dnf" | "yum" | "pacman" => AiPurpose::PackageManagement,
        "rm" | "mv" | "cp" | "chmod" | "chown" | "mkdir" | "touch" => AiPurpose::FileMutation,
        "shutdown" | "reboot" | "poweroff" | "kill" | "killall" => AiPurpose::SystemControl,
        _ => AiPurpose::Unknown,
    }
}

fn classify_command(command: &str, purpose: AiPurpose) -> (AiRisk, Vec<AiSignal>) {
    let lower = command.to_lowercase();
    let mut signals = Vec::new();
    if lower.split_whitespace().any(|value| value == "sudo") {
        signals.push(AiSignal::ElevatedPrivileges);
    }
    if lower.contains('|') {
        signals.push(AiSignal::ShellPipeline);
    }
    if lower.contains('>') {
        signals.push(AiSignal::OutputRedirection);
    }
    if contains_any(&lower, &["curl ", "wget "]) {
        signals.push(AiSignal::NetworkDownload);
    }
    if matches!(purpose, AiPurpose::FileMutation) {
        signals.push(AiSignal::DestructiveFileOperation);
    }
    if contains_any(
        &lower,
        &["systemctl restart", "systemctl stop", "systemctl reload"],
    ) {
        signals.push(AiSignal::ServiceMutation);
    }
    if contains_any(
        &lower,
        &["docker rm", "docker stop", "docker restart", "docker prune"],
    ) {
        signals.push(AiSignal::ContainerMutation);
    }
    let risk = if contains_any(
        &lower,
        &[
            "rm -rf", "mkfs", "wipefs", "dd if=", "shutdown", "poweroff", "reboot", ":(){",
        ],
    ) {
        AiRisk::Critical
    } else if matches!(purpose, AiPurpose::FileMutation | AiPurpose::SystemControl)
        || signals.contains(&AiSignal::ElevatedPrivileges)
        || signals.contains(&AiSignal::ServiceMutation)
        || signals.contains(&AiSignal::ContainerMutation)
    {
        AiRisk::High
    } else if signals.contains(&AiSignal::OutputRedirection)
        || (signals.contains(&AiSignal::NetworkDownload)
            && signals.contains(&AiSignal::ShellPipeline))
        || matches!(purpose, AiPurpose::PackageManagement)
    {
        AiRisk::Medium
    } else {
        signals.push(AiSignal::ReadOnly);
        AiRisk::Low
    };
    (risk, signals)
}

fn fix_proposals(diagnosis: AiDiagnosis) -> Vec<AiCommandProposal> {
    match diagnosis {
        AiDiagnosis::PermissionDenied => vec![
            proposal("id", AiRisk::Low, AiPurpose::ServiceStatus),
            proposal("ls -ld .", AiRisk::Low, AiPurpose::FileInspection),
        ],
        AiDiagnosis::CommandNotFound => Vec::new(),
        AiDiagnosis::DiskFull => vec![
            proposal("df -h", AiRisk::Low, AiPurpose::DiskUsage),
            proposal(
                "du -x -h /var/log | sort -h | tail -n 20",
                AiRisk::Low,
                AiPurpose::DiskUsage,
            ),
        ],
        AiDiagnosis::PortInUse | AiDiagnosis::ConnectionRefused => vec![proposal(
            "ss -lntup",
            AiRisk::Low,
            AiPurpose::NetworkSockets,
        )],
        AiDiagnosis::OutOfMemory => vec![
            proposal("free -h", AiRisk::Low, AiPurpose::MemoryUsage),
            proposal(
                "ps aux --sort=-%mem | head -n 20",
                AiRisk::Low,
                AiPurpose::ProcessList,
            ),
        ],
        AiDiagnosis::ResourceNotFound => vec![proposal(
            "pwd && ls -la",
            AiRisk::Low,
            AiPurpose::FileInspection,
        )],
        AiDiagnosis::AuthenticationFailed | AiDiagnosis::TimedOut | AiDiagnosis::Unknown => {
            Vec::new()
        }
    }
}

fn validate_text(value: &str, max: usize) -> AppResult<()> {
    if value.trim().is_empty() || value.len() > max || value.contains('\0') {
        Err(AppError::InvalidOperation)
    } else {
        Ok(())
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

const fn risk_rank(risk: AiRisk) -> u8 {
    match risk {
        AiRisk::Low => 0,
        AiRisk::Medium => 1,
        AiRisk::High => 2,
        AiRisk::Critical => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destructive_commands_are_never_classified_as_low_risk() {
        let provider = LocalAssistantProvider;
        let result = provider.explain("sudo rm -rf /srv/app").expect("analysis");
        assert_eq!(result.risk, AiRisk::Critical);
        assert!(result.signals.contains(&AiSignal::ElevatedPrivileges));
        assert!(result.signals.contains(&AiSignal::DestructiveFileOperation));
        assert!(result.proposals[0].requires_confirmation);
    }

    #[test]
    fn generation_is_limited_to_known_read_only_templates() {
        let provider = LocalAssistantProvider;
        let result = provider
            .generate("show the top CPU processes")
            .expect("generation");
        assert_eq!(
            result.proposals[0].command,
            "ps aux --sort=-%cpu | head -n 20"
        );
        assert_eq!(result.proposals[0].risk, AiRisk::Low);
        assert!(provider
            .generate("delete everything")
            .expect("unknown")
            .proposals
            .is_empty());
    }

    #[test]
    fn diagnostics_return_machine_codes_and_safe_inspection_steps() {
        let provider = LocalAssistantProvider;
        let result = provider
            .diagnose("write failed: No space left on device", true)
            .expect("diagnosis");
        assert_eq!(result.diagnosis, Some(AiDiagnosis::DiskFull));
        assert!(result
            .proposals
            .iter()
            .all(|proposal| proposal.risk == AiRisk::Low));
    }
}
