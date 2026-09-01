use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{
    AgentDoctorRequest, AgentRun, AgentRuntimeService, ChangeSetService, ModelGateway,
    ObservationCache,
};
use crate::credentials::CredentialService;
use crate::domain::{AppError, AppResult};
use crate::mcp::McpGateway;
use crate::policy::{AgentPolicyService, PolicyTarget};
use crate::profiles::ProfileService;
use crate::skills::SkillRegistry;
use crate::ssh::ServerSessionManager;
use crate::tools::NativeToolExecutionService;

const MAX_TARGETS: usize = 10;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MultiTarget {
    pub run_id: Uuid,
    pub session_id: Uuid,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MultiServerDoctorRequest {
    pub id: Uuid,
    pub user_request: String,
    pub targets: Vec<MultiTarget>,
    pub service: Option<String>,
    pub skill_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DriftValue {
    pub session_id: Uuid,
    pub value: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DriftFinding {
    pub field: String,
    pub values: Vec<DriftValue>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MultiServerRun {
    pub id: Uuid,
    pub targets: Vec<AgentRun>,
    pub drift: Vec<DriftFinding>,
}

impl AgentRuntimeService {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_multi_doctor(
        &self,
        sessions: &ServerSessionManager,
        tools: &NativeToolExecutionService,
        cache: &ObservationCache,
        skills: &SkillRegistry,
        mcp: &McpGateway,
        credentials: &CredentialService,
        models: &ModelGateway,
        changes: &ChangeSetService,
        policies: &AgentPolicyService,
        profiles: &ProfileService,
        request: MultiServerDoctorRequest,
    ) -> AppResult<MultiServerRun> {
        if request.targets.is_empty()
            || request.targets.len() > MAX_TARGETS
            || request.user_request.trim().is_empty()
        {
            return Err(AppError::InvalidOperation);
        }
        let mut unique = BTreeSet::new();
        if request
            .targets
            .iter()
            .any(|item| !unique.insert(item.session_id))
        {
            return Err(AppError::InvalidOperation);
        }
        let mut targets = Vec::with_capacity(request.targets.len());
        for target in request.targets {
            let server_id = sessions.profile_id(target.session_id).await?;
            let profile = profiles.get(server_id).await?;
            targets.push(
                self.run_doctor(
                    sessions,
                    tools,
                    cache,
                    skills,
                    mcp,
                    credentials,
                    models,
                    changes,
                    policies,
                    PolicyTarget {
                        server_id,
                        group_id: profile.group_id,
                        environment: None,
                    },
                    false,
                    None,
                    AgentDoctorRequest {
                        run_id: target.run_id,
                        session_id: target.session_id,
                        user_request: request.user_request.clone(),
                        service: request.service.clone(),
                        http_url: None,
                        port_host: None,
                        port: None,
                        include_nginx_test: false,
                        skill_id: request.skill_id.clone(),
                        mcp_context: None,
                        incident_id: None,
                        budget: None,
                    },
                )
                .await?,
            );
        }
        let drift = detect_drift(&targets);
        Ok(MultiServerRun {
            id: request.id,
            targets,
            drift,
        })
    }
}

fn detect_drift(runs: &[AgentRun]) -> Vec<DriftFinding> {
    let mut fields: BTreeMap<String, Vec<DriftValue>> = BTreeMap::new();
    for run in runs {
        for evidence in &run.evidence {
            if let Some(data) = &evidence.result.data {
                if matches!(
                    evidence.result.tool_name.as_str(),
                    "system.info" | "service.status"
                ) {
                    if let Ok(value) = serde_json::to_value(data) {
                        fields
                            .entry(evidence.result.tool_name.as_str().into())
                            .or_default()
                            .push(DriftValue {
                                session_id: run.session_id,
                                value,
                            });
                    }
                }
            }
        }
    }
    fields
        .into_iter()
        .filter_map(|(field, values)| {
            let distinct = values
                .iter()
                .filter_map(|item| serde_json::to_string(&item.value).ok())
                .collect::<BTreeSet<_>>();
            (distinct.len() > 1).then_some(DriftFinding { field, values })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_runs_have_no_drift() {
        assert!(detect_drift(&[]).is_empty());
    }
}
