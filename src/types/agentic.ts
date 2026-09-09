export type RiskLevel = "R0" | "R1" | "R2" | "R3" | "R4";
export type AgentRunState = "gathering-context" | "investigating" | "diagnosing" | "needs-input" | "succeeded" | "failed" | "cancelled" | "timed-out" | "policy-blocked" | "budget-exceeded";
export type AgentProgress = { stage: "gathering-context" | "planning" | "running-tools" | "drafting-change-set" | "complete"; toolNames: string[] };
export type AgentEvidence = { id: string; source: string; invocationId: string; trust: string; summary: string; result: { invocationId: string; toolName: string; success: boolean; errorCode: string | null; durationMs: number; data: unknown } };
export type AgentRunMetrics = { runId: string; incidentId: string | null; modelCalls: number; inputTokens: number; outputTokens: number; toolCalls: number; duplicateCalls: number; mcpCalls: number; contextSizeBytes: number; compactionCount: number; durationMs: number; diagnosisLatencyMs: number | null; resolutionLatencyMs: number | null; verificationResult: string | null; rollbackResult: string | null; estimatedCostMicrousd: number | null; cacheHits: number; parallelReadBatches: number };
export type AgentRun = { id: string; model: string; sessionId: string; state: AgentRunState; activities: { invocationId: string; toolName: string; success: boolean; errorCode: string | null; durationMs: number }[]; evidence: AgentEvidence[]; externalEvidence: { id: string; source: string; trust: string; data: unknown }[]; diagnosis: { id: string; titleCode: string; rootCauseCode: string; confidence: number; evidenceIds: string[]; recommendedActionCode: string; risk: RiskLevel } | null; answer: string | null; answerEvidenceIds: string[]; goalAchieved: boolean; clarificationQuestion: string | null; failureCode: string | null; changeSet: ChangeSet | null; maxToolCalls: number; usedToolCalls: number; maxModelTokens: number; usedModelTokens: number; metrics: AgentRunMetrics };
export type ModelProviderKind =
  | "local"
  | "runory-managed"
  | "deep-seek"
  | "glm"
  | "open-ai-compatible"
  | "chat-gpt"
  | "open-router"
  | "open-ai"
  | "anthropic"
  | "google"
  | "qwen"
  | "kimi"
  | "minimax";
export type ModelAuthMode = "none" | "account" | "api-key" | "oauth";
export type OauthProvider = "chat-gpt" | "open-router";
export type AdvancedProviderKind = Exclude<ModelProviderKind, "local" | "runory-managed" | "chat-gpt">;
export type ModelProviderStatus = {
  kind: ModelProviderKind;
  name: string;
  baseUrl: string;
  model: string;
  maxContextTokens: number;
  organizationId: string | null;
  apiKeyConfigured: boolean;
  authMode: ModelAuthMode;
  oauthInProgress: boolean;
  connectedAccountLabel: string | null;
};
export type ModelProfile = ModelProviderStatus & { id: string; active: boolean };
export type ModelConfigureRequest = {
  kind: ModelProviderKind;
  name: string;
  baseUrl: string;
  model: string;
  maxContextTokens: number;
  organizationId: string | null;
  apiKey: string | null;
};
export type DoctorRequest = { runId: string; sessionId: string; userRequest: string; service: string | null; httpUrl: string | null; portHost: string | null; port: number | null; includeNginxTest: boolean; skillId: string | null; mcpContext: { serverId: string; toolName: string; arguments: Record<string, unknown> } | null; incidentId?: string | null; budget?: { maxModelCalls: number; maxToolCalls: number; maxInputTokens: number; maxOutputTokens: number; timeBudgetMs: number; maxCostMicrousd: number | null; context: { maxItems: number; maxBytes: number; maxTokens: number } } | null };
export type PolicyDecision = "ALLOW" | "REQUIRE_APPROVAL" | "REQUIRE_STEP_APPROVAL" | "DENY";
export type PolicyScope = { kind: "global" } | { kind: "environment"; environment: string } | { kind: "group"; groupId: string } | { kind: "server"; serverId: string } | { kind: "tool"; tool: string };
export type PolicyEvaluation = { decision: PolicyDecision; matchedRules: { ruleId: string; scope: PolicyScope; decision: PolicyDecision; reason: string }[]; reason: string; scope: PolicyScope; policyVersion: number; policyHash: string };
export type PolicySnapshot = { policyVersion: number; policyHash: string; decision: PolicyDecision; matchedRuleIds: string[] };
export type ChangeSet = { id: string; agentRunId: string; sessionId: string; title: string; version: number; risk: RiskLevel; approvalState: "draft" | "approved" | "rejected" | "invalidated"; approvedVersion: number | null; approvedStepIds: string[]; executionState: "not-started" | "executing" | "committed" | "failed" | "rolled-back" | "rollback-failed" | "interrupted"; recoveryState: "live" | "metadata-only"; preconditions: { stepId: string; targetId: string; checkTool: string; observedAtEpochMs: number; state: "captured" | "changed" }[]; policyEvaluation: PolicyEvaluation | null; policySnapshot: PolicySnapshot | null; steps: { id: string; order: number; toolName: string; risk: RiskLevel; preview: string; verificationPlanCode: string; rollbackCapability: string; state: string; errorCode: string | null }[] };
export type EffectiveAgentPolicy = { target: { serverId: string; groupId: string | null; environment: string | null }; policyVersion: number; policyHash: string; evaluations: PolicyEvaluation[] };
export type Skill = { manifest: { id: string; version: string; publisher: string; requiredTools: string[]; optionalTools: string[]; riskCeiling: RiskLevel }; origin: "built-in" | "user"; enabled: boolean; permissionReviewCodes: string[] };
export type McpServerConfig = { id: string; label: string; endpoint: string; connected: boolean; enabled: boolean; transportEra: "modern" | "legacy" | null; protocolVersion: string | null; tools: { name: string; description: string; readOnly: boolean; enabled: boolean; requiresArguments: boolean }[] };
export type MultiServerRun = { id: string; targets: AgentRun[]; drift: { field: string; values: { sessionId: string; value: unknown }[] }[] };
export type OperationsPack = "website" | "nginx" | "docker" | "disk" | "service";
export type IncidentRequest = { id: string; pack: OperationsPack; severity: "sev1" | "sev2" | "sev3" | "sev4"; targets: string[]; symptoms: string[]; host: string | null; port: number | null; url: string | null; service: string | null; container: string | null; configPath: string | null; upstreamHost: string | null; upstreamPort: number | null; dependencies: string[] };
export type Incident = {
  id: string; agentRunId: string; model: string; pack: OperationsPack; status: string; severity: string; targets: string[]; symptoms: string[];
  evidence: { id: string; targetId: string; source: string; summary: string; result: { toolName: string; success: boolean; errorCode: string | null; durationMs: number; data: unknown } }[];
  evidenceReferences: { id: string; targetId: string; source: string; success: boolean; errorCode: string | null }[];
  comparisons: { dimension: string; drift: boolean; values: { targetId: string; signal: string }[] }[];
  hypotheses: { code: string; evidenceIds: string[]; supported: boolean }[];
  rootCause: { code: string; confidence: number; evidenceIds: string[] };
  proposedFix: { code: string; risk: RiskLevel; actionCodes: string[]; changeSetRequired: boolean; executionPipeline: string[]; dangerousDeletionBlocked: boolean };
  changeSet: { kind: "single" | "fleet"; id: string; version: number; exactTargetIds: string[] } | null;
  verification: { statusCode: string; evidenceIds: string[] }; resolution: { code: string; resolvedAtEpochMs: number } | null;
  handoff: { owner: string; summary: string; handedOffAtEpochMs: number } | null;
  timeline: { status: string; code: string; occurredAtEpochMs: number }[]; durationMs: number; recoveryState: "live" | "metadata-only";
  metrics: AgentRunMetrics;
};
export type IncidentAuditExport = Omit<Incident, "symptoms" | "evidence" | "hypotheses" | "handoff" | "recoveryState"> & { schemaVersion: number; contentOmitted: true };
