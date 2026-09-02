import { Channel, invoke } from "@tauri-apps/api/core";
import type { AgentEventEnvelope, AgentV2ResumableRun, AgentV2StartResponse } from "../../types/agent-v2";

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

export const approveAgentV2Run = (runId: string) =>
  invoke<void>("agent_v2_run_approve", { request: { runId } });

export const rejectAgentV2Run = (runId: string) =>
  invoke<void>("agent_v2_run_reject", { request: { runId } });

export const replyAgentV2Run = (runId: string, text: string) =>
  invoke<void>("agent_v2_run_reply", { request: { runId, text } });

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
