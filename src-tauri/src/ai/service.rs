use std::sync::Arc;

use crate::agentic::ModelGateway;
use crate::domain::{
    AiAssistantResponse, AiCommandProposal, AiDiagnosis, AiPlanProposal, AiPurpose, AiRisk,
    AiSignal, AiTask, AppError, AppResult, SessionId,
};
use crate::ssh::ServerSessionManager;

const MAX_COMMAND_BYTES: usize = 16 * 1024;
const MAX_ANALYSIS_BYTES: usize = 128 * 1024;
const MAX_PLAN_COMMANDS: usize = 16;

/// A single provider interview with the remote model. Every field is decoded
/// with `deny_unknown_fields` so a hallucinated or drifted schema is rejected
/// instead of being silently accepted by the recovery panel.
#[async_trait::async_trait]
pub trait AiAssistantProvider: Send + Sync {
    /// Provider identity shown on generated commands.
    fn name(&self) -> &'static str;
    fn explain(&self, command: &str) -> AppResult<AiAssistantResponse>;
    async fn generate(&self, intent: &str) -> AppResult<AiAssistantResponse>;
    /// Interview the model for a plan (or diagnosis + fixes). Async: the
    /// model HTTP round-trip runs directly on the Tokio runtime. It must NOT
    /// go through a nested `block_on`, which wedges the executor thread of an
    /// in-flight async command and leaves the panel stuck on "Selecting…".
    async fn diagnose(&self, output: &str, propose_fixes: bool) -> AppResult<AiAssistantResponse>;
    async fn plan(&self, intent: &str, exclude: &[String]) -> AppResult<AiPlanProposal>;
}

/// LLM-backed assistant. Command *generation*, *planning* and *diagnosis* are
/// delegated to the configured remote model; the local engine only
/// *classifies* the model's output (risk, purpose, signals) so the panel's
/// confirmation and gating stay deterministic. The legacy keyword-rules
/// provider was fully removed.
pub struct GatewayAssistantProvider {
    gateway: Arc<ModelGateway>,
}

impl GatewayAssistantProvider {
    pub fn new(gateway: Arc<ModelGateway>) -> Self {
        Self { gateway }
    }
}

#[async_trait::async_trait]
impl AiAssistantProvider for GatewayAssistantProvider {
    fn name(&self) -> &'static str {
        "llm-gateway-v1"
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

    async fn generate(&self, intent: &str) -> AppResult<AiAssistantResponse> {
        validate_text(intent, MAX_COMMAND_BYTES)?;
        let plan = self.complete_plan(intent, &[]).await?;
        let command = plan
            .commands
            .first()
            .map_or("", |value| value.command.as_str());
        let purpose = command_purpose(command);
        let (risk, signals) = classify_command(command, purpose);
        Ok(response(
            self.name(),
            AiTask::GenerateCommand,
            risk,
            purpose,
            signals,
            None,
            vec![proposal(command, risk, purpose)],
        ))
    }

    async fn diagnose(&self, output: &str, propose_fixes: bool) -> AppResult<AiAssistantResponse> {
        if output.len() > MAX_ANALYSIS_BYTES {
            return Err(AppError::InvalidOperation);
        }
        let plan = self
            .complete_plan(&diagnose_prompt(output, propose_fixes), &[])
            .await?;
        let diagnosis = diagnose_label(&plan.summary);
        let purpose = plan
            .commands
            .first()
            .map_or(AiPurpose::Unknown, |value| command_purpose(&value.command));
        let risk = plan
            .commands
            .iter()
            .map(|value| {
                let (risk, _) = classify_command(&value.command, command_purpose(&value.command));
                risk
            })
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
            plan.commands,
        ))
    }

    async fn plan(&self, intent: &str, exclude: &[String]) -> AppResult<AiPlanProposal> {
        validate_text(intent, MAX_COMMAND_BYTES)?;
        self.complete_plan(intent, exclude).await
    }
}

impl GatewayAssistantProvider {
    /// Interview the model and return a bounded, classified plan. `exclude`
    /// lists commands already settled in this conversation; the prompt orders
    /// the model not to repeat them. Runs the HTTP round-trip on the Tokio
    /// runtime directly — never wrapped in a nested `block_on`.
    async fn complete_plan(&self, prompt: &str, exclude: &[String]) -> AppResult<AiPlanProposal> {
        let plan = self.gateway.complete_plan(prompt, exclude).await?;
        validate_plan(&plan)
    }
}

/// Thin facade owned by Tauri state; commands resolve it by type. All actual
/// behavior lives in the provider trait implementation.
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

    pub async fn generate(&self, intent: String) -> AppResult<AiAssistantResponse> {
        self.provider.generate(&intent).await
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
        let mut result = self.provider.diagnose(&input, propose_fixes).await?;
        result.context_used = context_used && !input.is_empty();
        Ok(result)
    }
}

/// Validate a raw model plan against the hard limits:
/// - at most `MAX_PLAN_COMMANDS` commands,
/// - blank / oversized / devious commands rejected,
/// - the summary trimmed to the display size.
/// The prompt already demands no sudo; any sudo command that still slips
/// through is downgraded: it is kept as evidence but must be confirmed.
fn validate_plan(plan: &AiPlanProposal) -> AppResult<AiPlanProposal> {
    if plan.commands.len() > MAX_PLAN_COMMANDS {
        return Err(AppError::ModelResponseInvalid);
    }
    let mut commands = Vec::with_capacity(plan.commands.len());
    for raw in &plan.commands {
        let command = raw.command.trim();
        if command.is_empty()
            || command.len() > MAX_COMMAND_BYTES
            || command.contains('\0')
            || command.lines().count() > 1
            || command.split_whitespace().next().is_none()
        {
            return Err(AppError::ModelResponseInvalid);
        }
        let purpose = raw.purpose;
        let (risk, signals) = classify_command(command, purpose);
        // The model is never trusted on safety: force a confirmation flag on
        // anything that is not pure read-only.
        let requires_confirmation = risk != AiRisk::Low
            || signals.contains(&AiSignal::ElevatedPrivileges)
            || signals.contains(&AiSignal::ServiceMutation)
            || signals.contains(&AiSignal::ContainerMutation)
            || signals.contains(&AiSignal::DestructiveFileOperation);
        commands.push(AiCommandProposal {
            command: command.to_owned(),
            risk,
            purpose,
            requires_confirmation,
        });
    }
    Ok(AiPlanProposal {
        summary: plan.summary.trim().chars().take(512).collect(),
        commands,
    })
}

fn diagnose_prompt(output: &str, propose_fixes: bool) -> String {
    if propose_fixes {
        format!(
            "Analyze the following terminal output and produce a plan: state the diagnosis \
             concisely in `summary`, then propose minimal diagnostic commands in `commands` \
             (each with purpose and risk). If the problem is clear, include a small number of \
             fix commands with risk above low and mark them clearly. Commands run in the user's \
             live terminal under the logged-in user; never assume sudo, never prefix with sudo \
             unless the user explicitly asked for elevated access.\n\nOutput:\n{output}"
        )
    } else {
        format!(
            "Analyze the following terminal output and produce a JSON `summary` that identifies \
             the root cause. Do not invent facts not present in the output.\n\nOutput:\n{output}"
        )
    }
}

/// Same keyword classification used to map a model-provided diagnosis summary
/// onto the stable machine label consumed by the rest of the UI.
fn classify_diagnosis(lower: &str) -> AiDiagnosis {
    if contains_any(lower, &["permission denied", "operation not permitted"]) {
        AiDiagnosis::PermissionDenied
    } else if contains_any(
        lower,
        &["command not found", "not recognized as an internal"],
    ) {
        AiDiagnosis::CommandNotFound
    } else if contains_any(
        lower,
        &[
            "no space left on device",
            "disk quota exceeded",
            "disk full",
        ],
    ) {
        AiDiagnosis::DiskFull
    } else if contains_any(
        lower,
        &["address already in use", "port is already allocated"],
    ) {
        AiDiagnosis::PortInUse
    } else if contains_any(lower, &["connection refused", "failed to connect"]) {
        AiDiagnosis::ConnectionRefused
    } else if contains_any(
        lower,
        &["out of memory", "oom-kill", "cannot allocate memory"],
    ) {
        AiDiagnosis::OutOfMemory
    } else if contains_any(lower, &["no such file or directory", "not found"]) {
        AiDiagnosis::ResourceNotFound
    } else if contains_any(
        lower,
        &["authentication failed", "access denied", "unauthorized"],
    ) {
        AiDiagnosis::AuthenticationFailed
    } else if contains_any(lower, &["timed out", "timeout"]) {
        AiDiagnosis::TimedOut
    } else {
        AiDiagnosis::Unknown
    }
}

/// Map the model's natural-language diagnosis summary onto the stable
/// machine label used by the rest of the UI.
fn diagnose_label(summary: &str) -> AiDiagnosis {
    if summary.trim().is_empty() {
        return AiDiagnosis::Unknown;
    }
    classify_diagnosis(&summary.to_lowercase())
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
        requires_confirmation: risk != AiRisk::Low,
    }
}

pub(crate) fn command_purpose(command: &str) -> AiPurpose {
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

pub(crate) fn classify_command(command: &str, purpose: AiPurpose) -> (AiRisk, Vec<AiSignal>) {
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
        let purpose = command_purpose("sudo rm -rf /srv/app");
        let (risk, signals) = classify_command("sudo rm -rf /srv/app", purpose);
        assert_eq!(risk, AiRisk::Critical);
        assert!(signals.contains(&AiSignal::ElevatedPrivileges));
        assert!(signals.contains(&AiSignal::DestructiveFileOperation));
    }

    #[test]
    fn read_only_probes_are_low_risk_and_do_not_require_confirmation() {
        let purpose = command_purpose("df -h");
        let (risk, signals) = classify_command("df -h", purpose);
        assert_eq!(risk, AiRisk::Low);
        assert!(signals.contains(&AiSignal::ReadOnly));
        assert!(!proposal("df -h", risk, purpose).requires_confirmation);
    }

    #[test]
    fn validate_plan_rejects_multiline_commands_and_oversized_plans() {
        let bad = AiPlanProposal {
            summary: "x".into(),
            commands: vec![AiCommandProposal {
                command: "echo a\nrm -rf /".into(),
                risk: AiRisk::Low,
                purpose: AiPurpose::Unknown,
                requires_confirmation: false,
            }],
        };
        assert!(validate_plan(&bad).is_err());
        let too_many = AiPlanProposal {
            summary: "x".into(),
            commands: (0..=MAX_PLAN_COMMANDS)
                .map(|_| AiCommandProposal {
                    command: "ls".into(),
                    risk: AiRisk::Low,
                    purpose: AiPurpose::FileInspection,
                    requires_confirmation: false,
                })
                .collect(),
        };
        assert!(validate_plan(&too_many).is_err());
    }

    #[test]
    fn sudo_commands_always_require_confirmation_even_if_the_model_omits_it() {
        let plan = AiPlanProposal {
            summary: "x".into(),
            commands: vec![AiCommandProposal {
                command: "sudo systemctl restart nginx".into(),
                risk: AiRisk::Low,
                purpose: AiPurpose::ServiceStatus,
                requires_confirmation: false,
            }],
        };
        let validated = validate_plan(&plan).expect("validated plan");
        assert_eq!(validated.commands[0].risk, AiRisk::High);
        assert!(validated.commands[0].requires_confirmation);
    }

    #[test]
    fn diagnosis_labels_are_stable_machine_codes() {
        assert_eq!(
            diagnose_label("No space left on device on /dev/sda1"),
            AiDiagnosis::DiskFull
        );
        assert_eq!(
            diagnose_label("connection refused while connecting to 10.0.0.5"),
            AiDiagnosis::ConnectionRefused
        );
        assert_eq!(diagnose_label(""), AiDiagnosis::Unknown);
    }
}
