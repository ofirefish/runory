//! Session-scoped native read dispatcher for production V2 runs.

use async_trait::async_trait;
use tauri::{AppHandle, Manager};
use tokio::sync::watch;
use uuid::Uuid;

use super::decision::{CommandRisk, PreparedCommandProposal, PreparedToolCall};
use super::dispatch::{NativeReadDispatcher, ToolDispatcher, ToolOutcome};
use super::{AgentCommandExecutionService, CommandOutcome};
use crate::agentic::ObservationCache;
use crate::domain::SessionId;
use crate::ssh::ServerSessionManager;
use crate::tools::NativeToolExecutionService;

/// Resolves Tauri-managed services per dispatch so the controller can own
/// this type across async tasks.
pub struct SessionToolDispatcher {
    app: AppHandle,
    session_id: SessionId,
    target_id: Uuid,
    cancellation: watch::Receiver<bool>,
}

impl SessionToolDispatcher {
    pub fn new(
        app: AppHandle,
        session_id: SessionId,
        target_id: Uuid,
        cancellation: watch::Receiver<bool>,
    ) -> Self {
        Self {
            app,
            session_id,
            target_id,
            cancellation,
        }
    }
}

#[async_trait]
impl ToolDispatcher for SessionToolDispatcher {
    async fn execute_reads(&self, run_id: Uuid, calls: &[PreparedToolCall]) -> Vec<ToolOutcome> {
        let sessions = self.app.state::<ServerSessionManager>();
        let tools = self.app.state::<NativeToolExecutionService>();
        let cache = self.app.state::<ObservationCache>();
        let dispatcher = NativeReadDispatcher::new(
            &sessions,
            &tools,
            &cache,
            self.session_id,
            self.target_id,
            self.cancellation.clone(),
        );
        dispatcher.execute_reads(run_id, calls).await
    }

    async fn execute_command(
        &self,
        _run_id: Uuid,
        command: &PreparedCommandProposal,
    ) -> CommandOutcome {
        let sessions = self.app.state::<ServerSessionManager>();
        AgentCommandExecutionService::new(&sessions)
            .execute(
                self.session_id,
                self.target_id,
                command,
                self.cancellation.clone(),
            )
            .await
    }
}

/// Policy gate backed by the live Tauri-managed policy service.
pub struct SessionPolicyGate {
    app: AppHandle,
    server_id: Uuid,
}

impl SessionPolicyGate {
    pub fn new(app: AppHandle, server_id: Uuid) -> Self {
        Self { app, server_id }
    }
}

#[async_trait]
impl super::gate::AuthorizationGate for SessionPolicyGate {
    async fn authorize(&self, call: &PreparedToolCall) -> super::gate::Authorization {
        let policy = self.app.state::<crate::policy::AgentPolicyService>();
        super::gate::PolicyAuthorizationGate::new(&policy, self.server_id)
            .authorize(call)
            .await
    }

    async fn authorize_command(
        &self,
        command: &super::decision::PreparedCommandProposal,
    ) -> super::gate::CommandAuthorization {
        if command.risk == CommandRisk::Critical {
            return super::gate::CommandAuthorization::Blocked {
                reason_code: "COMMAND_CRITICAL_BLOCKED".into(),
            };
        }
        let policy = self.app.state::<crate::policy::AgentPolicyService>();
        let (policy_version, policy_hash) = policy.command_approval_identity().await;
        super::gate::CommandAuthorization::RequireApproval {
            risk: command.risk.as_str().into(),
            policy_version,
            policy_hash,
        }
    }
}

/// Resume-time policy snapshot matcher over the live policy engine.
pub struct SessionPolicyMatcher {
    app: AppHandle,
}

impl SessionPolicyMatcher {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl super::gate::PolicySnapshotMatcher for SessionPolicyMatcher {
    async fn matches(&self, policy_version: u64, policy_hash: &str) -> bool {
        let policy = self.app.state::<crate::policy::AgentPolicyService>();
        super::gate::LivePolicyMatcher::new(&policy)
            .matches(policy_version, policy_hash)
            .await
    }
}

/// Alias for the production controller wiring used by the V2 service.
pub type V2Controller = super::controller::AgentController<
    super::reasoner_planning::PlanningReasoner,
    SessionToolDispatcher,
>;
