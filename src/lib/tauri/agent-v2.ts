import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  AgentEventEnvelope,
  AgentHistoryDetail,
  AgentV2FleetTargetBinding,
  AgentV2FleetPlanDraftRequest,
  AgentV2FleetPromptDraftRequest,
  AgentV2FleetTargetRequest,
  AgentV2ResumableRun,
  AgentV2StartResponse,
  FleetRunV2,
  FleetApprovalV2,
  FleetEventEnvelopeV2,
  FleetInvestigationView,
  FleetChangeSetDraftRequest,
  FleetChangeSetReview,
  FleetMultiChangeSet,
} from "../../types/agent-v2";

export const listAgentHistory = (targetId: string | null) =>
  invoke<AgentV2ResumableRun[]>("agent_v2_history_list", { targetId });

export const getAgentHistory = (runId: string) =>
  invoke<AgentHistoryDetail>("agent_v2_history_get", { runId });

export const startAgentV2Run = (sessionId: string, goal: string) =>
  invoke<AgentV2StartResponse>("agent_v2_run_start", { request: { sessionId, goal } });

export const subscribeAgentV2Run = (
  runId: string,
  afterSeq: number,
  onEvent: (event: AgentEventEnvelope) => void,
) => {
  const onEventChannel = new Channel<AgentEventEnvelope>();
  onEventChannel.onmessage = onEvent;
  return invoke<void>("agent_v2_run_subscribe", {
    request: { runId, afterSeq },
    onEvent: onEventChannel,
  });
};

export const approveAgentV2Run = (runId: string, approvalId: string) =>
  invoke<void>("agent_v2_run_approve", { request: { runId, approvalId } });

export const rejectAgentV2Run = (runId: string, approvalId: string) =>
  invoke<void>("agent_v2_run_reject", { request: { runId, approvalId } });

export const replyAgentV2Run = (runId: string, text: string) =>
  invoke<void>("agent_v2_run_reply", { request: { runId, text } });

export const retryAgentV2Run = (runId: string) =>
  invoke<void>("agent_v2_run_retry", { request: { runId } });

export const cancelAgentV2Run = (runId: string) =>
  invoke<void>("agent_v2_run_cancel", { runId });

export const pauseAgentV2Run = (runId: string) =>
  invoke<void>("agent_v2_run_pause", { request: { runId } });

export const resumeAgentV2Run = (runId: string) =>
  invoke<void>("agent_v2_run_resume", { request: { runId } });

export const listResumableAgentV2Runs = () =>
  invoke<AgentV2ResumableRun[]>("agent_v2_list_resumable_runs");

export const bindResumableAgentV2Run = (runId: string, sessionId: string) =>
  invoke<void>("agent_v2_bind_resumable_run", { runId, sessionId });

export const validateAgentV2FleetTargets = (targets: AgentV2FleetTargetRequest[]) =>
  invoke<AgentV2FleetTargetBinding[]>("agent_v2_fleet_validate_targets", { request: { targets } });

export const draftAgentV2FleetPlan = (request: AgentV2FleetPlanDraftRequest) =>
  invoke<FleetRunV2>("agent_v2_fleet_plan_draft", { request });

export const draftAgentV2FleetPrompt = (request: AgentV2FleetPromptDraftRequest) =>
  invoke<FleetRunV2>("agent_v2_fleet_prompt_draft", { request });

export const getAgentV2FleetPlan = (fleetRunId: string) =>
  invoke<FleetRunV2>("agent_v2_fleet_plan_get", { fleetRunId });

export const getAgentV2FleetInvestigation = (fleetRunId: string) =>
  invoke<FleetInvestigationView>("agent_v2_fleet_investigation_get", { fleetRunId });

export const getLatestAgentV2FleetChangeSet = (fleetRunId: string) =>
  invoke<FleetChangeSetReview | null>("agent_v2_fleet_changeset_latest", { fleetRunId });

export const draftAgentV2FleetChangeSet = (request: FleetChangeSetDraftRequest) =>
  invoke<FleetChangeSetReview>("agent_v2_fleet_changeset_draft", { request });

export const approveAgentV2FleetChangeSet = (
  fleetRunId: string,
  fleetRunVersion: number,
  executionId: string,
  executionVersion: number,
) =>
  invoke<FleetMultiChangeSet>("agent_v2_fleet_changeset_approve", {
    request: { fleetRunId, fleetRunVersion, executionId, executionVersion },
  });

export const executeAgentV2FleetChangeSet = (
  fleetRunId: string,
  fleetRunVersion: number,
  executionId: string,
  executionVersion: number,
) =>
  invoke<FleetMultiChangeSet>("agent_v2_fleet_changeset_execute", {
    request: { fleetRunId, fleetRunVersion, executionId, executionVersion },
  });

export const rollbackAgentV2FleetChangeSet = (
  fleetRunId: string,
  fleetRunVersion: number,
  executionId: string,
  executionVersion: number,
) =>
  invoke<FleetMultiChangeSet>("agent_v2_fleet_changeset_rollback", {
    request: { fleetRunId, fleetRunVersion, executionId, executionVersion },
  });

export const listAgentV2FleetPlans = () =>
  invoke<FleetRunV2[]>("agent_v2_fleet_plan_list");

export const requestAgentV2FleetApproval = (fleetRunId: string, version: number) =>
  invoke<FleetApprovalV2>("agent_v2_fleet_plan_request_approval", {
    request: { fleetRunId, version },
  });

export const approveAgentV2FleetPlan = (fleetRunId: string, approvalId: string) =>
  invoke<FleetApprovalV2>("agent_v2_fleet_plan_approve", {
    request: { fleetRunId, approvalId },
  });

export const rejectAgentV2FleetPlan = (fleetRunId: string, approvalId: string) =>
  invoke<FleetApprovalV2>("agent_v2_fleet_plan_reject", {
    request: { fleetRunId, approvalId },
  });

export const getAgentV2FleetEvents = (fleetRunId: string, afterSeq = 0) =>
  invoke<FleetEventEnvelopeV2[]>("agent_v2_fleet_events_after", { fleetRunId, afterSeq });

export const startAgentV2FleetPlan = (
  fleetRunId: string,
  approvalId: string,
  capacity: number,
) =>
  invoke<string[]>("agent_v2_fleet_plan_start", {
    request: { fleetRunId, approvalId, capacity },
  });

export const pauseAgentV2FleetPlan = (fleetRunId: string) =>
  invoke<string[]>("agent_v2_fleet_plan_pause", { fleetRunId });

export const continueAgentV2FleetPlan = (fleetRunId: string, approvalId: string) =>
  invoke<string[]>("agent_v2_fleet_plan_continue", { request: { fleetRunId, approvalId } });

export const cancelAgentV2FleetPlan = (fleetRunId: string) =>
  invoke<string[]>("agent_v2_fleet_plan_cancel", { fleetRunId });
