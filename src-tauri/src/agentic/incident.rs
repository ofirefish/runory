use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Mutex};
use uuid::Uuid;

use super::fleet::FleetExecutionState;
use super::optimization::ObservationCache;
use super::runtime::execute_reads;
use super::state::AgentRunMetrics;
use super::{ChangeSetService, ExecutionState, FleetExecutionService};
use crate::domain::{AppError, AppResult, ServiceStatus};
use crate::ssh::ServerSessionManager;
use crate::storage::JsonRepository;
use crate::tools::{
    NativeToolExecutionService, NativeToolInvocation, RiskLevel, ToolData, ToolResult,
};

const MAX_TARGETS: usize = 10;
const MAX_SYMPTOMS: usize = 20;
const MAX_TOOL_CALLS_PER_TARGET: usize = 20;
const MAX_INCIDENTS: usize = 2_000;
const INCIDENT_SCHEMA_VERSION: u8 = 2;
const LEGACY_INCIDENT_SCHEMA_VERSION: u8 = 1;
const MAX_INCIDENT_TIMELINE_EVENTS: usize = 512;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum OperationsPack {
    Website,
    Nginx,
    Docker,
    Disk,
    Service,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum IncidentStatus {
    Reported,
    Investigating,
    Correlating,
    AwaitingApproval,
    Executing,
    Verifying,
    Resolved,
    Failed,
    RolledBack,
    Inconclusive,
    Interrupted,
    Closed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum IncidentRecoveryState {
    Live,
    MetadataOnly,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum IncidentSeverity {
    Sev1,
    Sev2,
    Sev3,
    Sev4,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncidentRequest {
    pub id: Uuid,
    pub pack: OperationsPack,
    pub severity: IncidentSeverity,
    pub targets: Vec<Uuid>,
    pub symptoms: Vec<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub service: Option<String>,
    pub container: Option<String>,
    pub config_path: Option<String>,
    pub upstream_host: Option<String>,
    pub upstream_port: Option<u16>,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncidentEvidence {
    pub id: Uuid,
    pub target_id: Uuid,
    pub source: String,
    pub summary: &'static str,
    pub result: ToolResult,
}

fn evidence_reference(item: &IncidentEvidence) -> IncidentEvidenceReference {
    IncidentEvidenceReference {
        id: item.id,
        target_id: item.target_id,
        source: item.source.clone(),
        success: item.result.success,
        error_code: item.result.error_code.map(str::to_string),
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncidentEvidenceReference {
    pub id: Uuid,
    pub target_id: Uuid,
    pub source: String,
    pub success: bool,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncidentComparisonValue {
    pub target_id: Uuid,
    pub signal: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncidentComparison {
    pub dimension: String,
    pub drift: bool,
    pub values: Vec<IncidentComparisonValue>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncidentHypothesis {
    pub code: String,
    pub evidence_ids: Vec<Uuid>,
    pub supported: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncidentRootCause {
    pub code: String,
    pub confidence: f32,
    pub evidence_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProposedFix {
    pub code: String,
    pub risk: RiskLevel,
    pub action_codes: Vec<String>,
    pub change_set_required: bool,
    pub execution_pipeline: [String; 5],
    pub dangerous_deletion_blocked: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncidentChangeSet {
    pub(crate) kind: String,
    pub(crate) id: Uuid,
    pub(crate) version: u64,
    pub(crate) exact_target_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncidentVerification {
    pub status_code: String,
    pub evidence_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncidentResolution {
    pub code: String,
    pub resolved_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncidentHandoff {
    pub owner: String,
    pub summary: String,
    pub handed_off_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncidentHandoffRequest {
    pub owner: String,
    pub summary: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum IncidentClosureCode {
    Resolved,
    AcceptedRisk,
    FalsePositive,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncidentClosureRequest {
    pub code: IncidentClosureCode,
    pub summary: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncidentTimelineEvent {
    pub status: IncidentStatus,
    pub code: String,
    pub occurred_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Incident {
    pub id: Uuid,
    pub agent_run_id: Uuid,
    pub model: String,
    pub pack: OperationsPack,
    pub status: IncidentStatus,
    pub severity: IncidentSeverity,
    pub targets: Vec<Uuid>,
    pub symptoms: Vec<String>,
    pub evidence: Vec<IncidentEvidence>,
    pub evidence_references: Vec<IncidentEvidenceReference>,
    pub comparisons: Vec<IncidentComparison>,
    pub hypotheses: Vec<IncidentHypothesis>,
    pub root_cause: IncidentRootCause,
    pub proposed_fix: ProposedFix,
    pub change_set: Option<IncidentChangeSet>,
    pub verification: IncidentVerification,
    pub resolution: Option<IncidentResolution>,
    pub handoff: Option<IncidentHandoff>,
    pub timeline: Vec<IncidentTimelineEvent>,
    pub duration_ms: u64,
    pub recovery_state: IncidentRecoveryState,
    pub metrics: AgentRunMetrics,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedIncident {
    schema_version: u8,
    id: Uuid,
    agent_run_id: Uuid,
    model: String,
    pack: OperationsPack,
    status: IncidentStatus,
    severity: IncidentSeverity,
    targets: Vec<Uuid>,
    evidence_references: Vec<IncidentEvidenceReference>,
    comparisons: Vec<IncidentComparison>,
    root_cause: IncidentRootCause,
    proposed_fix: ProposedFix,
    change_set: Option<IncidentChangeSet>,
    verification: IncidentVerification,
    resolution: Option<IncidentResolution>,
    handoff_owner: Option<String>,
    timeline: Vec<IncidentTimelineEvent>,
    duration_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncidentAuditExport {
    pub schema_version: u8,
    pub content_omitted: bool,
    pub id: Uuid,
    pub agent_run_id: Uuid,
    pub model: String,
    pub pack: OperationsPack,
    pub status: IncidentStatus,
    pub severity: IncidentSeverity,
    pub targets: Vec<Uuid>,
    pub evidence_references: Vec<IncidentEvidenceReference>,
    pub comparisons: Vec<IncidentComparison>,
    pub root_cause: IncidentRootCause,
    pub proposed_fix: ProposedFix,
    pub change_set: Option<IncidentChangeSet>,
    pub verification: IncidentVerification,
    pub resolution: Option<IncidentResolution>,
    pub timeline: Vec<IncidentTimelineEvent>,
    pub duration_ms: u64,
}

pub(crate) struct IncidentService {
    items: Arc<Mutex<HashMap<Uuid, Incident>>>,
    repository: Option<JsonRepository<Vec<PersistedIncident>>>,
}

impl Default for IncidentService {
    fn default() -> Self {
        Self {
            items: Arc::new(Mutex::new(HashMap::new())),
            repository: None,
        }
    }
}

impl IncidentService {
    pub(crate) fn at_path(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            items: Arc::new(Mutex::new(HashMap::new())),
            repository: Some(JsonRepository::new(path)),
        }
    }

    pub(crate) async fn load(&self) -> AppResult<()> {
        let Some(repository) = &self.repository else {
            return Ok(());
        };
        let records = repository.load_or_default().await?;
        if records.len() > MAX_INCIDENTS
            || records
                .iter()
                .any(|record| !valid_persisted_incident(record))
        {
            // Invalid audit metadata is preserved on disk and fails closed. Silently dropping or
            // rewriting a tampered record would destroy production incident traceability.
            return Err(AppError::Storage);
        }
        let mut items = self.items.lock().await;
        items.clear();
        for record in records {
            let incident = recovered_incident(record);
            items.insert(incident.id, incident);
        }
        self.persist_locked(&items).await
    }

    pub(crate) async fn run(
        &self,
        sessions: &ServerSessionManager,
        tools: &NativeToolExecutionService,
        cache: &ObservationCache,
        request: IncidentRequest,
    ) -> AppResult<Incident> {
        validate_request(&request)?;
        if self.items.lock().await.contains_key(&request.id) {
            return Err(AppError::InvalidOperation);
        }
        let started = now_ms();
        let agent_run_id = Uuid::new_v4();
        let mut metrics = AgentRunMetrics::new(agent_run_id, Some(request.id));
        let mut timeline = vec![
            event(IncidentStatus::Reported, "incident-reported"),
            event(IncidentStatus::Investigating, "typed-investigation-started"),
        ];
        let mut evidence = Vec::new();
        let (_sender, receiver) = watch::channel(false);
        for target in &request.targets {
            let plan = plan_for(&request)?;
            if plan.len() > MAX_TOOL_CALLS_PER_TARGET {
                return Err(AppError::InvalidOperation);
            }
            let results = execute_reads(
                sessions,
                tools,
                cache,
                agent_run_id,
                *target,
                receiver.clone(),
                plan,
                &mut metrics,
            )
            .await?;
            for result in results {
                evidence.push(IncidentEvidence {
                    id: Uuid::new_v4(),
                    target_id: *target,
                    source: format!("tool.{}", result.tool_name.as_str()),
                    summary: result.summary,
                    result,
                });
            }
        }
        timeline.push(event(IncidentStatus::Correlating, "evidence-correlated"));
        let (root_cause, hypotheses, proposed_fix) = diagnose(request.pack, &evidence);
        let status = if root_cause.code == "incident-inconclusive" {
            IncidentStatus::Inconclusive
        } else {
            IncidentStatus::AwaitingApproval
        };
        timeline.push(event(
            status,
            if status == IncidentStatus::Inconclusive {
                "root-cause-inconclusive"
            } else {
                "root-cause-evidence-bound"
            },
        ));
        let evidence_references = evidence.iter().map(evidence_reference).collect();
        let comparisons = compare_targets(&request.targets, &evidence);
        let duration_ms = now_ms().saturating_sub(started);
        metrics.duration_ms = duration_ms;
        metrics.diagnosis_latency_ms = Some(duration_ms);
        metrics.context_size_bytes = request.symptoms.iter().map(String::len).sum();
        let incident = Incident {
            id: request.id,
            agent_run_id,
            model: "runory-operations-packs-v2".into(),
            pack: request.pack,
            status,
            severity: request.severity,
            targets: request.targets,
            symptoms: request.symptoms,
            evidence,
            evidence_references,
            comparisons,
            hypotheses,
            root_cause,
            proposed_fix,
            change_set: None,
            verification: IncidentVerification {
                status_code: "not-started".into(),
                evidence_ids: Vec::new(),
            },
            resolution: None,
            handoff: None,
            timeline,
            duration_ms,
            recovery_state: IncidentRecoveryState::Live,
            metrics,
        };
        let mut items = self.items.lock().await;
        if items.contains_key(&incident.id) {
            return Err(AppError::InvalidOperation);
        }
        items.insert(incident.id, incident.clone());
        trim_incidents(&mut items);
        self.persist_locked(&items).await?;
        Ok(incident)
    }

    pub(crate) async fn get(&self, id: Uuid) -> AppResult<Incident> {
        self.items
            .lock()
            .await
            .get(&id)
            .cloned()
            .ok_or(AppError::InvalidOperation)
    }

    pub(crate) async fn list(&self) -> AppResult<Vec<Incident>> {
        let mut incidents = self
            .items
            .lock()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        incidents.sort_by_key(|incident| {
            std::cmp::Reverse(
                incident
                    .timeline
                    .first()
                    .map(|event| event.occurred_at_epoch_ms)
                    .unwrap_or_default(),
            )
        });
        Ok(incidents)
    }

    pub(crate) async fn attach_change_set(
        &self,
        id: Uuid,
        link: IncidentChangeSet,
    ) -> AppResult<Incident> {
        let mut items = self.items.lock().await;
        let incident = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        if incident.recovery_state != IncidentRecoveryState::Live
            || incident.status == IncidentStatus::Closed
            || link.version == 0
            || exact_targets(&link.exact_target_ids) != exact_targets(&incident.targets)
            || !matches!(link.kind.as_str(), "single" | "fleet")
        {
            return Err(AppError::InvalidOperation);
        }
        incident.change_set = Some(link);
        incident.status = IncidentStatus::AwaitingApproval;
        incident.timeline.push(event(
            IncidentStatus::AwaitingApproval,
            "changeset-exact-targets-bound",
        ));
        let result = incident.clone();
        self.persist_locked(&items).await?;
        Ok(result)
    }

    pub(crate) async fn refresh(
        &self,
        id: Uuid,
        changes: &ChangeSetService,
        fleets: &FleetExecutionService,
    ) -> AppResult<Incident> {
        let link = self
            .get(id)
            .await?
            .change_set
            .ok_or(AppError::InvalidOperation)?;
        let next = if link.kind == "single" {
            let change = changes.get(link.id).await?;
            if change.version != link.version {
                return Err(AppError::InvalidOperation);
            }
            match change.execution_state {
                ExecutionState::NotStarted => {
                    (IncidentStatus::AwaitingApproval, "not-started", None)
                }
                ExecutionState::Executing => (IncidentStatus::Executing, "pending", None),
                ExecutionState::Committed => (
                    IncidentStatus::Resolved,
                    "succeeded",
                    Some("repair-verified"),
                ),
                ExecutionState::Failed
                | ExecutionState::Interrupted
                | ExecutionState::RollbackFailed => (IncidentStatus::Failed, "failed", None),
                ExecutionState::RolledBack => (
                    IncidentStatus::RolledBack,
                    "rollback-verified",
                    Some("repair-rolled-back"),
                ),
            }
        } else {
            let fleet = fleets.get(link.id).await?;
            if fleet.version != link.version {
                return Err(AppError::InvalidOperation);
            }
            match fleet.execution_state {
                FleetExecutionState::Draft
                | FleetExecutionState::Approved
                | FleetExecutionState::PausedForReview => {
                    (IncidentStatus::AwaitingApproval, "not-started", None)
                }
                FleetExecutionState::Executing => (IncidentStatus::Executing, "pending", None),
                FleetExecutionState::Verifying => (IncidentStatus::Verifying, "pending", None),
                FleetExecutionState::Succeeded => (
                    IncidentStatus::Resolved,
                    "succeeded",
                    Some("repair-verified"),
                ),
                FleetExecutionState::RolledBack => (
                    IncidentStatus::RolledBack,
                    "rollback-verified",
                    Some("repair-rolled-back"),
                ),
                FleetExecutionState::Failed
                | FleetExecutionState::RollbackFailed
                | FleetExecutionState::Interrupted
                | FleetExecutionState::RollingBack => (IncidentStatus::Failed, "failed", None),
            }
        };
        let mut items = self.items.lock().await;
        let incident = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        if incident.status != next.0 {
            incident
                .timeline
                .push(event(next.0, "changeset-state-synchronized"));
        }
        incident.status = next.0;
        incident.verification.status_code = next.1.into();
        incident.resolution = next.2.map(|code| IncidentResolution {
            code: code.into(),
            resolved_at_epoch_ms: now_ms(),
        });
        let result = incident.clone();
        self.persist_locked(&items).await?;
        Ok(result)
    }

    pub(crate) async fn handoff(
        &self,
        id: Uuid,
        request: IncidentHandoffRequest,
    ) -> AppResult<Incident> {
        let owner = bounded_owner(request.owner)?;
        let summary = bounded_text(request.summary, 1_000)?;
        let mut items = self.items.lock().await;
        let incident = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        if matches!(
            incident.status,
            IncidentStatus::Executing | IncidentStatus::Verifying | IncidentStatus::Closed
        ) {
            return Err(AppError::InvalidOperation);
        }
        incident.handoff = Some(IncidentHandoff {
            owner,
            summary,
            handed_off_at_epoch_ms: now_ms(),
        });
        incident
            .timeline
            .push(event(incident.status, "operator-handoff-recorded"));
        let result = incident.clone();
        self.persist_locked(&items).await?;
        Ok(result)
    }

    pub(crate) async fn close(
        &self,
        id: Uuid,
        request: IncidentClosureRequest,
    ) -> AppResult<Incident> {
        let _summary = bounded_text(request.summary, 1_000)?;
        let mut items = self.items.lock().await;
        let incident = items.get_mut(&id).ok_or(AppError::InvalidOperation)?;
        if matches!(
            incident.status,
            IncidentStatus::Executing | IncidentStatus::Verifying | IncidentStatus::Closed
        ) || (request.code == IncidentClosureCode::Resolved
            && incident.verification.status_code != "succeeded")
            || (request.code == IncidentClosureCode::FalsePositive
                && incident.root_cause.code != "incident-inconclusive")
        {
            return Err(AppError::InvalidOperation);
        }
        incident.status = IncidentStatus::Closed;
        incident.resolution = Some(IncidentResolution {
            code: match request.code {
                IncidentClosureCode::Resolved => "operator-closed-resolved",
                IncidentClosureCode::AcceptedRisk => "operator-closed-accepted-risk",
                IncidentClosureCode::FalsePositive => "operator-closed-false-positive",
            }
            .into(),
            resolved_at_epoch_ms: now_ms(),
        });
        incident
            .timeline
            .push(event(IncidentStatus::Closed, "operator-closure-recorded"));
        let result = incident.clone();
        self.persist_locked(&items).await?;
        Ok(result)
    }

    pub(crate) async fn export(&self, id: Uuid) -> AppResult<IncidentAuditExport> {
        let incident = self.get(id).await?;
        Ok(audit_export(&incident))
    }

    async fn persist_locked(&self, items: &HashMap<Uuid, Incident>) -> AppResult<()> {
        let Some(repository) = &self.repository else {
            return Ok(());
        };
        let mut records = items.values().map(persisted_incident).collect::<Vec<_>>();
        records.sort_by_key(|record| {
            std::cmp::Reverse(
                record
                    .timeline
                    .first()
                    .map(|event| event.occurred_at_epoch_ms)
                    .unwrap_or_default(),
            )
        });
        records.truncate(MAX_INCIDENTS);
        repository.save_atomic(&records).await
    }
}

fn persisted_incident(incident: &Incident) -> PersistedIncident {
    PersistedIncident {
        schema_version: INCIDENT_SCHEMA_VERSION,
        id: incident.id,
        agent_run_id: incident.agent_run_id,
        model: incident.model.clone(),
        pack: incident.pack,
        status: incident.status,
        severity: incident.severity,
        targets: incident.targets.clone(),
        evidence_references: incident.evidence_references.clone(),
        comparisons: incident.comparisons.clone(),
        root_cause: incident.root_cause.clone(),
        proposed_fix: incident.proposed_fix.clone(),
        change_set: incident.change_set.clone(),
        verification: incident.verification.clone(),
        resolution: incident.resolution.clone(),
        handoff_owner: incident
            .handoff
            .as_ref()
            .map(|handoff| handoff.owner.clone()),
        timeline: incident.timeline.clone(),
        duration_ms: incident.duration_ms,
    }
}

fn recovered_incident(record: PersistedIncident) -> Incident {
    let active = matches!(
        record.status,
        IncidentStatus::Investigating
            | IncidentStatus::Correlating
            | IncidentStatus::Executing
            | IncidentStatus::Verifying
    );
    let status = if active {
        IncidentStatus::Interrupted
    } else {
        record.status
    };
    let mut timeline = record.timeline;
    if active {
        timeline.push(event(
            IncidentStatus::Interrupted,
            "process-recovery-interrupted",
        ));
    }
    let mut metrics = AgentRunMetrics::new(record.agent_run_id, Some(record.id));
    metrics.duration_ms = record.duration_ms;
    metrics.tool_calls = record.evidence_references.len().min(u32::MAX as usize) as u32;
    Incident {
        id: record.id,
        agent_run_id: record.agent_run_id,
        model: record.model,
        pack: record.pack,
        status,
        severity: record.severity,
        targets: record.targets,
        symptoms: Vec::new(),
        evidence: Vec::new(),
        evidence_references: record.evidence_references,
        comparisons: record.comparisons,
        hypotheses: Vec::new(),
        root_cause: record.root_cause,
        proposed_fix: record.proposed_fix,
        change_set: record.change_set,
        verification: record.verification,
        resolution: record.resolution,
        handoff: record.handoff_owner.map(|owner| IncidentHandoff {
            owner,
            summary: String::new(),
            handed_off_at_epoch_ms: 0,
        }),
        timeline,
        duration_ms: record.duration_ms,
        recovery_state: IncidentRecoveryState::MetadataOnly,
        metrics,
    }
}

fn audit_export(incident: &Incident) -> IncidentAuditExport {
    IncidentAuditExport {
        schema_version: INCIDENT_SCHEMA_VERSION,
        content_omitted: true,
        id: incident.id,
        agent_run_id: incident.agent_run_id,
        model: incident.model.clone(),
        pack: incident.pack,
        status: incident.status,
        severity: incident.severity,
        targets: incident.targets.clone(),
        evidence_references: incident.evidence_references.clone(),
        comparisons: incident.comparisons.clone(),
        root_cause: incident.root_cause.clone(),
        proposed_fix: incident.proposed_fix.clone(),
        change_set: incident.change_set.clone(),
        verification: incident.verification.clone(),
        resolution: incident.resolution.clone(),
        timeline: incident.timeline.clone(),
        duration_ms: incident.duration_ms,
    }
}

fn compare_targets(targets: &[Uuid], evidence: &[IncidentEvidence]) -> Vec<IncidentComparison> {
    if targets.len() < 2 {
        return Vec::new();
    }
    let mut dimensions: HashMap<String, Vec<IncidentComparisonValue>> = HashMap::new();
    for item in evidence {
        let Some((dimension, signal)) = comparison_signal(item) else {
            continue;
        };
        dimensions
            .entry(dimension)
            .or_default()
            .push(IncidentComparisonValue {
                target_id: item.target_id,
                signal,
            });
    }
    let mut output = dimensions
        .into_iter()
        .filter_map(|(dimension, mut values)| {
            values.sort_by_key(|value| value.target_id);
            values.dedup_by_key(|value| value.target_id);
            if values.len() != targets.len() {
                return None;
            }
            let drift = values
                .iter()
                .map(|value| &value.signal)
                .collect::<HashSet<_>>()
                .len()
                > 1;
            Some(IncidentComparison {
                dimension,
                drift,
                values,
            })
        })
        .collect::<Vec<_>>();
    output.sort_by(|left, right| left.dimension.cmp(&right.dimension));
    output
}

fn comparison_signal(item: &IncidentEvidence) -> Option<(String, String)> {
    let data = item.result.data.as_ref()?;
    match data {
        ToolData::SystemInfo(value) => Some((
            "system-platform".into(),
            format!(
                "{}|{}|{}",
                value.operating_system, value.kernel_release, value.architecture
            ),
        )),
        ToolData::SystemDisk(value) => Some((
            "disk-max-percent".into(),
            format!(
                "{:.1}",
                value
                    .disks
                    .iter()
                    .map(|disk| disk.usage_percent)
                    .fold(0.0, f64::max)
            ),
        )),
        ToolData::ServiceStatus(value) => Some((
            format!("service-status:{}", value.service.name),
            format!("{:?}", value.service.status).to_ascii_lowercase(),
        )),
        ToolData::NetworkPortCheck(value) => {
            Some((format!("port:{}", value.port), value.reachable.to_string()))
        }
        ToolData::HttpResponse(value) => {
            Some(("http-status".into(), value.status_code.to_string()))
        }
        ToolData::NginxTest(value) => Some(("nginx-config-valid".into(), value.valid.to_string())),
        ToolData::Diagnostic(value) => diagnostic_comparison(value.category, &value.fields),
        _ => None,
    }
}

fn diagnostic_comparison(category: &str, fields: &serde_json::Value) -> Option<(String, String)> {
    match category {
        "dns" => Some(("dns-resolved".into(), fields["resolved"].to_string())),
        "tls" => Some(("tls-verified".into(), fields["verified"].to_string())),
        "docker-inspect" => Some((
            "docker-state".into(),
            format!(
                "{}|{}|{}",
                fields["state"].as_str().unwrap_or("unknown"),
                fields["health"].as_str().unwrap_or("unknown"),
                fields["restartCount"].as_u64().unwrap_or_default()
            ),
        )),
        _ => None,
    }
}

fn bounded_text(value: String, max: usize) -> AppResult<String> {
    let value = value.trim().to_string();
    if value.is_empty() || value.len() > max {
        Err(AppError::InvalidOperation)
    } else {
        Ok(value)
    }
}

fn bounded_owner(value: String) -> AppResult<String> {
    let value = bounded_text(value, 80)?;
    if value
        .chars()
        .all(|character| character.is_alphanumeric() || " ._@-".contains(character))
    {
        Ok(value)
    } else {
        Err(AppError::InvalidOperation)
    }
}

fn trim_incidents(items: &mut HashMap<Uuid, Incident>) {
    while items.len() > MAX_INCIDENTS {
        let oldest = items
            .values()
            .min_by_key(|incident| {
                incident
                    .timeline
                    .first()
                    .map(|event| event.occurred_at_epoch_ms)
                    .unwrap_or_default()
            })
            .map(|incident| incident.id);
        if let Some(id) = oldest {
            items.remove(&id);
        } else {
            break;
        }
    }
}

fn valid_persisted_incident(record: &PersistedIncident) -> bool {
    let targets = exact_targets(&record.targets);
    let evidence_ids = record
        .evidence_references
        .iter()
        .map(|item| item.id)
        .collect::<HashSet<_>>();
    matches!(
        record.schema_version,
        LEGACY_INCIDENT_SCHEMA_VERSION | INCIDENT_SCHEMA_VERSION
    ) && !record.targets.is_empty()
        && record.targets.len() <= MAX_TARGETS
        && targets.len() == record.targets.len()
        && !record.timeline.is_empty()
        && record.timeline.len() <= MAX_INCIDENT_TIMELINE_EVENTS
        && !record.model.is_empty()
        && record.model.len() <= 128
        && record.root_cause.confidence.is_finite()
        && (0.0..=1.0).contains(&record.root_cause.confidence)
        && !record.root_cause.evidence_ids.is_empty()
        && record
            .root_cause
            .evidence_ids
            .iter()
            .all(|id| evidence_ids.contains(id))
        && evidence_ids.len() == record.evidence_references.len()
        && record.evidence_references.len() <= MAX_TOOL_CALLS_PER_TARGET * record.targets.len()
        && record
            .evidence_references
            .iter()
            .all(|item| targets.contains(&item.target_id) && item.source.starts_with("tool."))
        && record.comparisons.iter().all(|comparison| {
            !comparison.dimension.is_empty()
                && comparison.dimension.len() <= 128
                && exact_targets(
                    &comparison
                        .values
                        .iter()
                        .map(|value| value.target_id)
                        .collect::<Vec<_>>(),
                ) == targets
                && comparison
                    .values
                    .iter()
                    .all(|value| value.signal.len() <= 256)
        })
        && record.change_set.as_ref().is_none_or(|change_set| {
            change_set.version > 0
                && matches!(change_set.kind.as_str(), "single" | "fleet")
                && exact_targets(&change_set.exact_target_ids) == targets
                && change_set.exact_target_ids.len() == record.targets.len()
        })
}

fn plan_for(request: &IncidentRequest) -> AppResult<Vec<NativeToolInvocation>> {
    let mut tools = vec![NativeToolInvocation::SystemInfo];
    match request.pack {
        OperationsPack::Website => {
            let host = required(request.host.clone())?;
            let port = request.port.unwrap_or(443);
            let url = required(request.url.clone())?;
            tools.extend([
                NativeToolInvocation::DnsResolve { host: host.clone() },
                NativeToolInvocation::NetworkPortCheck {
                    host: host.clone(),
                    port,
                },
                NativeToolInvocation::TlsInspect { host, port },
                NativeToolInvocation::HttpRequest { url },
                NativeToolInvocation::NginxTest,
                NativeToolInvocation::NetworkListeners,
                NativeToolInvocation::ProcessList,
            ]);
            add_service(&mut tools, request.service.as_deref().unwrap_or("nginx"));
            if let Some(upstream_host) = &request.upstream_host {
                tools.push(NativeToolInvocation::NetworkPortCheck {
                    host: upstream_host.clone(),
                    port: request.upstream_port.ok_or(AppError::InvalidOperation)?,
                });
            }
        }
        OperationsPack::Nginx => {
            add_service(&mut tools, "nginx");
            tools.push(NativeToolInvocation::NginxTest);
            tools.push(NativeToolInvocation::NetworkListeners);
            tools.push(NativeToolInvocation::ProcessList);
            if let Some(url) = &request.url {
                tools.push(NativeToolInvocation::HttpRequest { url: url.clone() });
            }
            if let (Some(host), Some(port)) = (&request.host, request.port) {
                tools.push(NativeToolInvocation::NetworkPortCheck {
                    host: host.clone(),
                    port,
                });
                tools.push(NativeToolInvocation::TlsInspect {
                    host: host.clone(),
                    port,
                });
            }
            if let Some(path) = &request.config_path {
                tools.push(NativeToolInvocation::FileInspect { path: path.clone() });
            }
        }
        OperationsPack::Docker => {
            let container = required(request.container.clone())?;
            tools.extend([
                NativeToolInvocation::DockerList,
                NativeToolInvocation::DockerInspect {
                    container: container.clone(),
                },
                NativeToolInvocation::DockerLogs {
                    container,
                    lines: 200,
                },
                NativeToolInvocation::NetworkListeners,
                NativeToolInvocation::ProcessList,
            ]);
            if let Some(path) = &request.config_path {
                tools.push(NativeToolInvocation::FileInspect { path: path.clone() });
            }
        }
        OperationsPack::Disk => {
            tools.push(NativeToolInvocation::SystemDisk);
            tools.push(NativeToolInvocation::DockerList);
            for path in ["/var/log", "/var/lib/docker", "/var/backups", "/tmp"] {
                tools.push(NativeToolInvocation::SystemDirectoryUsage { path: path.into() });
                tools.push(NativeToolInvocation::SystemLargeFiles {
                    path: path.into(),
                    minimum_bytes: 10 * 1024 * 1024,
                });
            }
        }
        OperationsPack::Service => {
            let service = required(request.service.clone())?;
            add_service(&mut tools, &service);
            tools.push(NativeToolInvocation::ProcessList);
            tools.push(NativeToolInvocation::NetworkListeners);
            if let Some(port) = request.port {
                tools.push(NativeToolInvocation::NetworkPortCheck {
                    host: request.host.clone().unwrap_or_else(|| "127.0.0.1".into()),
                    port,
                });
            }
            if let Some(path) = &request.config_path {
                tools.push(NativeToolInvocation::FileInspect { path: path.clone() });
            }
            for dependency in &request.dependencies {
                tools.push(NativeToolInvocation::ServiceStatus {
                    service: dependency.clone(),
                });
            }
        }
    }
    Ok(tools)
}

fn add_service(tools: &mut Vec<NativeToolInvocation>, service: &str) {
    tools.push(NativeToolInvocation::ServiceStatus {
        service: service.into(),
    });
    tools.push(NativeToolInvocation::ServiceLogs {
        service: service.into(),
        lines: 200,
    });
}

fn diagnose(
    pack: OperationsPack,
    evidence: &[IncidentEvidence],
) -> (IncidentRootCause, Vec<IncidentHypothesis>, ProposedFix) {
    let mut candidates: Vec<(&'static str, f32, Uuid)> = Vec::new();
    for item in evidence {
        if !item.result.success {
            continue;
        }
        match item.result.data.as_ref() {
            Some(ToolData::NginxTest(value)) if !value.valid => {
                candidates.push(("nginx-invalid-config", 0.98, item.id))
            }
            Some(ToolData::ServiceStatus(value))
                if !matches!(value.service.status, ServiceStatus::Active) =>
            {
                candidates.push(("service-not-active", 0.93, item.id))
            }
            Some(ToolData::HttpResponse(value)) if value.status_code == 502 => {
                candidates.push(("nginx-upstream-unavailable", 0.94, item.id))
            }
            Some(ToolData::HttpResponse(value)) if value.status_code == 503 => {
                candidates.push(("service-unavailable", 0.92, item.id))
            }
            Some(ToolData::HttpResponse(value)) if value.status_code == 504 => {
                candidates.push(("upstream-timeout", 0.92, item.id))
            }
            Some(ToolData::SystemDisk(value))
                if value.disks.iter().any(|disk| disk.usage_percent >= 95.0) =>
            {
                candidates.push(("filesystem-capacity-exhausted", 0.97, item.id))
            }
            Some(ToolData::Diagnostic(value)) => {
                inspect_diagnostic(value.category, &value.fields, item.id, &mut candidates)
            }
            _ => {}
        }
    }
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
    let (code, confidence, ids) = candidates
        .first()
        .map(|(code, confidence, _)| {
            (
                *code,
                *confidence,
                candidates
                    .iter()
                    .filter(|(candidate, _, _)| candidate == code)
                    .map(|(_, _, id)| *id)
                    .collect(),
            )
        })
        .unwrap_or((
            "incident-inconclusive",
            0.35,
            evidence.iter().take(3).map(|item| item.id).collect(),
        ));
    let hypotheses = candidates
        .iter()
        .take(5)
        .map(|(candidate, _, id)| IncidentHypothesis {
            code: (*candidate).into(),
            evidence_ids: vec![*id],
            supported: *candidate == code,
        })
        .collect();
    let (fix_code, actions, risk) = if code == "incident-inconclusive" {
        (
            "continue-investigation",
            vec!["collect-targeted-evidence"],
            RiskLevel::R1,
        )
    } else {
        fix_for(pack, code)
    };
    (
        IncidentRootCause {
            code: code.into(),
            confidence,
            evidence_ids: ids,
        },
        hypotheses,
        ProposedFix {
            code: fix_code.into(),
            risk,
            action_codes: actions.into_iter().map(str::to_string).collect(),
            change_set_required: code != "incident-inconclusive",
            execution_pipeline: ["changeset", "approval", "execute", "verify", "rollback"]
                .map(str::to_string),
            dangerous_deletion_blocked: pack == OperationsPack::Disk,
        },
    )
}

fn inspect_diagnostic(
    category: &str,
    fields: &serde_json::Value,
    id: Uuid,
    out: &mut Vec<(&'static str, f32, Uuid)>,
) {
    match category {
        "dns" if fields["resolved"] == false => out.push(("dns-resolution-failed", 0.98, id)),
        "tls" if fields["verified"] == false => out.push(("tls-validation-failed", 0.97, id)),
        "docker-inspect" => {
            if fields["health"] == "unhealthy" {
                out.push(("docker-unhealthy", 0.97, id));
            } else if fields["restartCount"].as_u64().unwrap_or(0) >= 3 {
                out.push(("docker-restart-loop", 0.95, id));
            } else if fields["state"] == "exited" {
                out.push(("docker-exited", 0.94, id));
            }
            let error = fields["error"].as_str().unwrap_or("").to_ascii_lowercase();
            if error.contains("permission") {
                out.push(("docker-volume-permission", 0.96, id));
            }
            if error.contains("address already in use") {
                out.push(("docker-port-conflict", 0.96, id));
            }
        }
        "docker-logs" => {
            let raw = fields["entries"].to_string().to_ascii_lowercase();
            if raw.contains("permission denied") {
                out.push(("docker-volume-permission", 0.9, id));
            } else if raw.contains("address already in use") {
                out.push(("docker-port-conflict", 0.9, id));
            } else if raw.contains("panic") || raw.contains("fatal") {
                out.push(("docker-application-failure", 0.86, id));
            }
        }
        "file"
            if fields["mode"]
                .as_str()
                .is_some_and(|mode| mode.ends_with("00")) =>
        {
            out.push(("config-permission-denied", 0.82, id))
        }
        _ => {}
    }
}

fn fix_for(pack: OperationsPack, cause: &str) -> (&'static str, Vec<&'static str>, RiskLevel) {
    match (pack, cause) {
        (OperationsPack::Nginx | OperationsPack::Website, "nginx-invalid-config") => (
            "repair-nginx-config",
            vec![
                "draft-file-patch",
                "nginx-test",
                "nginx-reload",
                "http-verify",
            ],
            RiskLevel::R3,
        ),
        (OperationsPack::Docker, _) => (
            "repair-docker-container",
            vec![
                "review-container-config",
                "draft-docker-restart",
                "health-verify",
                "port-verify",
            ],
            RiskLevel::R3,
        ),
        (OperationsPack::Disk, _) => (
            "review-disk-reclamation",
            vec![
                "review-large-files",
                "review-retention-policy",
                "draft-explicit-change",
                "disk-verify",
                "service-impact-verify",
            ],
            RiskLevel::R4,
        ),
        (OperationsPack::Service, _) => (
            "repair-linux-service",
            vec![
                "review-config-permission",
                "draft-service-change",
                "status-verify",
                "port-verify",
            ],
            RiskLevel::R3,
        ),
        _ => (
            "repair-website-chain",
            vec![
                "repair-confirmed-layer",
                "http-verify",
                "cross-target-verify",
            ],
            RiskLevel::R3,
        ),
    }
}

fn validate_request(request: &IncidentRequest) -> AppResult<()> {
    if request.targets.is_empty()
        || request.targets.len() > MAX_TARGETS
        || exact_targets(&request.targets).len() != request.targets.len()
        || request.symptoms.is_empty()
        || request.symptoms.len() > MAX_SYMPTOMS
        || request
            .symptoms
            .iter()
            .any(|item| item.is_empty() || item.len() > 2048)
        || request.dependencies.len() > 8
        || request
            .dependencies
            .iter()
            .any(|item| item.is_empty() || item.len() > 128)
        || request
            .host
            .as_ref()
            .is_some_and(|item| item.is_empty() || item.len() > 253)
        || request
            .upstream_host
            .as_ref()
            .is_some_and(|item| item.is_empty() || item.len() > 253)
        || request.upstream_host.is_some() != request.upstream_port.is_some()
    {
        return Err(AppError::InvalidOperation);
    }
    Ok(())
}
fn exact_targets(values: &[Uuid]) -> HashSet<Uuid> {
    values.iter().copied().collect()
}
fn required(value: Option<String>) -> AppResult<String> {
    value
        .filter(|item| !item.is_empty())
        .ok_or(AppError::InvalidOperation)
}
fn event(status: IncidentStatus, code: &'static str) -> IncidentTimelineEvent {
    IncidentTimelineEvent {
        status,
        code: code.into(),
        occurred_at_epoch_ms: now_ms(),
    }
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DiskUsage, ServiceHealth};

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        pack: OperationsPack,
        signal: serde_json::Value,
        expected_root_cause: String,
    }
    fn evidence(data: ToolData) -> IncidentEvidence {
        evidence_for(Uuid::new_v4(), data)
    }
    fn evidence_for(target_id: Uuid, data: ToolData) -> IncidentEvidence {
        IncidentEvidence {
            id: Uuid::new_v4(),
            target_id,
            source: "tool.fixture".into(),
            summary: "fixture",
            result: ToolResult {
                invocation_id: Uuid::new_v4(),
                tool_name: crate::tools::NativeToolName::SystemInfo,
                success: true,
                summary: "fixture",
                data: Some(data),
                error_code: None,
                warnings: vec![],
                started_at_epoch_ms: 0,
                duration_ms: 1,
                truncated: false,
                cancelled: false,
                untrusted_remote_data: true,
            },
        }
    }
    fn sample_incident(status: IncidentStatus, verification: &str) -> Incident {
        let target = Uuid::new_v4();
        let item = evidence_for(
            target,
            ToolData::HttpResponse(crate::tools::HttpResponseData {
                status_code: 502,
                content_type: Some("text/plain".into()),
                body_preview: "remote-secret-body".into(),
                body_bytes: 18,
            }),
        );
        let (root_cause, hypotheses, proposed_fix) =
            diagnose(OperationsPack::Website, std::slice::from_ref(&item));
        let id = Uuid::new_v4();
        let agent_run_id = Uuid::new_v4();
        let mut metrics = AgentRunMetrics::new(agent_run_id, Some(id));
        metrics.tool_calls = 1;
        metrics.duration_ms = 10;
        Incident {
            id,
            agent_run_id,
            model: "fixture-model".into(),
            pack: OperationsPack::Website,
            status,
            severity: IncidentSeverity::Sev2,
            targets: vec![target],
            symptoms: vec!["secret symptom".into()],
            evidence_references: vec![evidence_reference(&item)],
            evidence: vec![item],
            comparisons: Vec::new(),
            hypotheses,
            root_cause,
            proposed_fix,
            change_set: None,
            verification: IncidentVerification {
                status_code: verification.into(),
                evidence_ids: Vec::new(),
            },
            resolution: None,
            handoff: None,
            timeline: vec![event(status, "incident-reported")],
            duration_ms: 10,
            recovery_state: IncidentRecoveryState::Live,
            metrics,
        }
    }
    #[test]
    fn root_cause_always_binds_evidence() {
        let item = evidence(ToolData::Diagnostic(crate::tools::DiagnosticData {
            category: "docker-inspect",
            fields: serde_json::json!({"state":"exited","restartCount":0,"health":null,"error":""}),
        }));
        let (root, _, _) = diagnose(OperationsPack::Docker, &[item]);
        assert_eq!(root.code, "docker-exited");
        assert_eq!(root.evidence_ids.len(), 1);
    }
    #[test]
    fn disk_pack_never_proposes_automatic_deletion() {
        let (_, _, fix) = diagnose(OperationsPack::Disk, &[]);
        assert!(fix.dangerous_deletion_blocked);
        assert!(!fix.action_codes.iter().any(|code| code.contains("delete")));
    }

    #[test]
    fn website_plan_keeps_public_and_upstream_probes_separate() {
        let request = IncidentRequest {
            id: Uuid::new_v4(),
            pack: OperationsPack::Website,
            severity: IncidentSeverity::Sev2,
            targets: vec![Uuid::new_v4()],
            symptoms: vec!["502".into()],
            host: Some("public.example".into()),
            port: Some(443),
            url: Some("https://public.example".into()),
            service: Some("app".into()),
            container: None,
            config_path: None,
            upstream_host: Some("10.0.0.8".into()),
            upstream_port: Some(8080),
            dependencies: Vec::new(),
        };
        let plan = plan_for(&request).expect("plan");
        assert!(plan.iter().any(|item| matches!(item, NativeToolInvocation::NetworkPortCheck { host, port: 443 } if host == "public.example")));
        assert!(plan.iter().any(|item| matches!(item, NativeToolInvocation::NetworkPortCheck { host, port: 8080 } if host == "10.0.0.8")));
    }

    #[test]
    fn cross_target_comparison_is_deterministic_and_marks_drift() {
        let first = Uuid::from_u128(1);
        let second = Uuid::from_u128(2);
        let evidence = vec![
            evidence_for(
                first,
                ToolData::HttpResponse(crate::tools::HttpResponseData {
                    status_code: 200,
                    content_type: None,
                    body_preview: String::new(),
                    body_bytes: 0,
                }),
            ),
            evidence_for(
                second,
                ToolData::HttpResponse(crate::tools::HttpResponseData {
                    status_code: 502,
                    content_type: None,
                    body_preview: String::new(),
                    body_bytes: 0,
                }),
            ),
        ];
        let comparisons = compare_targets(&[second, first], &evidence);
        assert_eq!(comparisons.len(), 1);
        assert_eq!(comparisons[0].dimension, "http-status");
        assert!(comparisons[0].drift);
        assert_eq!(comparisons[0].values[0].target_id, first);
    }

    #[tokio::test]
    async fn persistence_is_content_free_and_recovery_is_metadata_only() {
        let directory = tempfile::tempdir().expect("directory");
        let path = directory.path().join("incidents.json");
        let service = IncidentService::at_path(path.clone());
        let incident = sample_incident(IncidentStatus::Executing, "pending");
        service
            .items
            .lock()
            .await
            .insert(incident.id, incident.clone());
        {
            let items = service.items.lock().await;
            service.persist_locked(&items).await.expect("persist");
        }
        let raw = std::fs::read_to_string(&path).expect("read");
        assert!(!raw.contains("remote-secret-body"));
        assert!(!raw.contains("secret symptom"));
        let recovered = IncidentService::at_path(path);
        recovered.load().await.expect("load");
        let item = recovered.get(incident.id).await.expect("incident");
        assert_eq!(item.status, IncidentStatus::Interrupted);
        assert_eq!(item.recovery_state, IncidentRecoveryState::MetadataOnly);
        assert!(item.evidence.is_empty());
        assert_eq!(item.evidence_references.len(), 1);
    }

    #[tokio::test]
    async fn legacy_v1_incident_metadata_migrates_to_v2() {
        let directory = tempfile::tempdir().expect("directory");
        let path = directory.path().join("incidents.json");
        let service = IncidentService::at_path(path.clone());
        let incident = sample_incident(IncidentStatus::Resolved, "succeeded");
        service
            .items
            .lock()
            .await
            .insert(incident.id, incident.clone());
        {
            let items = service.items.lock().await;
            service.persist_locked(&items).await.expect("persist");
        }
        let mut records: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(&path).await.expect("read")).expect("json");
        records[0]["schemaVersion"] = serde_json::json!(1);
        tokio::fs::write(&path, serde_json::to_vec_pretty(&records).expect("encode"))
            .await
            .expect("legacy fixture");
        let recovered = IncidentService::at_path(path.clone());
        recovered.load().await.expect("migrate");
        let migrated = tokio::fs::read_to_string(path)
            .await
            .expect("migrated file");
        assert!(migrated.contains("\"schemaVersion\": 2"));
        assert_eq!(
            recovered
                .get(incident.id)
                .await
                .expect("incident")
                .recovery_state,
            IncidentRecoveryState::MetadataOnly
        );
    }

    #[tokio::test]
    async fn malformed_incident_repository_fails_closed_without_overwrite() {
        let directory = tempfile::tempdir().expect("directory");
        let path = directory.path().join("incidents.json");
        let malformed = b"[{not-valid-json";
        tokio::fs::write(&path, malformed).await.expect("fixture");
        let service = IncidentService::at_path(path.clone());
        assert!(matches!(service.load().await, Err(AppError::Storage)));
        assert_eq!(tokio::fs::read(path).await.expect("preserved"), malformed);
    }

    #[tokio::test]
    async fn dangling_evidence_reference_fails_closed_without_rewrite() {
        let directory = tempfile::tempdir().expect("directory");
        let path = directory.path().join("incidents.json");
        let service = IncidentService::at_path(path.clone());
        let incident = sample_incident(IncidentStatus::Resolved, "succeeded");
        service.items.lock().await.insert(incident.id, incident);
        {
            let items = service.items.lock().await;
            service.persist_locked(&items).await.expect("persist");
        }
        let mut records: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(&path).await.expect("read")).expect("json");
        records[0]["rootCause"]["evidenceIds"][0] = serde_json::json!(Uuid::new_v4());
        let tampered = serde_json::to_vec_pretty(&records).expect("tampered json");
        tokio::fs::write(&path, &tampered).await.expect("tamper");
        let recovered = IncidentService::at_path(path.clone());
        assert!(matches!(recovered.load().await, Err(AppError::Storage)));
        assert_eq!(tokio::fs::read(path).await.expect("preserved"), tampered);
    }

    #[test]
    fn incident_memory_retention_is_bounded_deterministically() {
        let mut incidents = HashMap::new();
        for index in 0..=MAX_INCIDENTS {
            let mut incident = sample_incident(IncidentStatus::Resolved, "succeeded");
            incident.id = Uuid::from_u128(index as u128 + 1);
            incident.timeline[0].occurred_at_epoch_ms = index as u64;
            incidents.insert(incident.id, incident);
        }
        trim_incidents(&mut incidents);
        assert_eq!(incidents.len(), MAX_INCIDENTS);
        assert!(!incidents.contains_key(&Uuid::from_u128(1)));
        assert!(incidents.contains_key(&Uuid::from_u128(MAX_INCIDENTS as u128 + 1)));
    }

    #[tokio::test]
    async fn closure_requires_successful_verification_for_resolved_claim() {
        let service = IncidentService::default();
        let incident = sample_incident(IncidentStatus::Failed, "failed");
        service
            .items
            .lock()
            .await
            .insert(incident.id, incident.clone());
        let rejected = service
            .close(
                incident.id,
                IncidentClosureRequest {
                    code: IncidentClosureCode::Resolved,
                    summary: "operator reviewed".into(),
                },
            )
            .await;
        assert!(matches!(rejected, Err(AppError::InvalidOperation)));
        let accepted = service
            .close(
                incident.id,
                IncidentClosureRequest {
                    code: IncidentClosureCode::AcceptedRisk,
                    summary: "accepted after review".into(),
                },
            )
            .await
            .expect("accepted risk");
        assert_eq!(accepted.status, IncidentStatus::Closed);
    }

    #[tokio::test]
    async fn audit_export_contains_only_sanitized_metadata() {
        let service = IncidentService::default();
        let incident = sample_incident(IncidentStatus::Failed, "failed");
        service
            .items
            .lock()
            .await
            .insert(incident.id, incident.clone());
        let encoded = serde_json::to_string(&service.export(incident.id).await.expect("export"))
            .expect("json");
        assert!(encoded.contains("contentOmitted"));
        assert!(!encoded.contains("remote-secret-body"));
        assert!(!encoded.contains("secret symptom"));
    }

    #[test]
    fn deterministic_operations_pack_fixtures_bind_expected_root_causes() {
        for raw in [
            include_str!("../../tests/fixtures/agentic/incident-website.json"),
            include_str!("../../tests/fixtures/agentic/incident-nginx.json"),
            include_str!("../../tests/fixtures/agentic/incident-docker.json"),
            include_str!("../../tests/fixtures/agentic/incident-disk.json"),
            include_str!("../../tests/fixtures/agentic/incident-service.json"),
        ] {
            let fixture: Fixture = serde_json::from_str(raw).expect("fixture");
            let data = match fixture.signal["kind"].as_str().expect("kind") {
                "http-status" => ToolData::HttpResponse(crate::tools::HttpResponseData {
                    status_code: fixture.signal["statusCode"].as_u64().unwrap_or_default() as u16,
                    content_type: None,
                    body_preview: String::new(),
                    body_bytes: 0,
                }),
                "nginx-test" => ToolData::NginxTest(crate::tools::NginxTestData {
                    valid: fixture.signal["valid"].as_bool().unwrap_or(false),
                    config_file: None,
                    error_file: Some("/etc/nginx/nginx.conf".into()),
                    error_line: Some(12),
                    error_message: Some("fixture".into()),
                    raw_summary: "fixture".into(),
                }),
                "docker-inspect" => ToolData::Diagnostic(crate::tools::DiagnosticData {
                    category: "docker-inspect",
                    fields: fixture.signal.clone(),
                }),
                "disk" => ToolData::SystemDisk(crate::tools::SystemDiskData {
                    disks: vec![DiskUsage {
                        mount: "/".into(),
                        used_bytes: 98,
                        total_bytes: 100,
                        usage_percent: fixture.signal["usagePercent"].as_f64().unwrap_or_default(),
                    }],
                }),
                "service-status" => ToolData::ServiceStatus(crate::tools::ServiceStatusData {
                    service: ServiceHealth {
                        name: "api".into(),
                        status: if fixture.signal["active"].as_bool().unwrap_or(false) {
                            ServiceStatus::Active
                        } else {
                            ServiceStatus::Failed
                        },
                    },
                }),
                _ => panic!("unknown fixture signal"),
            };
            let (root, _, _) = diagnose(fixture.pack, &[evidence(data)]);
            assert_eq!(root.code, fixture.expected_root_cause);
            assert!(!root.evidence_ids.is_empty());
        }
    }
}
