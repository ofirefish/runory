use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::domain::{
    AiAgentPlan, AiAuditRecord, AiPlanRequest, AiPlanStep, AiStepStatus, AiToolExecution, AppError,
    AppResult,
};
use crate::ssh::ServerSessionManager;

use super::agent_repository::AiAuditRepository;
use super::policy::{audit_target, risk, summary, validate_plan};
use super::tools::AiToolExecutor;

const MAX_AUDIT_RECORDS: usize = 2_000;

pub struct AiAgentService {
    plans: Arc<RwLock<HashMap<Uuid, StoredPlan>>>,
    audit: AiAuditRepository,
    audit_lock: Arc<Mutex<()>>,
}

struct StoredPlan {
    plan: AiAgentPlan,
    tools: HashMap<Uuid, crate::domain::AiToolInput>,
}

impl AiAgentService {
    pub fn new(audit: AiAuditRepository) -> Self {
        Self {
            plans: Arc::new(RwLock::new(HashMap::new())),
            audit,
            audit_lock: Arc::new(Mutex::new(())),
        }
    }

    pub async fn create_plan(
        &self,
        sessions: &ServerSessionManager,
        request: AiPlanRequest,
    ) -> AppResult<AiAgentPlan> {
        validate_plan(&request)?;
        for session_id in &request.session_ids {
            sessions.profile_id(*session_id).await?;
        }
        let mut steps = Vec::with_capacity(request.session_ids.len() * request.tools.len());
        let mut tools = HashMap::new();
        for session_id in request.session_ids {
            for tool in &request.tools {
                let id = Uuid::new_v4();
                steps.push(AiPlanStep {
                    id,
                    session_id,
                    tool: summary(tool),
                    risk: risk(tool),
                    status: AiStepStatus::PendingApproval,
                });
                tools.insert(id, tool.clone());
            }
        }
        let plan = AiAgentPlan {
            id: Uuid::new_v4(),
            goal: request.goal.trim().to_owned(),
            steps,
        };
        self.plans.write().await.insert(
            plan.id,
            StoredPlan {
                plan: plan.clone(),
                tools,
            },
        );
        Ok(plan)
    }

    pub async fn get_plan(&self, plan_id: Uuid) -> AppResult<AiAgentPlan> {
        self.plans
            .read()
            .await
            .get(&plan_id)
            .map(|stored| stored.plan.clone())
            .ok_or(AppError::InvalidOperation)
    }

    pub async fn discard_plan(&self, plan_id: Uuid) -> AppResult<()> {
        let mut plans = self.plans.write().await;
        let plan = plans.get(&plan_id).ok_or(AppError::InvalidOperation)?;
        if plan
            .plan
            .steps
            .iter()
            .any(|step| step.status == AiStepStatus::Running)
        {
            return Err(AppError::InvalidOperation);
        }
        plans.remove(&plan_id);
        Ok(())
    }

    pub async fn approve_step(&self, plan_id: Uuid, step_id: Uuid) -> AppResult<AiAgentPlan> {
        let mut plans = self.plans.write().await;
        let stored = plans.get_mut(&plan_id).ok_or(AppError::InvalidOperation)?;
        let step = stored
            .plan
            .steps
            .iter_mut()
            .find(|step| step.id == step_id)
            .ok_or(AppError::InvalidOperation)?;
        if step.status != AiStepStatus::PendingApproval {
            return Err(AppError::InvalidOperation);
        }
        step.status = AiStepStatus::Approved;
        Ok(stored.plan.clone())
    }

    pub async fn execute_step(
        &self,
        sessions: &ServerSessionManager,
        plan_id: Uuid,
        step_id: Uuid,
    ) -> AppResult<AiToolExecution> {
        let (session_id, tool, step_risk) = {
            let mut plans = self.plans.write().await;
            let stored = plans.get_mut(&plan_id).ok_or(AppError::InvalidOperation)?;
            let step = stored
                .plan
                .steps
                .iter_mut()
                .find(|step| step.id == step_id)
                .ok_or(AppError::InvalidOperation)?;
            if step.status != AiStepStatus::Approved {
                return Err(AppError::InvalidOperation);
            }
            step.status = AiStepStatus::Running;
            let tool = stored
                .tools
                .get(&step_id)
                .cloned()
                .ok_or(AppError::InvalidOperation)?;
            (step.session_id, tool, step.risk)
        };
        let profile_id = match sessions.profile_id(session_id).await {
            Ok(profile_id) => profile_id,
            Err(error) => {
                if let Some(stored) = self.plans.write().await.get_mut(&plan_id) {
                    stored.tools.remove(&step_id);
                    if let Some(step) = stored.plan.steps.iter_mut().find(|step| step.id == step_id)
                    {
                        step.status = AiStepStatus::Failed;
                    }
                }
                return Err(error);
            }
        };
        let mut record = AiAuditRecord {
            id: Uuid::new_v4(),
            plan_id,
            step_id,
            profile_id,
            tool: tool.name(),
            risk: step_risk,
            target: audit_target(&tool),
            started_at_epoch_seconds: now_epoch_seconds(),
            succeeded: false,
            error_code: Some("IN_PROGRESS".into()),
        };
        if let Err(error) = self.append_audit(record.clone()).await {
            if let Some(step) = self
                .plans
                .write()
                .await
                .get_mut(&plan_id)
                .and_then(|stored| stored.plan.steps.iter_mut().find(|step| step.id == step_id))
            {
                step.status = AiStepStatus::Approved;
            }
            return Err(error);
        }

        let result = AiToolExecutor::execute(sessions, session_id, tool).await;
        let status = if result.is_ok() {
            AiStepStatus::Succeeded
        } else {
            AiStepStatus::Failed
        };
        {
            let mut plans = self.plans.write().await;
            if let Some(stored) = plans.get_mut(&plan_id) {
                stored.tools.remove(&step_id);
                if let Some(step) = stored.plan.steps.iter_mut().find(|step| step.id == step_id) {
                    step.status = status;
                }
            }
        }
        record.succeeded = result.is_ok();
        record.error_code = result.as_ref().err().map(|error| error.code().to_owned());
        self.replace_audit(record).await?;
        let output = result?;
        Ok(AiToolExecution {
            plan: self.get_plan(plan_id).await?,
            step_id,
            output,
        })
    }

    pub async fn audit(&self, profile_id: Option<Uuid>) -> AppResult<Vec<AiAuditRecord>> {
        let mut records = self.audit.list().await?;
        if let Some(profile_id) = profile_id {
            records.retain(|record| record.profile_id == profile_id);
        }
        records.sort_by_key(|record| std::cmp::Reverse(record.started_at_epoch_seconds));
        Ok(records)
    }

    async fn append_audit(&self, record: AiAuditRecord) -> AppResult<()> {
        let _guard = self.audit_lock.lock().await;
        let mut records = self.audit.list().await?;
        records.push(record);
        if records.len() > MAX_AUDIT_RECORDS {
            records.drain(..records.len() - MAX_AUDIT_RECORDS);
        }
        self.audit.save(&records).await
    }

    async fn replace_audit(&self, record: AiAuditRecord) -> AppResult<()> {
        let _guard = self.audit_lock.lock().await;
        let mut records = self.audit.list().await?;
        let stored = records
            .iter_mut()
            .find(|stored| stored.id == record.id)
            .ok_or(AppError::Storage)?;
        *stored = record;
        self.audit.save(&records).await
    }
}

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AiRisk, AiTerminalPreset, AiToolInput, AiToolSummary};
    use crate::storage::JsonRepository;

    #[tokio::test]
    async fn audit_never_persists_file_content() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let service = AiAgentService::new(AiAuditRepository::new(JsonRepository::new(
            directory.path().join("audit.json"),
        )));
        service
            .append_audit(AiAuditRecord {
                id: Uuid::new_v4(),
                plan_id: Uuid::new_v4(),
                step_id: Uuid::new_v4(),
                profile_id: Uuid::new_v4(),
                tool: AiToolInput::FileWrite {
                    path: "/tmp/config".into(),
                    content: "secret-value".into(),
                }
                .name(),
                risk: AiRisk::Critical,
                target: Some("/tmp/config".into()),
                started_at_epoch_seconds: 1,
                succeeded: true,
                error_code: None,
            })
            .await
            .expect("audit");
        let bytes = tokio::fs::read(directory.path().join("audit.json"))
            .await
            .expect("read");
        let text = String::from_utf8(bytes).expect("utf8");
        assert!(!text.contains("secret-value"));
    }

    #[tokio::test]
    async fn discarding_plan_removes_pending_tool_payloads() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let service = AiAgentService::new(AiAuditRepository::new(JsonRepository::new(
            directory.path().join("audit.json"),
        )));
        let plan_id = Uuid::new_v4();
        let step_id = Uuid::new_v4();
        service.plans.write().await.insert(
            plan_id,
            StoredPlan {
                plan: AiAgentPlan {
                    id: plan_id,
                    goal: "inspect".into(),
                    steps: vec![AiPlanStep {
                        id: step_id,
                        session_id: Uuid::new_v4(),
                        tool: AiToolSummary::TerminalExec {
                            preset: AiTerminalPreset::DiskUsage,
                        },
                        risk: AiRisk::Low,
                        status: AiStepStatus::PendingApproval,
                    }],
                },
                tools: HashMap::from([(
                    step_id,
                    AiToolInput::TerminalExec {
                        preset: AiTerminalPreset::DiskUsage,
                    },
                )]),
            },
        );
        service.discard_plan(plan_id).await.expect("discard");
        assert!(matches!(
            service.get_plan(plan_id).await,
            Err(AppError::InvalidOperation)
        ));
    }
}
