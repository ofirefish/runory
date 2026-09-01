import type { AiRisk } from "./ai";
import type { DiskUsage, DockerContainer, ProcessInfo } from "./infrastructure";

export type AiTerminalPreset = "disk-usage" | "memory-usage" | "listening-ports" | "recent-errors";
export type AiToolName = "terminal-exec" | "file-read" | "file-write" | "system-metrics" | "process-list" | "docker-list" | "docker-restart" | "nginx-test" | "nginx-reload";
export type AiToolInput =
  | { tool: "terminal-exec"; preset: AiTerminalPreset }
  | { tool: "file-read"; path: string }
  | { tool: "file-write"; path: string; content: string }
  | { tool: "system-metrics" }
  | { tool: "process-list" }
  | { tool: "docker-list" }
  | { tool: "docker-restart"; container: string }
  | { tool: "nginx-test" }
  | { tool: "nginx-reload" };
export type AiToolSummary =
  | Exclude<AiToolInput, { tool: "file-write" }>
  | { tool: "file-write"; path: string; bytes: number };
export type AiStepStatus = "pending-approval" | "approved" | "running" | "succeeded" | "failed";
export type AiPlanStep = { id: string; sessionId: string; tool: AiToolSummary; risk: AiRisk; status: AiStepStatus };
export type AiAgentPlan = { id: string; goal: string; steps: AiPlanStep[] };
export type AiToolOutput =
  | { kind: "text"; value: string }
  | { kind: "file"; path: string; content: string }
  | { kind: "file-written"; path: string; bytes: number }
  | { kind: "metrics"; cpuUsagePercent: number; memoryUsedBytes: number; memoryTotalBytes: number; uptimeSeconds: number; networkReceivedBytes: number; networkTransmittedBytes: number; disks: DiskUsage[] }
  | { kind: "processes"; processes: ProcessInfo[] }
  | { kind: "containers"; containers: DockerContainer[] };
export type AiToolExecution = { plan: AiAgentPlan; stepId: string; output: AiToolOutput };
export type AiAuditRecord = { id: string; planId: string; stepId: string; profileId: string; tool: AiToolName; risk: AiRisk; target: string | null; startedAtEpochSeconds: number; succeeded: boolean; errorCode: string | null };
