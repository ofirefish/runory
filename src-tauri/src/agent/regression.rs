//! Runtime V2 release-gate regression scenarios (AR2-I).
//!
//! Named acceptance tests mapped to `AGENT_RUNTIME_V2.md` §36 and §39.
//! Each scenario uses the in-memory controller harness — no Docker fixture
//! required — and validates orchestration semantics, not remote SSH truth.

#[cfg(test)]
mod scenarios {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use async_trait::async_trait;
    use serde_json::json;
    use uuid::Uuid;

    use crate::agent::artifact::InMemoryArtifactStore;
    use crate::agent::changeset::{ChangeSetExecutor, ChangeSetExecutorError};
    use crate::agent::controller::{
        AgentController, AgentControllerConfig, AgentControllerError, AgentStores, RunBudget,
        RunOutcome, AGENT_BUDGET_EXCEEDED,
    };
    use crate::agent::decision::{
        validate_decision, AgentDecision, CommandProposalRequest, PreparedCommandProposal,
        PreparedToolCall, ToolCallRequest, ValidatedDecision,
    };
    use crate::agent::dispatch::{ToolDispatcher, ToolOutcome};
    use crate::agent::gate::{
        Authorization, AuthorizationGate, AutoAuthorizationGate, FixedPolicyMatcher,
        FnAuthorizationGate,
    };
    use crate::agent::reasoner::{Reasoner, ReasonerError, ReasonerInput};
    use crate::agent::repository::{
        InMemoryAgentEventRepository, InMemoryAgentRunStore, InMemoryApprovalStore,
        InMemoryCheckpointStore, InMemoryPendingToolCallStore,
    };
    use crate::agent::state::AgentRunStateV2;
    use crate::agent::CommandOutcome;
    use crate::agentic::{ChangeSet, ChangeStepDraft};

    struct FnReasoner<F>(F);
    #[async_trait]
    impl<F> Reasoner for FnReasoner<F>
    where
        F: for<'a> Fn(&ReasonerInput<'a>) -> Result<AgentDecision, ReasonerError> + Send + Sync,
    {
        async fn decide(&self, input: &ReasonerInput<'_>) -> Result<AgentDecision, ReasonerError> {
            (self.0)(input)
        }
    }

    struct FnDispatcher<F>(F);
    #[async_trait]
    impl<F> ToolDispatcher for FnDispatcher<F>
    where
        F: Fn(&PreparedToolCall) -> ToolOutcome + Send + Sync,
    {
        async fn execute_reads(
            &self,
            _run_id: Uuid,
            calls: &[PreparedToolCall],
        ) -> Vec<ToolOutcome> {
            calls.iter().map(&self.0).collect()
        }
    }

    struct CommandFixtureDispatcher;

    #[async_trait]
    impl ToolDispatcher for CommandFixtureDispatcher {
        async fn execute_reads(
            &self,
            _run_id: Uuid,
            _calls: &[PreparedToolCall],
        ) -> Vec<ToolOutcome> {
            Vec::new()
        }

        async fn execute_command(
            &self,
            _run_id: Uuid,
            command: &PreparedCommandProposal,
        ) -> CommandOutcome {
            assert_eq!(command.command, "df -h");
            CommandOutcome {
                command_id: command.command_id,
                success: true,
                exit_code: Some(0),
                stdout: "/dev/sda2 100G 94G 6G 94% /".into(),
                stderr: String::new(),
                error_code: None,
                duration_ms: 2,
                cancelled: false,
            }
        }
    }

    struct VerificationCommandFixtureDispatcher;

    #[async_trait]
    impl ToolDispatcher for VerificationCommandFixtureDispatcher {
        async fn execute_reads(
            &self,
            _run_id: Uuid,
            _calls: &[PreparedToolCall],
        ) -> Vec<ToolOutcome> {
            Vec::new()
        }

        async fn execute_command(
            &self,
            _run_id: Uuid,
            command: &PreparedCommandProposal,
        ) -> CommandOutcome {
            let stdout = match command.command.as_str() {
                "systemctl restart nginx" => String::new(),
                "systemctl is-active nginx" => "active\n".into(),
                other => panic!("unexpected command: {other}"),
            };
            CommandOutcome {
                command_id: command.command_id,
                success: true,
                exit_code: Some(0),
                stdout,
                stderr: String::new(),
                error_code: None,
                duration_ms: 5,
                cancelled: false,
            }
        }
    }

    struct Harness {
        stores: AgentStores,
        cancel_rx: tokio::sync::watch::Receiver<bool>,
        target_id: Uuid,
        session_id: Uuid,
    }

    impl Harness {
        fn new() -> Self {
            let (_tx, cancel_rx) = tokio::sync::watch::channel(false);
            Self {
                stores: AgentStores {
                    runs: Arc::new(InMemoryAgentRunStore::default()),
                    events: Arc::new(InMemoryAgentEventRepository::default()),
                    approvals: Arc::new(InMemoryApprovalStore::default()),
                    checkpoints: Arc::new(InMemoryCheckpointStore::default()),
                    pending_calls: Arc::new(InMemoryPendingToolCallStore::default()),
                },
                cancel_rx,
                target_id: Uuid::new_v4(),
                session_id: Uuid::new_v4(),
            }
        }

        fn config(
            &self,
            gate: Arc<dyn AuthorizationGate>,
            changesets: Arc<dyn ChangeSetExecutor>,
        ) -> AgentControllerConfig {
            AgentControllerConfig {
                gate,
                policy_matcher: Arc::new(FixedPolicyMatcher { matches: true }),
                changesets,
                artifacts: Arc::new(InMemoryArtifactStore::default()),
                target_ids: vec![self.target_id],
                session_id: Some(self.session_id),
                budget: RunBudget::default(),
                cancellation: self.cancel_rx.clone(),
            }
        }

        fn build<R: Reasoner, D: ToolDispatcher>(
            &self,
            goal: &str,
            reasoner: R,
            dispatcher: D,
            gate: Arc<dyn AuthorizationGate>,
            changesets: Arc<dyn ChangeSetExecutor>,
        ) -> Result<AgentController<R, D>, AgentControllerError> {
            AgentController::new(
                goal,
                reasoner,
                dispatcher,
                self.stores.clone(),
                self.config(gate, changesets),
            )
        }

        fn events(&self, run_id: Uuid) -> Vec<&'static str> {
            self.stores
                .events
                .all_events(run_id)
                .expect("events")
                .iter()
                .map(|e| e.event.event_type())
                .collect()
        }
    }

    fn tool_calls(requests: &[(&str, serde_json::Value)]) -> AgentDecision {
        AgentDecision::ToolCalls(
            requests
                .iter()
                .map(|(tool_name, arguments)| ToolCallRequest {
                    tool_name: (*tool_name).into(),
                    arguments: arguments.clone(),
                    reason_summary: format!("Running {tool_name}"),
                })
                .collect(),
        )
    }

    fn success_outcome(call: &PreparedToolCall, detail: &str) -> ToolOutcome {
        ToolOutcome {
            tool_call_id: call.tool_call_id,
            tool_name: call.tool_name.clone(),
            success: true,
            summary: "ok".into(),
            error_code: None,
            duration_ms: 1,
            cancelled: false,
            from_cache: false,
            untrusted_remote_data: true,
            sanitized_data: Some(detail.into()),
            tool_result: None,
        }
    }

    fn failure_outcome(call: &PreparedToolCall, code: &str) -> ToolOutcome {
        ToolOutcome {
            tool_call_id: call.tool_call_id,
            tool_name: call.tool_name.clone(),
            success: false,
            summary: "failed".into(),
            error_code: Some(code.into()),
            duration_ms: 1,
            cancelled: false,
            from_cache: false,
            untrusted_remote_data: false,
            sanitized_data: None,
            tool_result: None,
        }
    }

    fn approval_gate() -> Arc<dyn AuthorizationGate> {
        Arc::new(FnAuthorizationGate(|call: &PreparedToolCall| {
            if call.tool_name == "service.logs" {
                Authorization::RequireApproval {
                    risk: "R1".into(),
                    policy_version: 1,
                    policy_hash: "policy".into(),
                }
            } else {
                Authorization::Auto
            }
        }))
    }

    struct NoopChangeSetExecutor;
    #[async_trait]
    impl ChangeSetExecutor for NoopChangeSetExecutor {
        async fn draft_and_check_policy(
            &self,
            _: Uuid,
            _: String,
            _: Vec<ChangeStepDraft>,
        ) -> Result<ChangeSet, ChangeSetExecutorError> {
            Err(ChangeSetExecutorError::InvalidOperation)
        }
        async fn approve(&self, _: Uuid, _: u64) -> Result<ChangeSet, ChangeSetExecutorError> {
            Err(ChangeSetExecutorError::InvalidOperation)
        }
        async fn execute(&self, _: Uuid, _: u64) -> Result<ChangeSet, ChangeSetExecutorError> {
            Err(ChangeSetExecutorError::InvalidOperation)
        }
        async fn verify(&self, _: Uuid, _: u64) -> Result<bool, ChangeSetExecutorError> {
            Ok(false)
        }
        async fn rollback(&self, _: Uuid, _: u64) -> Result<ChangeSet, ChangeSetExecutorError> {
            Err(ChangeSetExecutorError::InvalidOperation)
        }
    }

    /// §36-A: log path missing → tool failure is observation → alternate source.
    #[tokio::test]
    async fn scenario_a_log_not_found_agent_chooses_alternate_source() {
        let h = Harness::new();
        let mut c = h
            .build(
                "Find why nginx cannot start",
                FnReasoner(|input: &ReasonerInput<'_>| {
                    if input.observations.is_empty() {
                        return Ok(tool_calls(&[(
                            "service.logs",
                            json!({"service": "nginx", "lines": 50}),
                        )]));
                    }
                    let logs_failed = input
                        .observations
                        .iter()
                        .any(|o| o.tool_name.as_deref() == Some("service.logs") && !o.success);
                    let journal_ok = input
                        .observations
                        .iter()
                        .any(|o| o.tool_name.as_deref() == Some("docker.logs") && o.success);
                    if logs_failed && !journal_ok {
                        return Ok(tool_calls(&[(
                            "docker.logs",
                            json!({"container": "nginx", "lines": 50}),
                        )]));
                    }
                    Ok(AgentDecision::Final {
                        summary: "nginx logs unavailable; used container logs instead.".into(),
                    })
                }),
                FnDispatcher(|call: &PreparedToolCall| {
                    if call.tool_name == "service.logs" {
                        failure_outcome(call, "SFTP_NOT_FOUND")
                    } else {
                        success_outcome(call, "container stderr")
                    }
                }),
                Arc::new(AutoAuthorizationGate),
                Arc::new(NoopChangeSetExecutor),
            )
            .expect("controller");
        let outcome = c.run_to_interrupt().await.expect("run");
        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        assert!(!c.state().is_terminal() || c.state() == AgentRunStateV2::Completed);
        assert!(h.events(c.run_id()).contains(&"tool_failed"));
        assert!(h.events(c.run_id()).contains(&"tool_completed"));
    }

    /// §36-B / §39: approval interrupt → same run_id → resume → execute.
    #[tokio::test]
    async fn scenario_b_approval_interrupt_resume_same_run() {
        let h = Harness::new();
        let mut c = h
            .build(
                "Investigate nginx",
                FnReasoner(|input: &ReasonerInput<'_>| {
                    if input.observations.is_empty() {
                        Ok(tool_calls(&[(
                            "service.logs",
                            json!({"service": "nginx", "lines": 50}),
                        )]))
                    } else {
                        Ok(AgentDecision::Final {
                            summary: "done".into(),
                        })
                    }
                }),
                FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "logs")),
                approval_gate(),
                Arc::new(NoopChangeSetExecutor),
            )
            .expect("controller");
        let run_id = c.run_id();
        let RunOutcome::AwaitingApproval { approval_id, .. } =
            c.run_to_interrupt().await.expect("interrupt")
        else {
            panic!("expected approval");
        };
        let outcome = c.approve(approval_id).await.expect("approve");
        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        assert_eq!(c.run_id(), run_id);
    }

    /// §36-C: reject never executes; run continues.
    #[tokio::test]
    async fn scenario_c_reject_continues_safely() {
        let h = Harness::new();
        let mut c = h
            .build(
                "Investigate nginx",
                FnReasoner(|input: &ReasonerInput<'_>| {
                    if input
                        .observations
                        .iter()
                        .any(|o| o.summary.contains("rejected"))
                    {
                        return Ok(AgentDecision::Final {
                            summary: "alternative path".into(),
                        });
                    }
                    Ok(tool_calls(&[(
                        "service.logs",
                        json!({"service": "nginx", "lines": 50}),
                    )]))
                }),
                FnDispatcher(|_: &PreparedToolCall| panic!("must not execute")),
                approval_gate(),
                Arc::new(NoopChangeSetExecutor),
            )
            .expect("controller");
        let RunOutcome::AwaitingApproval { approval_id, .. } =
            c.run_to_interrupt().await.expect("interrupt")
        else {
            panic!("expected approval");
        };
        let outcome = c.reject(approval_id).await.expect("reject");
        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        assert!(h.events(c.run_id()).contains(&"approval_rejected"));
    }

    /// §36-D / §39: crash recovery — rehydrated controller resumes same run_id.
    #[tokio::test]
    async fn scenario_d_rehydrate_after_approval_interrupt() {
        let h = Harness::new();
        let gate = approval_gate();
        let changesets = Arc::new(NoopChangeSetExecutor);
        let mut c = h
            .build(
                "Investigate nginx",
                FnReasoner(|input: &ReasonerInput<'_>| {
                    if input.observations.is_empty() {
                        Ok(tool_calls(&[(
                            "service.logs",
                            json!({"service": "nginx", "lines": 50}),
                        )]))
                    } else {
                        Ok(AgentDecision::Final {
                            summary: "recovered".into(),
                        })
                    }
                }),
                FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "logs")),
                gate.clone(),
                changesets.clone(),
            )
            .expect("controller");
        let run_id = c.run_id();
        let RunOutcome::AwaitingApproval { approval_id, .. } =
            c.run_to_interrupt().await.expect("interrupt")
        else {
            panic!("expected approval");
        };
        let run = h.stores.runs.get(run_id).expect("get").expect("run");
        let _events = h.stores.events.all_events(run_id).expect("events");
        let goal = "Investigate nginx".to_string();
        let observations = Vec::new();
        let mut rehydrated = AgentController::attach(
            run,
            goal,
            FnReasoner(|input: &ReasonerInput<'_>| {
                if input.observations.is_empty() {
                    Ok(tool_calls(&[(
                        "service.logs",
                        json!({"service": "nginx", "lines": 50}),
                    )]))
                } else {
                    Ok(AgentDecision::Final {
                        summary: "recovered".into(),
                    })
                }
            }),
            FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "logs")),
            h.stores.clone(),
            h.config(gate, changesets),
            observations,
            Vec::new(),
            1,
            0,
            10,
        )
        .expect("attach");
        let outcome = rehydrated.approve(approval_id).await.expect("approve");
        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        assert_eq!(rehydrated.run_id(), run_id);
    }

    /// §39: crash-recovered `Paused` state resumes on the same run_id.
    #[tokio::test]
    async fn scenario_e_crash_paused_run_resumes_same_run() {
        let h = Harness::new();
        let gate = Arc::new(AutoAuthorizationGate);
        let changesets = Arc::new(NoopChangeSetExecutor);
        let c = h
            .build(
                "Check disk usage",
                FnReasoner(|_: &ReasonerInput<'_>| {
                    Ok(tool_calls(&[("system.disk_usage", json!({}))]))
                }),
                FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "50% free")),
                gate.clone(),
                changesets.clone(),
            )
            .expect("controller");
        let run_id = c.run_id();
        let mut run = h.stores.runs.get(run_id).expect("get").expect("run");
        run.transition_to(AgentRunStateV2::Running)
            .expect("running");
        run.transition_to(AgentRunStateV2::Reasoning)
            .expect("reasoning");
        run.transition_to(AgentRunStateV2::Paused).expect("paused");
        h.stores.runs.save(&run).expect("save");
        let mut rehydrated = AgentController::attach(
            run,
            "Check disk usage".into(),
            FnReasoner(|input: &ReasonerInput<'_>| {
                if input.observations.is_empty() {
                    Ok(tool_calls(&[("system.disk_usage", json!({}))]))
                } else {
                    Ok(AgentDecision::Final {
                        summary: "disk ok".into(),
                    })
                }
            }),
            FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "50% free")),
            h.stores.clone(),
            h.config(gate, changesets),
            Vec::new(),
            Vec::new(),
            1,
            0,
            10,
        )
        .expect("attach");
        let outcome = rehydrated.resume().await.expect("resume");
        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        assert_eq!(rehydrated.run_id(), run_id);
        assert!(h.events(run_id).contains(&"run_resumed"));
    }

    /// §36 scenario 7: tool timeout → observation → alternate tool.
    #[tokio::test]
    async fn scenario_g_tool_timeout_recovery() {
        let h = Harness::new();
        let mut c = h
            .build(
                "Find nginx errors",
                FnReasoner(|input: &ReasonerInput<'_>| {
                    if input.observations.is_empty() {
                        return Ok(tool_calls(&[(
                            "service.logs",
                            json!({"service": "nginx", "lines": 100}),
                        )]));
                    }
                    if input
                        .observations
                        .iter()
                        .any(|o| o.tool_name.as_deref() == Some("docker.logs") && o.success)
                    {
                        return Ok(AgentDecision::Final {
                            summary: "recovered via docker".into(),
                        });
                    }
                    if input
                        .observations
                        .iter()
                        .any(|o| o.error_code.as_deref() == Some("EXEC_TIMED_OUT"))
                    {
                        return Ok(tool_calls(&[(
                            "docker.logs",
                            json!({"container": "nginx", "lines": 100}),
                        )]));
                    }
                    Ok(AgentDecision::Final {
                        summary: "recovered via docker".into(),
                    })
                }),
                FnDispatcher(|call: &PreparedToolCall| {
                    if call.tool_name == "service.logs" {
                        failure_outcome(call, "EXEC_TIMED_OUT")
                    } else {
                        success_outcome(call, "docker logs")
                    }
                }),
                Arc::new(AutoAuthorizationGate),
                Arc::new(NoopChangeSetExecutor),
            )
            .expect("controller");
        let outcome = c.run_to_interrupt().await.expect("run");
        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        assert!(c.metrics().recovery_attempts >= 1);
    }

    /// §39: stale binding after argument mutation must not execute.
    #[tokio::test]
    async fn scenario_f_stale_approval_does_not_execute() {
        let h = Harness::new();
        let mut c = h
            .build(
                "Investigate nginx",
                FnReasoner(|input: &ReasonerInput<'_>| {
                    if input.observations.iter().any(|o| {
                        o.error_code
                            .as_deref()
                            .is_some_and(|c| c.contains("ARGUMENTS_CHANGED"))
                    }) {
                        return Ok(AgentDecision::Final {
                            summary: "replanned".into(),
                        });
                    }
                    Ok(tool_calls(&[(
                        "service.logs",
                        json!({"service": "nginx", "lines": 100}),
                    )]))
                }),
                FnDispatcher(|_: &PreparedToolCall| panic!("must not execute stale approval")),
                approval_gate(),
                Arc::new(NoopChangeSetExecutor),
            )
            .expect("controller");
        let RunOutcome::AwaitingApproval {
            approval_id,
            tool_call_id,
            ..
        } = c.run_to_interrupt().await.expect("interrupt")
        else {
            panic!("expected approval");
        };
        h.stores.pending_calls.remove(tool_call_id).expect("remove");
        let mut swapped = validate_decision(tool_calls(&[(
            "service.logs",
            json!({"service": "nginx", "lines": 200}),
        )]))
        .expect("validates");
        let ValidatedDecision::ToolCalls(ref mut calls) = swapped else {
            panic!("calls")
        };
        calls[0].tool_call_id = tool_call_id;
        h.stores.pending_calls.put(calls[0].clone()).expect("put");
        let outcome = c.approve(approval_id).await.expect("approve attempt");
        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        assert!(h.events(c.run_id()).contains(&"approval_invalidated"));
    }

    /// §39: budget exhaustion is a stable terminal failure, not a panic.
    #[tokio::test]
    async fn scenario_h_budget_exhaustion_fails_closed() {
        let h = Harness::new();
        let mut config = h.config(
            Arc::new(AutoAuthorizationGate),
            Arc::new(NoopChangeSetExecutor),
        );
        config.budget.max_reasoner_rounds = 1;
        let mut c = AgentController::new(
            "loop forever",
            FnReasoner(|_: &ReasonerInput<'_>| Ok(tool_calls(&[("system.disk_usage", json!({}))]))),
            FnDispatcher(|call: &PreparedToolCall| success_outcome(call, "50%")),
            h.stores.clone(),
            config,
        )
        .expect("controller");
        let outcome = c.run_to_interrupt().await.expect("run");
        assert_eq!(
            outcome,
            RunOutcome::Failed {
                error_code: AGENT_BUDGET_EXCEEDED
            }
        );
    }

    #[tokio::test]
    async fn disk_diagnosis_proposes_command_and_waits_for_approval() {
        let h = Harness::new();
        let rounds = Arc::new(AtomicUsize::new(0));
        let observed_rounds = rounds.clone();
        let session_id = h.session_id;
        let target_id = h.target_id;
        let mut controller = h
            .build(
                "Check disk usage and diagnose issues.",
                FnReasoner(move |input: &ReasonerInput<'_>| {
                    observed_rounds.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(input.session_id, Some(session_id));
                    assert_eq!(input.target_ids, &[target_id]);
                    if input.observations.is_empty() {
                        return Ok(AgentDecision::CommandProposal(CommandProposalRequest {
                            command: "df -h".into(),
                            reason_summary: "Inspect filesystem usage".into(),
                            observation_analysis: None,
                        }));
                    }
                    assert!(input.observations.iter().any(|observation| {
                        observation.tool_name.as_deref() == Some("agent.command")
                            && observation.success
                            && observation.detail.as_deref().is_some_and(|detail| detail.contains("94%"))
                    }));
                    Ok(AgentDecision::Final {
                        summary: "Root filesystem usage is 94%; investigate its largest directories next."
                            .into(),
                    })
                }),
                CommandFixtureDispatcher,
                Arc::new(AutoAuthorizationGate),
                Arc::new(NoopChangeSetExecutor),
            )
            .expect("controller");
        let awaiting = controller.run_to_interrupt().await.expect("run");
        let RunOutcome::AwaitingApproval { approval_id, .. } = awaiting else {
            panic!("expected command approval interrupt");
        };
        assert!(!h.events(controller.run_id()).contains(&"command_started"));
        let outcome = controller
            .approve(approval_id)
            .await
            .expect("approve command");
        let RunOutcome::Completed { summary } = outcome else {
            panic!("expected completed diagnosis");
        };
        assert_eq!(rounds.load(Ordering::SeqCst), 2);
        assert!(!summary.contains("please run"));
        let events = h.events(controller.run_id());
        assert!(events.contains(&"command_completed"));
        assert!(events.contains(&"command_analysis_updated"));
    }

    #[tokio::test]
    async fn command_result_causes_second_reasoning_round() {
        let h = Harness::new();
        let rounds = Arc::new(AtomicUsize::new(0));
        let observed_rounds = rounds.clone();
        let mut controller = h
            .build(
                "Check disk usage and diagnose issues.",
                FnReasoner(move |input: &ReasonerInput<'_>| {
                    let round = observed_rounds.fetch_add(1, Ordering::SeqCst) + 1;
                    match round {
                        1 => {
                            assert!(input.observations.is_empty());
                            Ok(AgentDecision::CommandProposal(CommandProposalRequest {
                                command: "df -h".into(),
                                reason_summary: "Inspect filesystem usage".into(),
                                observation_analysis: None,
                            }))
                        }
                        2 => {
                            assert_eq!(input.round, 2);
                            assert_eq!(input.observations.len(), 1);
                            assert_eq!(
                                input.observations[0].tool_name.as_deref(),
                                Some("agent.command")
                            );
                            Ok(AgentDecision::Final {
                                summary: "Disk evidence was processed in round two.".into(),
                            })
                        }
                        _ => panic!("unexpected extra reasoning round"),
                    }
                }),
                CommandFixtureDispatcher,
                Arc::new(AutoAuthorizationGate),
                Arc::new(NoopChangeSetExecutor),
            )
            .expect("controller");
        let awaiting = controller.run_to_interrupt().await.expect("run");
        let RunOutcome::AwaitingApproval { approval_id, .. } = awaiting else {
            panic!("expected approval");
        };
        let outcome = controller.approve(approval_id).await.expect("approve");
        assert!(matches!(outcome, RunOutcome::Completed { .. }));
        assert_eq!(rounds.load(Ordering::SeqCst), 2);
        assert_eq!(
            h.events(controller.run_id())
                .into_iter()
                .filter(|event| *event == "reasoning_started")
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn mutating_command_requires_successful_follow_up_verification() {
        let h = Harness::new();
        let rounds = Arc::new(AtomicUsize::new(0));
        let observed_rounds = rounds.clone();
        let mut controller = h
            .build(
                "Restart nginx and verify it is healthy.",
                FnReasoner(move |input: &ReasonerInput<'_>| {
                    let round = observed_rounds.fetch_add(1, Ordering::SeqCst) + 1;
                    match round {
                        1 => Ok(AgentDecision::CommandProposal(CommandProposalRequest {
                            command: "systemctl restart nginx".into(),
                            reason_summary: "Restart nginx".into(),
                            observation_analysis: None,
                        })),
                        2 => Ok(AgentDecision::Final {
                            summary: "Restarted.".into(),
                        }),
                        3 => {
                            assert!(input.observations.iter().any(|observation| {
                                observation.error_code.as_deref()
                                    == Some("COMMAND_VERIFICATION_REQUIRED")
                            }));
                            Ok(AgentDecision::CommandProposal(CommandProposalRequest {
                                command: "systemctl is-active nginx".into(),
                                reason_summary: "Verify nginx is active".into(),
                                observation_analysis: Some(
                                    "The restart command completed; service health still needs verification."
                                        .into(),
                                ),
                            }))
                        }
                        4 => {
                            assert!(input.observations.iter().any(|observation| {
                                observation.success
                                    && observation
                                        .detail
                                        .as_deref()
                                        .is_some_and(|detail| detail.contains("active"))
                            }));
                            Ok(AgentDecision::Final {
                                summary: "Nginx restarted and is active.".into(),
                            })
                        }
                        _ => panic!("unexpected extra reasoning round"),
                    }
                }),
                VerificationCommandFixtureDispatcher,
                Arc::new(AutoAuthorizationGate),
                Arc::new(NoopChangeSetExecutor),
            )
            .expect("controller");

        let first = controller.run_to_interrupt().await.expect("first approval");
        let RunOutcome::AwaitingApproval {
            approval_id: first_approval,
            ..
        } = first
        else {
            panic!("expected restart approval");
        };
        let second = controller.approve(first_approval).await.expect("restart");
        let RunOutcome::AwaitingApproval {
            approval_id: verification_approval,
            ..
        } = second
        else {
            panic!("expected verification approval");
        };
        let completed = controller
            .approve(verification_approval)
            .await
            .expect("verification");
        assert_eq!(
            completed,
            RunOutcome::Completed {
                summary: "Nginx restarted and is active.".into()
            }
        );
        assert_eq!(rounds.load(Ordering::SeqCst), 4);
    }
}
