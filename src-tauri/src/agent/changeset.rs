//! ChangeSet execution boundary for Runtime V2 (AR2-G).
//!
//! Write remediation always flows through the existing `ChangeSetService`;
//! the controller never calls write tools directly.

use async_trait::async_trait;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::agentic::{
    ChangeSet, ChangeSetDraftRequest, ChangeSetService, ChangeStepDraft, PolicyCheckContext,
};
use crate::domain::{AppError, SessionId};
use crate::policy::{
    AgentPolicyService, PolicyEvaluation, PolicyExecutionStrategy, PolicyInvocationSource,
    PolicyTarget,
};
use crate::ssh::ServerSessionManager;
use crate::tools::NativeToolExecutionService;

pub const CHANGE_PROPOSAL_NOT_EVIDENCE_BOUND: &str = "CHANGE_PROPOSAL_NOT_EVIDENCE_BOUND";
pub const CHANGESET_POLICY_BLOCKED: &str = "CHANGESET_POLICY_BLOCKED";
pub const CHANGESET_EXECUTION_FAILED: &str = "CHANGESET_EXECUTION_FAILED";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChangeSetExecutorError {
    Store(String),
    PolicyBlocked,
    ExecutionFailed,
    InvalidOperation,
}

impl ChangeSetExecutorError {
    pub fn code(&self) -> &str {
        match self {
            Self::Store(code) => code,
            Self::PolicyBlocked => CHANGESET_POLICY_BLOCKED,
            Self::ExecutionFailed => CHANGESET_EXECUTION_FAILED,
            Self::InvalidOperation => "INVALID_OPERATION",
        }
    }
}

impl From<AppError> for ChangeSetExecutorError {
    fn from(error: AppError) -> Self {
        Self::Store(error.code().to_owned())
    }
}

#[async_trait]
pub(crate) trait ChangeSetExecutor: Send + Sync {
    async fn draft_and_check_policy(
        &self,
        run_id: Uuid,
        title: String,
        steps: Vec<ChangeStepDraft>,
    ) -> Result<ChangeSet, ChangeSetExecutorError>;

    async fn approve(
        &self,
        change_set_id: Uuid,
        version: u64,
    ) -> Result<ChangeSet, ChangeSetExecutorError>;

    async fn execute(
        &self,
        change_set_id: Uuid,
        version: u64,
    ) -> Result<ChangeSet, ChangeSetExecutorError>;

    async fn verify(
        &self,
        change_set_id: Uuid,
        version: u64,
    ) -> Result<bool, ChangeSetExecutorError>;

    async fn rollback(
        &self,
        change_set_id: Uuid,
        version: u64,
    ) -> Result<ChangeSet, ChangeSetExecutorError>;
}

/// Production executor over Tauri-managed ChangeSet / Policy / Tool services.
pub struct SessionChangeSetExecutor {
    app: AppHandle,
    session_id: SessionId,
    server_id: Uuid,
}

impl SessionChangeSetExecutor {
    pub fn new(app: AppHandle, session_id: SessionId, server_id: Uuid) -> Self {
        Self {
            app,
            session_id,
            server_id,
        }
    }

    fn policy_context(&self) -> PolicyCheckContext {
        PolicyCheckContext {
            target: PolicyTarget {
                server_id: self.server_id,
                group_id: None,
                environment: None,
            },
            target_count: 1,
            execution_strategy: PolicyExecutionStrategy::Sequential,
            source: PolicyInvocationSource::AgentRuntime,
        }
    }
}

#[async_trait]
impl ChangeSetExecutor for SessionChangeSetExecutor {
    async fn draft_and_check_policy(
        &self,
        run_id: Uuid,
        title: String,
        steps: Vec<ChangeStepDraft>,
    ) -> Result<ChangeSet, ChangeSetExecutorError> {
        let changes = self.app.state::<ChangeSetService>();
        let policies = self.app.state::<AgentPolicyService>();
        let draft = changes
            .draft(ChangeSetDraftRequest {
                agent_run_id: run_id,
                session_id: self.session_id,
                title,
                steps,
            })
            .await?;
        let checked = changes
            .check_policy(draft.id, draft.version, &policies, self.policy_context())
            .await?;
        if checked
            .policy_evaluation
            .as_ref()
            .is_some_and(|evaluation: &PolicyEvaluation| !evaluation.permits_execution())
        {
            return Err(ChangeSetExecutorError::PolicyBlocked);
        }
        Ok(checked)
    }

    async fn approve(
        &self,
        change_set_id: Uuid,
        version: u64,
    ) -> Result<ChangeSet, ChangeSetExecutorError> {
        self.app
            .state::<ChangeSetService>()
            .approve(change_set_id, version)
            .await
            .map_err(Into::into)
    }

    async fn execute(
        &self,
        change_set_id: Uuid,
        version: u64,
    ) -> Result<ChangeSet, ChangeSetExecutorError> {
        let sessions = self.app.state::<ServerSessionManager>();
        let tools = self.app.state::<NativeToolExecutionService>();
        let policies = self.app.state::<AgentPolicyService>();
        self.app
            .state::<ChangeSetService>()
            .execute_with_policy(change_set_id, version, &sessions, &tools, &policies)
            .await
            .map_err(|error| {
                if matches!(error, AppError::InvalidOperation) {
                    ChangeSetExecutorError::InvalidOperation
                } else {
                    ChangeSetExecutorError::ExecutionFailed
                }
            })
    }

    async fn verify(
        &self,
        change_set_id: Uuid,
        version: u64,
    ) -> Result<bool, ChangeSetExecutorError> {
        let sessions = self.app.state::<ServerSessionManager>();
        let tools = self.app.state::<NativeToolExecutionService>();
        self.app
            .state::<ChangeSetService>()
            .verify_execution(change_set_id, version, &sessions, &tools)
            .await
            .map_err(Into::into)
    }

    async fn rollback(
        &self,
        change_set_id: Uuid,
        version: u64,
    ) -> Result<ChangeSet, ChangeSetExecutorError> {
        let sessions = self.app.state::<ServerSessionManager>();
        let tools = self.app.state::<NativeToolExecutionService>();
        self.app
            .state::<ChangeSetService>()
            .rollback(change_set_id, version, &sessions, &tools)
            .await
            .map_err(Into::into)
    }
}
