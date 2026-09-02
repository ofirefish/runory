export type AgentRunStateV2 =
  | "created"
  | "running"
  | "reasoning"
  | "acting"
  | "awaiting_approval"
  | "awaiting_user"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled";

export type AgentRunV2 = {
  id: string;
  state: AgentRunStateV2;
  createdAtEpochMs: number;
  updatedAtEpochMs: number;
  nextEventSeq: number;
};

export type AgentEventType =
  | "run_created"
  | "run_started"
  | "run_resumed"
  | "run_paused"
  | "run_cancelled"
  | "run_completed"
  | "run_failed"
  | "user_message_added"
  | "assistant_message_added"
  | "reasoning_started"
  | "progress_updated"
  | "tool_requested"
  | "tool_auto_authorized"
  | "tool_approval_required"
  | "tool_started"
  | "tool_output_chunk"
  | "tool_completed"
  | "tool_failed"
  | "command_proposed"
  | "command_approval_required"
  | "command_started"
  | "command_completed"
  | "command_failed"
  | "command_analysis_updated"
  | "observation_added"
  | "facts_updated"
  | "user_input_required"
  | "user_input_received"
  | "approval_granted"
  | "approval_rejected"
  | "approval_invalidated"
  | "diagnosis_updated"
  | "root_cause_identified"
  | "change_set_proposed"
  | "change_set_approved"
  | "change_set_execution_started"
  | "change_set_execution_completed"
  | "verification_started"
  | "verification_completed"
  | "rollback_started"
  | "rollback_completed";

export type AgentEventPayload = Record<string, unknown>;

export type AgentEventEnvelope = {
  runId: string;
  seq: number;
  timestampEpochMs: number;
  event: {
    type: AgentEventType;
    payload?: AgentEventPayload;
  };
};

export type AgentV2ResumableRun = {
  run: AgentRunV2;
  goal: string;
  targetIds: string[];
};

export type AgentV2DisplayContext = {
  os: string;
  user: string;
  directory: string;
};

export type AgentV2StartResponse = {
  runId: string;
  context: AgentV2DisplayContext;
};

export type TimelineApproval = {
  approvalId: string;
  toolCallId: string;
  toolName: string;
  risk?: string;
  reason?: string;
  kind?: "tool" | "command" | "change_set";
  command?: string;
  mutability?: "read" | "mutating" | "unknown";
  changeSetId?: string;
};

export type AgentTimelineView = {
  runId: string | null;
  runState: AgentRunStateV2 | null;
  events: AgentEventEnvelope[];
  currentApproach?: string;
  pendingApproval?: TimelineApproval;
  pendingChangeSetId?: string;
  pendingQuestion?: string;
  displayContext?: AgentV2DisplayContext;
  running: boolean;
  lastErrorCode: string | null;
};
