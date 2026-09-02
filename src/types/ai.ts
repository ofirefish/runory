export type AiTask = "explain-command" | "generate-command" | "diagnose-output" | "propose-fix";
export type AiRisk = "low" | "medium" | "high" | "critical";
export type AiPurpose = "disk-usage" | "memory-usage" | "process-list" | "network-sockets" | "read-logs" | "container-list" | "service-status" | "package-management" | "file-inspection" | "file-mutation" | "system-control" | "unknown";
export type AiSignal = "read-only" | "elevated-privileges" | "destructive-file-operation" | "network-download" | "shell-pipeline" | "output-redirection" | "service-mutation" | "container-mutation";
export type AiDiagnosis = "permission-denied" | "command-not-found" | "disk-full" | "port-in-use" | "connection-refused" | "out-of-memory" | "resource-not-found" | "authentication-failed" | "timed-out" | "unknown";

export type AiCommandProposal = {
  command: string;
  risk: AiRisk;
  purpose: AiPurpose;
  requiresConfirmation: boolean;
};

export type AiAssistantResponse = {
  task: AiTask;
  provider: string;
  risk: AiRisk;
  purpose: AiPurpose;
  signals: AiSignal[];
  diagnosis: AiDiagnosis | null;
  contextUsed: boolean;
  proposals: AiCommandProposal[];
};

/** A multi-command, review-first execution plan. Nothing executes until the
 *  user confirms each command individually. */
export type AiPlanProposal = {
  summary: string;
  commands: AiCommandProposal[];
};
