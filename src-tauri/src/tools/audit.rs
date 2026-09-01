use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::domain::{AppError, AppResult, SessionId};
use crate::storage::JsonRepository;

use super::{Mutability, NativeToolName, RiskLevel, ToolPolicyDecision, ToolResult, ToolScope};

const MAX_TOOL_AUDIT_RECORDS: usize = 2_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub(crate) enum SanitizedToolInput {
    None,
    Service {
        name: Option<String>,
        lines: Option<u32>,
    },
    NetworkEndpoint {
        host: Option<String>,
        port: u16,
    },
    HttpOrigin {
        origin: Option<String>,
    },
    RemotePath {
        path: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ToolAuditStatus {
    PolicyBlocked,
    InProgress,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ToolApprovalAudit {
    NotRequired,
    Approved,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ToolVerificationAudit {
    NotApplicable,
    Pending,
    Succeeded,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ToolRollbackAudit {
    NotSupported,
    Available,
    Executed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolAuditRecord {
    pub id: Uuid,
    pub request_id: Uuid,
    pub invocation_id: Uuid,
    pub agent_run_id: Option<Uuid>,
    #[serde(default)]
    pub model: Option<String>,
    pub change_set_id: Option<Uuid>,
    pub change_set_version: Option<u64>,
    pub profile_id: Option<Uuid>,
    pub session_id: SessionId,
    pub tool_name: NativeToolName,
    pub sanitized_input: SanitizedToolInput,
    pub risk_level: RiskLevel,
    pub mutability: Mutability,
    pub scope: ToolScope,
    pub policy_decision: ToolPolicyDecision,
    pub approval: ToolApprovalAudit,
    pub status: ToolAuditStatus,
    pub verification: ToolVerificationAudit,
    pub rollback: ToolRollbackAudit,
    pub started_at_epoch_ms: u64,
    pub completed_at_epoch_ms: Option<u64>,
    pub cancellation_requested_at_epoch_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub succeeded: Option<bool>,
    pub error_code: Option<String>,
    pub truncated: bool,
}

#[derive(Clone)]
pub(crate) struct ToolAuditRepository {
    repository: JsonRepository<Vec<ToolAuditRecord>>,
}

impl ToolAuditRepository {
    pub(crate) fn new(repository: JsonRepository<Vec<ToolAuditRecord>>) -> Self {
        Self { repository }
    }

    async fn list(&self) -> AppResult<Vec<ToolAuditRecord>> {
        self.repository.load_or_default().await
    }

    async fn save(&self, records: &[ToolAuditRecord]) -> AppResult<()> {
        self.repository.save_atomic(&records.to_vec()).await
    }
}

pub(crate) struct ToolAuditRecorder {
    repository: ToolAuditRepository,
    lock: Arc<Mutex<()>>,
}

impl ToolAuditRecorder {
    pub(crate) fn new(repository: ToolAuditRepository) -> Self {
        Self {
            repository,
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) async fn append(&self, record: ToolAuditRecord) -> AppResult<()> {
        let _guard = self.lock.lock().await;
        let mut records = self.repository.list().await?;
        records.push(record);
        if records.len() > MAX_TOOL_AUDIT_RECORDS {
            records.drain(..records.len() - MAX_TOOL_AUDIT_RECORDS);
        }
        self.repository.save(&records).await
    }

    pub(crate) async fn complete(
        &self,
        invocation_id: Uuid,
        profile_id: Option<Uuid>,
        result: &ToolResult,
        completed_at_epoch_ms: u64,
    ) -> AppResult<()> {
        let _guard = self.lock.lock().await;
        let mut records = self.repository.list().await?;
        let record = records
            .iter_mut()
            .find(|record| record.invocation_id == invocation_id)
            .ok_or(AppError::Storage)?;
        record.profile_id = profile_id;
        record.status = if result.cancelled {
            ToolAuditStatus::Cancelled
        } else if result.success {
            ToolAuditStatus::Succeeded
        } else {
            ToolAuditStatus::Failed
        };
        record.completed_at_epoch_ms = Some(completed_at_epoch_ms);
        record.duration_ms = Some(result.duration_ms);
        record.succeeded = Some(result.success);
        record.error_code = result.error_code.map(str::to_owned);
        record.truncated = result.truncated;
        if record.mutability == Mutability::Write {
            record.verification = if result.success {
                ToolVerificationAudit::Succeeded
            } else {
                ToolVerificationAudit::Failed
            };
            if record.rollback == ToolRollbackAudit::Executed && !result.success {
                record.rollback = ToolRollbackAudit::Failed;
            }
        }
        self.repository.save(&records).await
    }

    pub(crate) async fn mark_cancellation_requested(
        &self,
        invocation_id: Uuid,
        requested_at_epoch_ms: u64,
    ) -> AppResult<()> {
        let _guard = self.lock.lock().await;
        let mut records = self.repository.list().await?;
        let record = records
            .iter_mut()
            .find(|record| record.invocation_id == invocation_id)
            .ok_or(AppError::Storage)?;
        record.cancellation_requested_at_epoch_ms = Some(requested_at_epoch_ms);
        self.repository.save(&records).await
    }

    pub(crate) async fn list(&self) -> AppResult<Vec<ToolAuditRecord>> {
        let _guard = self.lock.lock().await;
        self.repository.list().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolPolicyDecision;

    #[tokio::test]
    async fn audit_persists_only_sanitized_metadata() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("tool-audit.json");
        let recorder =
            ToolAuditRecorder::new(ToolAuditRepository::new(JsonRepository::new(path.clone())));
        recorder
            .append(ToolAuditRecord {
                id: Uuid::new_v4(),
                request_id: Uuid::new_v4(),
                invocation_id: Uuid::new_v4(),
                agent_run_id: None,
                model: None,
                change_set_id: None,
                change_set_version: None,
                profile_id: None,
                session_id: Uuid::new_v4(),
                tool_name: NativeToolName::HttpRequest,
                sanitized_input: SanitizedToolInput::HttpOrigin {
                    origin: Some("https://example.com".into()),
                },
                risk_level: RiskLevel::R1,
                mutability: Mutability::Read,
                scope: ToolScope::Session,
                policy_decision: ToolPolicyDecision::Allowed,
                approval: ToolApprovalAudit::NotRequired,
                status: ToolAuditStatus::InProgress,
                verification: ToolVerificationAudit::NotApplicable,
                rollback: ToolRollbackAudit::NotSupported,
                started_at_epoch_ms: 1,
                completed_at_epoch_ms: None,
                cancellation_requested_at_epoch_ms: None,
                duration_ms: None,
                succeeded: None,
                error_code: None,
                truncated: false,
            })
            .await
            .expect("append audit");
        let serialized = String::from_utf8(tokio::fs::read(path).await.expect("read audit"))
            .expect("utf8 audit");
        assert!(serialized.contains("https://example.com"));
        assert!(!serialized.contains("token=secret"));
        assert!(!serialized.contains("bodyPreview"));
        assert!(!serialized.contains("password"));
    }
}
