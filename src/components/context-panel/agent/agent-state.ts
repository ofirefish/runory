import type { AgentRun, Incident } from "../../../types/agentic";

export type AgentStatusState =
  | "idle"
  | "routing"
  | "investigating"
  | "diagnosing"
  | "question"
  | "done"
  | "failed"
  | "approval"
  | "cancelled";

/**
 * Structured conversation blocks. Not a plain chat bubble list:
 * user intent, status, ActivityTimeline (run), DiagnosisCard (incident),
 * thinking question, and repair proposal are distinct blocks.
 */
export type AgentConversationItem =
  | { kind: "user"; id: string; text: string; at: number }
  | { kind: "status"; id: string; state: AgentStatusState; text: string; at: number }
  | { kind: "run"; id: string; run: AgentRun; at: number }
  | { kind: "incident"; id: string; incident: Incident; at: number }
  | { kind: "question"; id: string; text: string; answered: boolean; at: number }
  | { kind: "repair"; id: string; runId: string; changeRisk: string; changeCount: number; at: number };

/** React owns conversation UI state only — orchestration stays in Rust. */
export type AgentConversationState = {
  /** Conversations keyed by server profile id, plus the active one. */
  byServer: Record<string, AgentConversationItem[]>;
  expanded: Record<string, boolean>;
  activeRunId: string | null;
  running: boolean;
  lastError: string | null;
  pendingQuestion: string | null;
};
