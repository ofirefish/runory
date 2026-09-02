import type { AgentEventEnvelope, AgentRunStateV2, AgentTimelineView, TimelineApproval } from "../../../types/agent-v2";

const TERMINAL: AgentRunStateV2[] = ["completed", "failed", "cancelled"];
const RUNNING_STATES: AgentRunStateV2[] = ["running", "reasoning", "acting"];
export const AGENT_MAX_ROUNDS = 50;

export type AgentTimelinePhase = "thinking" | "awaiting_approval" | "executing" | "analyzing" | "completed" | "failed" | "cancelled";

export function mergeAgentEvent(timeline: AgentTimelineView, envelope: AgentEventEnvelope): AgentTimelineView {
  if (timeline.events.some((item) => item.seq === envelope.seq)) return timeline;
  const events = [...timeline.events, envelope].sort((left, right) => left.seq - right.seq);
  let next: AgentTimelineView = { ...timeline, events, lastErrorCode: null };
  const type = envelope.event.type;
  const payload = envelope.event.payload ?? {};

  if (type === "progress_updated" && typeof payload.summary === "string") {
    next = { ...next, currentApproach: payload.summary };
  }
  if (type === "change_set_proposed" && typeof payload.change_set_id === "string") {
    next = { ...next, pendingChangeSetId: payload.change_set_id };
  }
  if (type === "user_input_required" && typeof payload.question === "string") {
    next = { ...next, pendingQuestion: payload.question };
  }
  if (type === "user_input_received") {
    next = { ...next, pendingQuestion: undefined };
  }
  if (type === "tool_approval_required") {
    const toolCallId = String(payload.tool_call_id ?? "");
    next = {
      ...next,
      pendingApproval: {
        approvalId: toolCallId,
        toolCallId,
        toolName: String(payload.tool_name ?? "tool"),
        kind: next.pendingChangeSetId ? "change_set" : "tool",
        changeSetId: next.pendingChangeSetId,
      },
    };
  }
  if (type === "command_approval_required") {
    const commandId = String(payload.command_id ?? "");
    const proposal = commandProposal(events, commandId);
    next = {
      ...next,
      pendingApproval: {
        approvalId: String(payload.approval_id ?? ""),
        toolCallId: commandId,
        toolName: "agent.command",
        kind: "command",
        command: proposal?.command,
        reason: proposal?.reason,
        risk: proposal?.risk,
        mutability: proposal?.mutability as TimelineApproval["mutability"],
      },
    };
  }
  if (type === "approval_granted" || type === "approval_rejected" || type === "approval_invalidated") {
    next = { ...next, pendingApproval: undefined, pendingChangeSetId: undefined };
  }
  if (type === "run_failed" && typeof payload.error_code === "string") {
    next = { ...next, lastErrorCode: payload.error_code, running: false };
  }
  if (type === "run_completed" || type === "run_cancelled") {
    next = { ...next, running: false, pendingApproval: undefined, pendingChangeSetId: undefined, pendingQuestion: undefined };
  }
  if (type === "run_started" || type === "run_resumed" || type === "reasoning_started") {
    next = { ...next, running: true };
  }
  if (type === "run_paused") {
    next = { ...next, running: false };
  }
  return next;
}

export function deriveRunState(events: AgentEventEnvelope[]): AgentRunStateV2 | null {
  let approvalSettled = false;
  let userInputSettled = false;
  for (let index = events.length - 1; index >= 0; index -= 1) {
    const type = events[index].event.type;
    if (type === "run_completed") return "completed";
    if (type === "run_failed") return "failed";
    if (type === "run_cancelled") return "cancelled";
    if (type === "approval_granted" || type === "approval_rejected" || type === "approval_invalidated") approvalSettled = true;
    if ((type === "tool_approval_required" || type === "command_approval_required") && !approvalSettled) return "awaiting_approval";
    if (type === "user_input_received") userInputSettled = true;
    if (type === "user_input_required" && !userInputSettled) return "awaiting_user";
    if (type === "run_paused") return "paused";
  }
  if (events.some((item) => item.event.type === "tool_started" || item.event.type === "command_started")) return "acting";
  if (events.some((item) => item.event.type === "reasoning_started")) return "reasoning";
  if (events.some((item) => item.event.type === "run_started")) return "running";
  return events.length > 0 ? "running" : null;
}

export function isTerminalState(state: AgentRunStateV2 | null): boolean {
  return state !== null && TERMINAL.includes(state);
}

export function isRunningState(state: AgentRunStateV2 | null, runningFlag: boolean): boolean {
  if (runningFlag) return true;
  return state !== null && RUNNING_STATES.includes(state);
}

export function reasoningRound(events: AgentEventEnvelope[]): number {
  return Math.max(1, events.filter((item) => item.event.type === "reasoning_started").length);
}

export function timelinePhase(events: AgentEventEnvelope[]): AgentTimelinePhase {
  const state = deriveRunState(events);
  if (state === "completed") return "completed";
  if (state === "failed") return "failed";
  if (state === "cancelled") return "cancelled";
  if (state === "awaiting_approval") return "awaiting_approval";
  for (let index = events.length - 1; index >= 0; index -= 1) {
    const type = events[index].event.type;
    if (type === "assistant_message_added") return "completed";
    if (type === "reasoning_started") {
      const hasResult = events.slice(0, index).some((item) => item.event.type === "command_completed" || item.event.type === "command_failed");
      return hasResult ? "analyzing" : "thinking";
    }
    if (type === "command_completed" || type === "command_failed") return "analyzing";
    if (type === "command_started" || type === "approval_granted") return "executing";
  }
  return "thinking";
}

export function runElapsedSeconds(events: AgentEventEnvelope[], nowEpochMs: number): number {
  const start = events.find((item) => item.event.type === "run_created")?.timestampEpochMs
    ?? events[0]?.timestampEpochMs;
  if (start === undefined) return 0;
  const terminal = [...events].reverse().find((item) => ["run_completed", "run_failed", "run_cancelled"].includes(item.event.type));
  const end = terminal?.timestampEpochMs ?? nowEpochMs;
  return Math.max(0, Math.floor((end - start) / 1000));
}

export function toolLabelKey(toolName: string): string {
  const map: Record<string, string> = {
    "system.disk_usage": "contextPanel.activity.diskUsage",
    "system.info": "contextPanel.timeline.systemInfo",
    "nginx.test": "contextPanel.activity.nginxTest",
    "http.request": "contextPanel.activity.httpCheck",
    "service.status": "contextPanel.activity.serviceStatus",
    "service.logs": "contextPanel.activity.logRead",
    "docker.list": "contextPanel.activity.dockerList",
    "process.list": "contextPanel.activity.processList",
  };
  return map[toolName] ?? "contextPanel.timeline.genericTool";
}

export function pendingApprovalFromEvents(events: AgentEventEnvelope[]): TimelineApproval | undefined {
  let pending: TimelineApproval | undefined;
  let pendingChangeSetId: string | undefined;
  for (const envelope of events) {
    const payload = envelope.event.payload ?? {};
    if (envelope.event.type === "change_set_proposed" && typeof payload.change_set_id === "string") {
      pendingChangeSetId = payload.change_set_id;
    }
    if (envelope.event.type === "tool_approval_required") {
      pending = {
        approvalId: String(payload.tool_call_id ?? ""),
        toolCallId: String(payload.tool_call_id ?? ""),
        toolName: String(payload.tool_name ?? "tool"),
        kind: pendingChangeSetId ? "change_set" : "tool",
        changeSetId: pendingChangeSetId,
      };
    }
    if (envelope.event.type === "command_approval_required") {
      const commandId = String(payload.command_id ?? "");
      const proposal = commandProposal(events, commandId);
      pending = {
        approvalId: String(payload.approval_id ?? ""),
        toolCallId: commandId,
        toolName: "agent.command",
        kind: "command",
        command: proposal?.command,
        reason: proposal?.reason,
        risk: proposal?.risk,
        mutability: proposal?.mutability as TimelineApproval["mutability"],
      };
    }
    if (envelope.event.type === "approval_granted" || envelope.event.type === "approval_rejected" || envelope.event.type === "approval_invalidated") {
      pending = undefined;
      pendingChangeSetId = undefined;
    }
  }
  return pending;
}

function commandProposal(events: AgentEventEnvelope[], commandId: string): {
  command?: string;
  reason?: string;
  risk?: string;
  mutability?: string;
} | undefined {
  for (let index = events.length - 1; index >= 0; index -= 1) {
    const envelope = events[index];
    if (envelope.event.type !== "command_proposed") continue;
    const payload = envelope.event.payload ?? {};
    if (String(payload.command_id ?? "") !== commandId) continue;
    return {
      command: typeof payload.command === "string" ? payload.command : undefined,
      reason: typeof payload.reason === "string" ? payload.reason : undefined,
      risk: typeof payload.risk === "string" ? payload.risk : undefined,
      mutability: typeof payload.mutability === "string" ? payload.mutability : undefined,
    };
  }
  return undefined;
}

export function emptyTimeline(runId: string | null = null): AgentTimelineView {
  return {
    runId,
    runState: null,
    events: [],
    running: false,
    lastErrorCode: null,
  };
}
