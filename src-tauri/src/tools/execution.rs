use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::sync::watch;
use uuid::Uuid;

use crate::domain::{AppError, AppResult, SessionId};
use crate::policy::{
    AgentPolicyService, PolicyEvaluationRequest, PolicyExecutionStrategy, PolicyInvocationSource,
    PolicyTarget,
};
use crate::ssh::ServerSessionManager;

use super::registry::NativeToolRegistry;
use super::{
    NativeToolInvocation, SanitizedToolInput, ToolApprovalAudit, ToolAuditRecord,
    ToolAuditRecorder, ToolAuditRepository, ToolAuditStatus, ToolDescriptor, ToolPolicy,
    ToolPolicyDecision, ToolResult, ToolRollbackAudit, ToolVerificationAudit,
};

#[derive(Clone, Debug)]
pub(crate) struct NativeToolRequest {
    request_id: Uuid,
    invocation_id: Uuid,
    session_id: SessionId,
    invocation: NativeToolInvocation,
    cancellation: Option<watch::Receiver<bool>>,
    agent_run_id: Option<Uuid>,
    authority: Option<ToolExecutionAuthority>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ToolExecutionAuthority {
    pub agent_run_id: Uuid,
    pub change_set_id: Uuid,
    pub change_set_version: u64,
    pub rollback: bool,
}

impl NativeToolRequest {
    pub(crate) fn new(session_id: SessionId, invocation: NativeToolInvocation) -> Self {
        Self {
            request_id: Uuid::new_v4(),
            invocation_id: Uuid::new_v4(),
            session_id,
            invocation,
            cancellation: None,
            agent_run_id: None,
            authority: None,
        }
    }

    pub(crate) fn cancellable(
        session_id: SessionId,
        invocation: NativeToolInvocation,
    ) -> (Self, NativeToolCancellationHandle) {
        let invocation_id = Uuid::new_v4();
        let (sender, receiver) = watch::channel(false);
        (
            Self {
                request_id: Uuid::new_v4(),
                invocation_id,
                session_id,
                invocation,
                cancellation: Some(receiver),
                agent_run_id: None,
                authority: None,
            },
            NativeToolCancellationHandle {
                invocation_id,
                sender,
            },
        )
    }

    pub(crate) fn with_cancellation(
        session_id: SessionId,
        invocation: NativeToolInvocation,
        cancellation: watch::Receiver<bool>,
    ) -> Self {
        Self {
            request_id: Uuid::new_v4(),
            invocation_id: Uuid::new_v4(),
            session_id,
            invocation,
            cancellation: Some(cancellation),
            agent_run_id: None,
            authority: None,
        }
    }

    pub(crate) fn with_cancellation_for_agent(
        agent_run_id: Uuid,
        session_id: SessionId,
        invocation: NativeToolInvocation,
        cancellation: watch::Receiver<bool>,
    ) -> Self {
        let mut request = Self::with_cancellation(session_id, invocation, cancellation);
        request.agent_run_id = Some(agent_run_id);
        request
    }

    pub(crate) fn approved(
        session_id: SessionId,
        invocation: NativeToolInvocation,
        authority: ToolExecutionAuthority,
    ) -> Self {
        Self {
            request_id: Uuid::new_v4(),
            invocation_id: Uuid::new_v4(),
            session_id,
            invocation,
            cancellation: None,
            agent_run_id: Some(authority.agent_run_id),
            authority: Some(authority),
        }
    }

    pub(crate) fn approved_rollback(
        session_id: SessionId,
        invocation: NativeToolInvocation,
        mut authority: ToolExecutionAuthority,
    ) -> Self {
        authority.rollback = true;
        Self::approved(session_id, invocation, authority)
    }

    pub(crate) const fn invocation_id(&self) -> Uuid {
        self.invocation_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ToolCancellationStatus {
    Requested,
    AlreadyRequested,
    InvocationFinished,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeToolCancellationHandle {
    invocation_id: Uuid,
    sender: watch::Sender<bool>,
}

impl NativeToolCancellationHandle {
    pub(crate) const fn invocation_id(&self) -> Uuid {
        self.invocation_id
    }

    pub(crate) fn cancel(&self) -> ToolCancellationStatus {
        if *self.sender.borrow() {
            ToolCancellationStatus::AlreadyRequested
        } else if self.sender.send(true).is_ok() {
            ToolCancellationStatus::Requested
        } else {
            ToolCancellationStatus::InvocationFinished
        }
    }
}

pub(crate) struct NativeToolExecutionService {
    registry: NativeToolRegistry,
    policy: ToolPolicy,
    audit: ToolAuditRecorder,
    agent_policy: AgentPolicyService,
}

impl NativeToolExecutionService {
    pub(crate) fn foundation(audit_repository: ToolAuditRepository) -> Self {
        Self::with_policy(audit_repository, ToolPolicy::foundation())
    }

    pub(crate) fn approved_repair(audit_repository: ToolAuditRepository) -> Self {
        Self::with_policy(audit_repository, ToolPolicy::approved_repair())
    }

    pub(crate) fn with_policy(audit_repository: ToolAuditRepository, policy: ToolPolicy) -> Self {
        Self::with_agent_policy(audit_repository, policy, AgentPolicyService::default())
    }

    pub(crate) fn approved_repair_with_agent_policy(
        audit_repository: ToolAuditRepository,
        agent_policy: AgentPolicyService,
    ) -> Self {
        Self::with_agent_policy(
            audit_repository,
            ToolPolicy::approved_repair(),
            agent_policy,
        )
    }

    fn with_agent_policy(
        audit_repository: ToolAuditRepository,
        policy: ToolPolicy,
        agent_policy: AgentPolicyService,
    ) -> Self {
        Self {
            registry: NativeToolRegistry::new(),
            policy,
            audit: ToolAuditRecorder::new(audit_repository),
            agent_policy,
        }
    }

    pub(crate) fn descriptors(&self) -> Vec<&ToolDescriptor> {
        self.registry.descriptors()
    }

    pub(crate) async fn execute(
        &self,
        sessions: &ServerSessionManager,
        request: NativeToolRequest,
    ) -> AppResult<ToolResult> {
        let NativeToolRequest {
            request_id,
            invocation_id,
            session_id,
            invocation,
            mut cancellation,
            agent_run_id,
            authority,
        } = request;
        let started_at_epoch_ms = now_epoch_ms();
        let started = Instant::now();
        let tool_name = invocation.name();
        let descriptor = self
            .registry
            .descriptor(tool_name)
            .cloned()
            .ok_or(AppError::InvalidOperation)?;
        let sanitized_input = invocation.sanitized_input();
        let policy_server_id = sessions.profile_id(session_id).await.unwrap_or(session_id);
        // Policy is evaluated before the immutable registry/risk gate. Remote data, Skills and
        // MCP output are never inputs to this deterministic request.
        let policy_evaluation = self
            .agent_policy
            .evaluate(&PolicyEvaluationRequest {
                target: PolicyTarget {
                    server_id: policy_server_id,
                    group_id: None,
                    environment: None,
                },
                tool: tool_name,
                risk_level: descriptor.risk_level,
                resource_impact: descriptor.resource_impact,
                target_count: 1,
                execution_strategy: PolicyExecutionStrategy::Sequential,
                source: PolicyInvocationSource::NativeTool,
            })
            .await?;
        let mut decision = if policy_evaluation.permits_execution() {
            self.policy.evaluate(&descriptor)
        } else {
            ToolPolicyDecision::Blocked {
                reason: super::policy::ToolPolicyDenial::AgentPolicyDenied,
            }
        };
        if descriptor.requires_approval && authority.is_none() {
            decision = ToolPolicyDecision::Blocked {
                reason: super::policy::ToolPolicyDenial::ApprovalEngineUnavailable,
            };
        }
        let initial_status = match decision {
            ToolPolicyDecision::Allowed => ToolAuditStatus::InProgress,
            ToolPolicyDecision::Blocked { .. } => ToolAuditStatus::PolicyBlocked,
        };
        let approval = if descriptor.requires_approval && authority.is_some() {
            ToolApprovalAudit::Approved
        } else if descriptor.requires_approval {
            ToolApprovalAudit::Unavailable
        } else {
            ToolApprovalAudit::NotRequired
        };
        let blocked_duration_ms =
            matches!(decision, ToolPolicyDecision::Blocked { .. }).then(|| elapsed_ms(started));
        let audited_agent_run_id = authority.map(|value| value.agent_run_id).or(agent_run_id);
        self.audit
            .append(ToolAuditRecord {
                id: Uuid::new_v4(),
                request_id,
                invocation_id,
                agent_run_id: audited_agent_run_id,
                model: audited_agent_run_id.map(|_| "runory-local-doctor-v1".to_owned()),
                change_set_id: authority.map(|value| value.change_set_id),
                change_set_version: authority.map(|value| value.change_set_version),
                profile_id: None,
                session_id,
                tool_name,
                sanitized_input,
                risk_level: descriptor.risk_level,
                mutability: descriptor.mutability,
                scope: descriptor.scope,
                policy_decision: decision,
                approval,
                status: initial_status,
                verification: if descriptor.mutability == super::Mutability::Write {
                    ToolVerificationAudit::Pending
                } else {
                    ToolVerificationAudit::NotApplicable
                },
                rollback: if authority.is_some_and(|value| value.rollback) {
                    ToolRollbackAudit::Executed
                } else if descriptor.supports_rollback {
                    ToolRollbackAudit::Available
                } else {
                    ToolRollbackAudit::NotSupported
                },
                started_at_epoch_ms,
                completed_at_epoch_ms: if matches!(decision, ToolPolicyDecision::Blocked { .. }) {
                    Some(now_epoch_ms())
                } else {
                    None
                },
                cancellation_requested_at_epoch_ms: None,
                duration_ms: blocked_duration_ms,
                succeeded: if matches!(decision, ToolPolicyDecision::Blocked { .. }) {
                    Some(false)
                } else {
                    None
                },
                error_code: match decision {
                    ToolPolicyDecision::Allowed => None,
                    ToolPolicyDecision::Blocked { reason } => Some(reason.code().to_owned()),
                },
                truncated: false,
            })
            .await?;

        if let ToolPolicyDecision::Blocked { reason } = decision {
            return Ok(ToolResult::failure(
                invocation_id,
                tool_name,
                reason.code(),
                started_at_epoch_ms,
                blocked_duration_ms.unwrap_or_default(),
            ));
        }

        if cancellation
            .as_ref()
            .is_some_and(|receiver| *receiver.borrow())
        {
            let result = ToolResult::cancelled(
                invocation_id,
                tool_name,
                started_at_epoch_ms,
                elapsed_ms(started),
            );
            self.audit
                .mark_cancellation_requested(invocation_id, now_epoch_ms())
                .await?;
            self.audit
                .complete(invocation_id, None, &result, now_epoch_ms())
                .await?;
            return Ok(result);
        }

        let profile_id = match sessions.profile_id(session_id).await {
            Ok(profile_id) => Some(profile_id),
            Err(error) => {
                let result = ToolResult::failure(
                    invocation_id,
                    tool_name,
                    error.code(),
                    started_at_epoch_ms,
                    elapsed_ms(started),
                );
                self.audit
                    .complete(invocation_id, None, &result, now_epoch_ms())
                    .await?;
                return Ok(result);
            }
        };
        let execution = self
            .registry
            .execute(sessions, session_id, invocation_id, invocation);
        tokio::pin!(execution);
        let timeout = tokio::time::sleep(Duration::from_millis(descriptor.timeout_ms));
        tokio::pin!(timeout);
        let result = if let Some(receiver) = cancellation.as_mut() {
            tokio::select! {
                result = &mut execution => result,
                _ = &mut timeout => ToolResult::failure(
                    invocation_id,
                    tool_name,
                    "EXEC_TIMED_OUT",
                    started_at_epoch_ms,
                    elapsed_ms(started),
                ),
                _ = wait_for_cancellation(receiver) => {
                    self.audit
                        .mark_cancellation_requested(invocation_id, now_epoch_ms())
                        .await?;
                    ToolResult::cancelled(
                        invocation_id,
                        tool_name,
                        started_at_epoch_ms,
                        elapsed_ms(started),
                    )
                },
            }
        } else {
            tokio::select! {
                result = &mut execution => result,
                _ = &mut timeout => ToolResult::failure(
                    invocation_id,
                    tool_name,
                    "EXEC_TIMED_OUT",
                    started_at_epoch_ms,
                    elapsed_ms(started),
                ),
            }
        };
        self.audit
            .complete(invocation_id, profile_id, &result, now_epoch_ms())
            .await?;
        Ok(result)
    }

    pub(crate) async fn audit(&self) -> AppResult<Vec<ToolAuditRecord>> {
        self.audit.list().await
    }
}

async fn wait_for_cancellation(receiver: &mut watch::Receiver<bool>) {
    loop {
        if *receiver.borrow() {
            return;
        }
        if receiver.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

impl NativeToolInvocation {
    fn sanitized_input(&self) -> SanitizedToolInput {
        match self {
            Self::SystemInfo
            | Self::SystemDisk
            | Self::NginxTest
            | Self::NginxReload
            | Self::NetworkListeners
            | Self::ProcessList
            | Self::DockerList
            | Self::FilesystemInodeUsage
            | Self::BlockDevicesList => SanitizedToolInput::None,
            Self::ServiceStatus { service } => SanitizedToolInput::Service {
                name: sanitized_identifier(service, 128),
                lines: None,
            },
            Self::ServiceLogs { service, lines } => SanitizedToolInput::Service {
                name: sanitized_identifier(service, 128),
                lines: Some(*lines),
            },
            Self::NetworkPortCheck { host, port } | Self::TlsInspect { host, port } => {
                SanitizedToolInput::NetworkEndpoint {
                    host: sanitized_host(host),
                    port: *port,
                }
            }
            Self::DnsResolve { host } => SanitizedToolInput::NetworkEndpoint {
                host: sanitized_host(host),
                port: 0,
            },
            Self::HttpRequest { url } => SanitizedToolInput::HttpOrigin {
                origin: sanitized_http_origin(url),
            },
            Self::FilePatch { path, .. }
            | Self::FileInspect { path }
            | Self::SystemDirectoryUsage { path }
            | Self::SystemLargeFiles { path, .. } => SanitizedToolInput::RemotePath {
                path: sanitized_remote_path(path),
            },
            Self::DockerInspect { container } => SanitizedToolInput::Service {
                name: sanitized_identifier(container, 256),
                lines: None,
            },
            Self::DockerRestart { container } => SanitizedToolInput::Service {
                name: sanitized_identifier(container, 256),
                lines: None,
            },
            Self::DockerLogs { container, lines } => SanitizedToolInput::Service {
                name: sanitized_identifier(container, 256),
                lines: Some(*lines),
            },
            Self::ServiceRestart { service } | Self::ServiceReload { service } => {
                SanitizedToolInput::Service {
                    name: sanitized_identifier(service, 128),
                    lines: None,
                }
            }
            Self::TerminalExecReadonly { command } => SanitizedToolInput::HttpOrigin {
                origin: (command.len() <= 256 && !command.chars().any(char::is_control))
                    .then(|| command.to_owned()),
            },
        }
    }
}

fn sanitized_remote_path(path: &str) -> Option<String> {
    (!path.is_empty() && path.len() <= 4096 && !path.chars().any(char::is_control))
        .then(|| path.to_owned())
}

fn sanitized_identifier(value: &str, max_bytes: usize) -> Option<String> {
    (!value.is_empty()
        && value.len() <= max_bytes
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'@' | b':' | b'-')
        }))
    .then(|| value.to_owned())
}

fn sanitized_host(host: &str) -> Option<String> {
    (!host.is_empty()
        && host.len() <= 255
        && !host.starts_with('-')
        && !host
            .chars()
            .any(|character| character.is_whitespace() || character.is_control()))
    .then(|| host.to_owned())
}

fn sanitized_http_origin(url: &str) -> Option<String> {
    if url.len() > 2048
        || url.contains('@')
        || url
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return None;
    }
    let (scheme, remainder) = url
        .strip_prefix("http://")
        .map(|value| ("http", value))
        .or_else(|| url.strip_prefix("https://").map(|value| ("https", value)))?;
    let authority = remainder.split(['/', '?', '#']).next()?;
    if authority.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{authority}"))
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::JsonRepository;
    use crate::tools::NativeToolName;

    fn service(policy: ToolPolicy) -> (tempfile::TempDir, NativeToolExecutionService) {
        let directory = tempfile::tempdir().expect("temporary directory");
        let repository = ToolAuditRepository::new(JsonRepository::new(
            directory.path().join("tool-audit.json"),
        ));
        (
            directory,
            NativeToolExecutionService::with_policy(repository, policy),
        )
    }

    #[tokio::test]
    async fn policy_block_is_structured_audited_and_does_not_need_a_session() {
        let (_directory, service) = service(ToolPolicy::restricted(
            NativeToolName::ALL,
            super::super::RiskLevel::R0,
        ));
        let result = service
            .execute(
                &ServerSessionManager::default(),
                NativeToolRequest::new(
                    Uuid::new_v4(),
                    NativeToolInvocation::HttpRequest {
                        url: "https://example.com/path?token=secret".into(),
                    },
                ),
            )
            .await
            .expect("policy result");
        assert!(!result.success);
        assert_eq!(result.error_code, Some("TOOL_RISK_CEILING_EXCEEDED"));
        let audit = service.audit().await.expect("audit");
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].status, ToolAuditStatus::PolicyBlocked);
        assert_eq!(
            audit[0].sanitized_input,
            SanitizedToolInput::HttpOrigin {
                origin: Some("https://example.com".into())
            }
        );
        let serialized = serde_json::to_string(&audit).expect("serialize audit");
        assert!(!serialized.contains("token=secret"));
    }

    #[test]
    fn invalid_dynamic_inputs_are_not_copied_into_audit_metadata() {
        let secret = "sshd; token=secret";
        assert_eq!(
            NativeToolInvocation::ServiceStatus {
                service: secret.into()
            }
            .sanitized_input(),
            SanitizedToolInput::Service {
                name: None,
                lines: None
            }
        );
        let serialized = serde_json::to_string(
            &NativeToolInvocation::NetworkPortCheck {
                host: "host\nAuthorization: secret".into(),
                port: 22,
            }
            .sanitized_input(),
        )
        .expect("serialize sanitized input");
        assert!(!serialized.contains("Authorization"));
        assert!(!serialized.contains("secret"));
    }

    #[tokio::test]
    async fn missing_session_failure_is_structured_and_finalizes_audit() {
        let (_directory, service) = service(ToolPolicy::foundation());
        let (request, cancellation) =
            NativeToolRequest::cancellable(Uuid::new_v4(), NativeToolInvocation::SystemInfo);
        let result = service
            .execute(&ServerSessionManager::default(), request)
            .await
            .expect("structured result");
        assert!(!result.success);
        assert_eq!(result.error_code, Some("SESSION_NOT_FOUND"));
        assert_eq!(
            cancellation.cancel(),
            ToolCancellationStatus::InvocationFinished
        );
        let audit = service.audit().await.expect("audit");
        assert_eq!(audit[0].status, ToolAuditStatus::Failed);
        assert_eq!(audit[0].error_code.as_deref(), Some("SESSION_NOT_FOUND"));
    }

    #[tokio::test]
    async fn agent_tool_audit_binds_run_and_model() {
        let (_directory, service) = service(ToolPolicy::foundation());
        let agent_run_id = Uuid::new_v4();
        let (_sender, receiver) = watch::channel(false);
        let request = NativeToolRequest::with_cancellation_for_agent(
            agent_run_id,
            Uuid::new_v4(),
            NativeToolInvocation::SystemInfo,
            receiver,
        );
        service
            .execute(&ServerSessionManager::default(), request)
            .await
            .expect("structured result");
        let audit = service.audit().await.expect("audit");
        assert_eq!(audit[0].agent_run_id, Some(agent_run_id));
        assert_eq!(audit[0].model.as_deref(), Some("runory-local-doctor-v1"));
    }

    #[tokio::test]
    async fn cancellation_is_bound_to_invocation_and_finalized_in_audit() {
        let (_directory, service) = service(ToolPolicy::foundation());
        let (request, cancellation) =
            NativeToolRequest::cancellable(Uuid::new_v4(), NativeToolInvocation::SystemInfo);
        assert_eq!(request.invocation_id(), cancellation.invocation_id());
        assert_eq!(cancellation.cancel(), ToolCancellationStatus::Requested);
        assert_eq!(
            cancellation.cancel(),
            ToolCancellationStatus::AlreadyRequested
        );
        let result = service
            .execute(&ServerSessionManager::default(), request)
            .await
            .expect("cancelled result");
        assert!(result.cancelled);
        assert_eq!(result.error_code, Some("TOOL_CANCELLED"));
        let audit = service.audit().await.expect("audit");
        assert_eq!(audit[0].status, ToolAuditStatus::Cancelled);
        assert!(audit[0].cancellation_requested_at_epoch_ms.is_some());
        assert_eq!(audit[0].succeeded, Some(false));
    }

    #[tokio::test]
    async fn write_tool_without_changeset_authority_is_blocked_before_session_access() {
        let (_directory, service) = service(ToolPolicy::approved_repair());
        let result = service
            .execute(
                &ServerSessionManager::default(),
                NativeToolRequest::new(
                    Uuid::new_v4(),
                    NativeToolInvocation::FilePatch {
                        path: "/etc/example.conf".into(),
                        expected: "secret-before".into(),
                        replacement: "secret-after".into(),
                    },
                ),
            )
            .await
            .expect("structured policy result");
        assert_eq!(result.error_code, Some("TOOL_APPROVAL_UNAVAILABLE"));
        let serialized =
            serde_json::to_string(&service.audit().await.expect("audit")).expect("serialize audit");
        assert!(!serialized.contains("secret-before"));
        assert!(!serialized.contains("secret-after"));
    }
}
