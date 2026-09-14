use super::*;
use crate::agentic::{ChangeSetDraftRequest, ChangeStepDraft};
use crate::storage::JsonRepository;
use crate::tools::ToolAuditRepository;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncidentFixture {
    agent_run_id: Uuid,
    target_ids: Vec<Uuid>,
    failed_target_id: Uuid,
}

fn incident_fixture() -> IncidentFixture {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/agentic/nginx-partial-failure.json"
    ))
    .expect("valid deterministic incident fixture")
}

fn target(id: u128) -> ChangeSetDraftRequest {
    ChangeSetDraftRequest {
        agent_run_id: Uuid::from_u128(100),
        session_id: Uuid::from_u128(id),
        title: format!("target-{id}"),
        steps: vec![ChangeStepDraft::NginxReload],
    }
}

fn request(strategy: ExecutionStrategy, policy: FailurePolicy) -> MultiChangeSetDraftRequest {
    MultiChangeSetDraftRequest {
        agent_run_id: Uuid::from_u128(100),
        title: "deterministic-nginx-incident".into(),
        targets: vec![target(1), target(2), target(3), target(4)],
        execution_strategy: strategy,
        failure_policy: policy,
        batch_size: 2,
        canary_count: 1,
        production: true,
        service_verification: Some("nginx".into()),
        cross_target_verification: true,
    }
}

#[tokio::test]
async fn deterministic_fixture_binds_exact_targets_and_versions() {
    let fixture = incident_fixture();
    let changes = ChangeSetService::default();
    let fleets = FleetExecutionService::default();
    let draft = fleets
        .draft(
            &changes,
            request(ExecutionStrategy::Canary, FailurePolicy::PauseForReview),
        )
        .await
        .expect("fleet draft");
    assert_eq!(draft.targets.len(), 4);
    assert_eq!(draft.agent_run_id, fixture.agent_run_id);
    assert_eq!(
        draft
            .targets
            .iter()
            .map(|item| item.target_id)
            .collect::<Vec<_>>(),
        fixture.target_ids
    );
    assert_eq!(draft.risk, RiskLevel::R2);
    assert!(fleets
        .approve(&changes, draft.id, draft.version, vec![Uuid::from_u128(1)])
        .await
        .is_err());
    let approved = fleets
        .approve(
            &changes,
            draft.id,
            draft.version,
            draft.targets.iter().map(|item| item.target_id).collect(),
        )
        .await
        .expect("exact approval");
    let binding = approved.approval.expect("approval binding");
    assert_eq!(binding.fleet_version, 1);
    assert_eq!(binding.targets.len(), 4);
    assert!(binding
        .targets
        .iter()
        .all(|item| item.change_set_version == 1));
}

#[tokio::test]
async fn target_revision_automatically_invalidates_fleet_approval() {
    let changes = ChangeSetService::default();
    let fleets = FleetExecutionService::default();
    let draft = fleets
        .draft(
            &changes,
            request(ExecutionStrategy::Sequential, FailurePolicy::Stop),
        )
        .await
        .expect("fleet draft");
    fleets
        .approve(
            &changes,
            draft.id,
            1,
            draft.targets.iter().map(|item| item.target_id).collect(),
        )
        .await
        .expect("approve");
    fleets
        .invalidate_target(
            draft.targets[0].change_set_id,
            draft.targets[0].target_id,
            2,
        )
        .await
        .expect("invalidate");
    let invalidated = fleets.get(draft.id).await.expect("fleet");
    assert_eq!(invalidated.approval_state, ApprovalState::Invalidated);
    assert_eq!(invalidated.version, 2);
    assert!(invalidated.approval.is_none());
}

#[tokio::test]
async fn production_parallel_all_is_rejected_and_batches_are_deterministic() {
    let changes = ChangeSetService::default();
    let fleets = FleetExecutionService::default();
    assert!(fleets
        .draft(
            &changes,
            request(ExecutionStrategy::Parallel, FailurePolicy::Continue),
        )
        .await
        .is_err());
    let rolling = fleets
        .draft(
            &changes,
            request(
                ExecutionStrategy::RollingBatch,
                FailurePolicy::PauseForReview,
            ),
        )
        .await
        .expect("rolling draft");
    assert_eq!(
        execution_batches(&rolling).expect("batches"),
        vec![vec![0, 1], vec![2, 3]]
    );
}

#[tokio::test]
async fn recovery_preserves_per_target_state_but_invalidates_approval() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("fleet-runs.json");
    let changes = ChangeSetService::default();
    let fleets = FleetExecutionService::at_path(path.clone());
    let draft = fleets
        .draft(
            &changes,
            request(ExecutionStrategy::Sequential, FailurePolicy::Rollback),
        )
        .await
        .expect("fleet draft");
    fleets
        .approve(
            &changes,
            draft.id,
            1,
            draft.targets.iter().map(|item| item.target_id).collect(),
        )
        .await
        .expect("approve");
    fleets
        .set_state(
            draft.id,
            FleetExecutionState::Executing,
            "FIXTURE_EXECUTING",
        )
        .await
        .expect("state");
    let serialized = tokio::fs::read_to_string(&path).await.expect("persisted");
    assert!(!serialized.contains("deterministic-nginx-incident"));
    let recovered = FleetExecutionService::at_path(path);
    recovered.load().await.expect("load");
    let run = recovered.get(draft.id).await.expect("run");
    assert_eq!(run.execution_state, FleetExecutionState::Interrupted);
    assert_eq!(run.approval_state, ApprovalState::Invalidated);
    assert_eq!(run.recovery_state, FleetRecoveryState::MetadataOnly);
    assert_eq!(run.targets.len(), 4);
    assert!(run.last_approval.is_some());
    assert!(run.audit.iter().any(|event| event.code == "FLEET_APPROVED"));
}

#[tokio::test]
async fn pause_resume_batches_only_pending_targets() {
    let changes = ChangeSetService::default();
    let fleets = FleetExecutionService::default();
    let draft = fleets
        .draft(
            &changes,
            request(ExecutionStrategy::Sequential, FailurePolicy::PauseForReview),
        )
        .await
        .expect("fleet draft");
    let mut resumed = draft.clone();
    resumed.execution_state = FleetExecutionState::PausedForReview;
    resumed.targets[0].state = FleetTargetState::Succeeded;
    resumed.targets[0].local_verification = VerificationState::Succeeded;
    resumed.targets[1].state = FleetTargetState::Failed;
    resumed.targets[1].local_verification = VerificationState::Failed;
    assert_eq!(
        execution_batches(&resumed).expect("resume batches"),
        vec![vec![2], vec![3]]
    );
}

#[tokio::test]
async fn explicit_rollback_is_hidden_without_a_completed_reversible_step() {
    let changes = ChangeSetService::default();
    let fleets = FleetExecutionService::default();
    let draft = fleets
        .draft(
            &changes,
            request(ExecutionStrategy::Sequential, FailurePolicy::PauseForReview),
        )
        .await
        .expect("fleet draft");
    {
        let mut items = fleets.items.lock().await;
        let run = items.get_mut(&draft.id).expect("run");
        run.execution_state = FleetExecutionState::Failed;
        run.targets[0].state = FleetTargetState::Succeeded;
    }
    let sessions = ServerSessionManager::default();
    let audit_directory = tempfile::tempdir().expect("temporary audit directory");
    let tools = NativeToolExecutionService::foundation(ToolAuditRepository::new(
        JsonRepository::new(audit_directory.path().join("tool-audit.json")),
    ));
    assert!(matches!(
        fleets
            .rollback_explicit(&changes, &sessions, &tools, draft.id, draft.version,)
            .await,
        Err(AppError::InvalidOperation)
    ));
}

#[test]
fn fixture_verification_and_rollback_states_are_independently_traceable() {
    let fixture = incident_fixture();
    let mut targets = fixture
        .target_ids
        .iter()
        .map(|target_id| FleetTargetExecution {
            target_id: *target_id,
            change_set_id: Uuid::new_v4(),
            change_set_version: 1,
            state: FleetTargetState::Succeeded,
            local_verification: VerificationState::Succeeded,
            service_verification: VerificationState::Succeeded,
            rollback_state: VerificationState::NotApplicable,
            error_code: None,
            duration_ms: Some(10),
        })
        .collect::<Vec<_>>();
    let failed = targets
        .iter_mut()
        .find(|target| target.target_id == fixture.failed_target_id)
        .expect("fixture failed target");
    failed.state = FleetTargetState::Failed;
    failed.local_verification = VerificationState::Failed;
    failed.service_verification = VerificationState::Failed;
    failed.error_code = Some("EXEC_FAILED".into());
    targets[0].state = FleetTargetState::RolledBack;
    targets[0].rollback_state = VerificationState::Succeeded;
    targets[2].state = FleetTargetState::RollbackFailed;
    targets[2].rollback_state = VerificationState::Failed;
    assert_eq!(verification_outcome(&targets, true), (false, false));
    assert_eq!(targets[0].rollback_state, VerificationState::Succeeded);
    assert_eq!(targets[1].state, FleetTargetState::Failed);
    assert_eq!(targets[2].rollback_state, VerificationState::Failed);
    assert_eq!(targets[3].state, FleetTargetState::Succeeded);
}
